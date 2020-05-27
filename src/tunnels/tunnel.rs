use super::Cmd;
use super::THeader;
use super::{Reqq, Request, TunStub};
use crate::config::KEEP_ALIVE_INTERVAL;
use crate::lws::{RMessage, WMessage};
use crate::service::SubServiceCtlCmd;
use crate::tunnels::theader::THEADER_SIZE;
use byte::*;
use bytes::Buf;
use bytes::BytesMut;
use log::{error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::net::SocketAddr;
use std::net::{
    IpAddr::{self, V4, V6},
    Ipv4Addr, Ipv6Addr,
};
use std::os::unix::io::RawFd;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

type UdpxTxType = UnboundedSender<SubServiceCtlCmd>;
pub struct Tunnel {
    pub tx: UnboundedSender<WMessage>,
    pub udpx_tx: Option<UdpxTxType>,
    pub index: usize,

    pub requests: Reqq,

    pub capacity: u16,
    rtt_queue: Vec<i64>,
    rtt_index: usize,
    rtt_sum: i64,
    req_count: u16,
    ping_count: u8,

    time: Instant,
    rawfd: RawFd,

    busy: usize,
    request_quota: u16,
}

impl Tunnel {
    pub fn new(
        tx: UnboundedSender<WMessage>,
        rawfd: RawFd,
        idx: usize,
        cap: usize,
        request_quota: u16,
    ) -> Tunnel {
        info!("[Tunnel]new Tunnel, idx:{}", idx);
        let size = 5;
        let rtt_queue = vec![0; size];

        let capacity = cap;
        Tunnel {
            tx: tx,
            udpx_tx: None,
            index: idx,
            requests: Reqq::new(capacity),
            capacity: capacity as u16,
            rtt_queue: rtt_queue,
            rtt_index: 0,
            rtt_sum: 0,
            req_count: 0,
            ping_count: 0,
            time: Instant::now(),
            rawfd: rawfd,
            busy: 0,
            request_quota,
        }
    }

    pub fn on_tunnel_msg(&mut self, mut msg: RMessage) {
        let bs = msg.buf.as_ref().unwrap();
        // info!("[Tunnel]on_tunnel_msg, len:{}", bs.len());

        let bs = &bs[2..]; // skip the length
        self.busy += bs.len();

        let offset = &mut 0;
        let cmd = bs.read_with::<u8>(offset, LE).unwrap();
        let bs = &bs[1..]; // skip cmd
        let cmd = Cmd::from(cmd);

        match cmd {
            Cmd::Ping => {
                self.reply_ping(msg);
            }
            Cmd::Pong => {
                self.on_pong(bs);
            }
            Cmd::UdpX => {
                self.on_udpx_south(msg);
            }
            Cmd::ReqData => {
                let th = THeader::read_from(&bs[..]);
                // data
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                let tx = self.get_request_tx(req_idx, req_tag);
                match tx {
                    None => {
                        info!(
                            "[Tunnel] {} no request found for: {}:{}",
                            self.index, req_idx, req_tag
                        );
                        return;
                    }
                    Some(tx) => {
                        // info!(
                        //     "[Tunnel] {} return request msg: {}:{}",
                        //     self.index, req_idx, req_tag
                        // );
                        let wmsg =
                            WMessage::new(msg.buf.take().unwrap(), (3 + THEADER_SIZE) as u16);
                        let result = tx.send(wmsg);
                        match result {
                            Err(e) => {
                                info!(
                                    "[Tunnel] {} tunnel msg send to request failed:{}",
                                    self.index, e
                                );
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Cmd::ReqServerFinished => {
                let th = THeader::read_from(&bs[..]);
                // server finished
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                self.free_request_tx(req_idx, req_tag);
            }
            Cmd::ReqServerClosed => {
                let th = THeader::read_from(&bs[..]);
                // server finished
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                // TODO: extract new method
                let reqs = &mut self.requests;
                let r = reqs.free(req_idx, req_tag);
                if r {
                    self.req_count -= 1;
                }
            }
            _ => {
                error!(
                    "[Tunnel]{} unsupport cmd:{:?}, discard msg",
                    self.index, cmd
                );
            }
        }
    }

    fn reply_ping(&mut self, mut msg: RMessage) {
        //info!("[Tunnel] reply_ping");
        let mut vec = msg.buf.take().unwrap();
        let bs = &mut vec[2..];
        let offset = &mut 0;
        bs.write_with::<u8>(offset, Cmd::Pong as u8, LE).unwrap();

        let wmsg = WMessage::new(vec, 0);
        let tx = &self.tx;
        let result = tx.send(wmsg);
        match result {
            Err(e) => {
                error!(
                    "[Tunnel]{} reply_ping tun send error:{}, tun_tx maybe closed",
                    self.index, e
                );
            }
            _ => {
                //info!("[Tunnel]on_dns_reply unbounded_send request msg",)
            }
        }
    }

    fn on_pong(&mut self, bs: &[u8]) {
        //info!("[Tunnel] on_pong");
        let len = bs.len();
        if len != 8 {
            error!("[Tunnel]{} pong data length({}) != 8", self.index, len);
            return;
        }

        // reset ping count
        self.ping_count = 0;

        let offset = &mut 0;
        let timestamp = bs.read_with::<u64>(offset, LE).unwrap();

        let in_ms = self.get_elapsed_milliseconds();
        assert!(
            in_ms >= timestamp,
            "[Tunnel] {} pong timestamp > now!",
            self.index
        );

        let rtt = in_ms - timestamp;
        let rtt = rtt as i64;
        self.append_rtt(rtt);
    }

    fn append_rtt(&mut self, rtt: i64) {
        let rtt_remove = self.rtt_queue[self.rtt_index];
        self.rtt_queue[self.rtt_index] = rtt;
        let len = self.rtt_queue.len();
        self.rtt_index = (self.rtt_index + 1) % len;

        self.rtt_sum = self.rtt_sum + rtt - rtt_remove;
    }

    // pub fn get_rtt(&self) -> i64 {
    //     let rtt_sum = self.rtt_sum;
    //     rtt_sum / (self.rtt_queue.len() as i64)
    // }
    pub fn reset_busy(&mut self) {
        self.busy = 0;
    }

    pub fn get_busy(&self) -> usize {
        return self.busy;
    }

    pub fn get_req_count(&self) -> u16 {
        self.req_count
    }

    fn get_elapsed_milliseconds(&self) -> u64 {
        let in_ms = self.time.elapsed().as_millis();
        in_ms as u64
    }

    fn get_request_tx(&self, req_idx: u16, req_tag: u16) -> Option<UnboundedSender<WMessage>> {
        let requests = &self.requests;
        let req_idx = req_idx as usize;
        if req_idx >= requests.elements.len() {
            return None;
        }

        let req = &requests.elements[req_idx];
        if req.tag == req_tag && req.request_tx.is_some() {
            match req.request_tx {
                None => {
                    return None;
                }
                Some(ref tx) => {
                    return Some(tx.clone());
                }
            }
        }

        None
    }

    fn free_request_tx(&mut self, req_idx: u16, req_tag: u16) {
        let requests = &mut self.requests;
        let req_idx = req_idx as usize;
        if req_idx >= requests.elements.len() {
            return;
        }

        let req = &mut requests.elements[req_idx];
        if req.tag == req_tag && req.request_tx.is_some() {
            info!(
                "[Tunnel]{} free_request_tx, req_idx:{}, req_tag:{}",
                self.index, req_idx, req_tag
            );
            req.request_tx = None;
        }
    }

    pub fn on_request_created(&mut self, req: Request, ip: IpAddr, port: u16) -> Option<TunStub> {
        info!("[Tunnel]{} on_request_created", self.index);

        let ts = self.on_request_created_internal(req);
        match ts {
            Some(ts) => {
                Tunnel::send_request_created_to_server(&ts, ip, port);

                Some(ts)
            }
            None => None,
        }
    }

    fn on_request_created_internal(&mut self, req: Request) -> Option<TunStub> {
        let reqs = &mut self.requests;
        let (idx, tag) = reqs.alloc(req);
        let tun_idx;
        if idx != std::u16::MAX {
            tun_idx = self.index as u16;
        } else {
            return None;
        }

        let tx = self.tx.clone();
        self.req_count += 1;

        Some(TunStub {
            tunnel_tx: tx,
            tun_idx: tun_idx,
            req_idx: idx,
            req_tag: tag,
            request_quota: self.request_quota,
        })
    }

    pub fn on_request_closed(&mut self, tunstub: &TunStub) {
        info!("[Tunnel]{} on_request_closed", self.index);
        let reqs = &mut self.requests;
        let r = reqs.free(tunstub.req_idx, tunstub.req_tag);

        if r {
            info!(
                "[Tunnel]on_request_closed, tun index:{}, sub req_count by 1",
                self.index
            );
            self.req_count -= 1;
            Tunnel::send_request_closed_to_server(tunstub);
        }
    }

    pub fn on_closed(&mut self) {
        // free all requests
        let reqs = &mut self.requests;
        reqs.clear_all();

        info!(
            "[Tunnel]{} tunnel live duration {} minutes",
            self.index,
            self.time.elapsed().as_secs() / 60
        );
    }

    pub fn send_ping(&mut self) -> bool {
        let ping_count = self.ping_count;
        if ping_count > 5 {
            return false;
        }

        let timestamp = self.get_elapsed_milliseconds();
        let mut bs1 = vec![0 as u8; 11]; // 2 bytes length, 1 byte cmd, 8 byte content
        let bs = &mut bs1[..];
        let offset = &mut 0;
        bs.write_with::<u16>(offset, 11, LE).unwrap();
        bs.write_with::<u8>(offset, Cmd::Ping as u8, LE).unwrap();
        bs.write_with::<u64>(offset, timestamp, LE).unwrap();

        let msg = WMessage::new(bs1, 0);
        let r = self.tx.send(msg);
        match r {
            Err(e) => {
                error!("[Tunnel]{} tunnel send_ping error:{}", self.index, e);
            }
            _ => {
                self.ping_count += 1;
                if ping_count > 0 {
                    // TODO: fix accurate RTT?
                    self.append_rtt((ping_count as i64) * (KEEP_ALIVE_INTERVAL as i64));
                }
            }
        }

        true
    }

    fn send_request_created_to_server(ts: &TunStub, ip: IpAddr, port: u16) {
        info!(
            "[Tunnel]{} send_request_created_to_server, target:{}:{}",
            ts.tun_idx, ip, port
        );

        let ipaddr_length;
        match ip {
            V4(_) => {
                ipaddr_length = 4;
            }
            V6(_) => {
                ipaddr_length = 16;
            }
        }

        // send request to server
        let content_size = 1 + ipaddr_length + 2; // ipv4 + port;
        let hsize = 2 + 1 + THEADER_SIZE;
        let total = hsize + content_size;
        let mut buf = vec![0; total];

        let bs = &mut buf[..];
        let offset = &mut 0;
        bs.write_with::<u16>(offset, total as u16, LE).unwrap(); // length
        bs.write_with::<u8>(offset, Cmd::ReqCreated as u8, LE)
            .unwrap(); // cmd

        let th = THeader::new(ts.req_idx, ts.req_tag); // header
        let msg_header = &mut buf[3..];
        th.write_to(msg_header);

        let msg_body = &mut buf[hsize..];
        *offset = 0;

        match ip {
            V4(v4) => {
                msg_body.write_with::<u8>(offset, 0, LE).unwrap(); // address type, ipv4
                let ipbytes = v4.octets();
                for b in ipbytes.iter() {
                    msg_body.write_with::<u8>(offset, *b, LE).unwrap(); // ip
                }
            }
            V6(v6) => {
                msg_body.write_with::<u8>(offset, 2, LE).unwrap(); // address type, ipv6
                let ipbytes = v6.segments();
                for b in ipbytes.iter() {
                    msg_body.write_with::<u16>(offset, *b, LE).unwrap(); // ip
                }
            }
        }

        msg_body.write_with::<u16>(offset, port, LE).unwrap(); // port

        // websocket message
        let wmsg = WMessage::new(buf, 0);

        // send to peer, should always succeed
        if let Err(e) = ts.tunnel_tx.send(wmsg) {
            error!(
                "[Tunnel]{} send_request_created_to_server tx send failed:{}",
                ts.tun_idx, e
            );
        }
    }

    fn send_request_closed_to_server(ts: &TunStub) {
        info!("[Tunnel]send_request_closed_to_server, {:?}", ts);

        // send request to server
        let content_size = 0;
        let hsize = 2 + 1 + THEADER_SIZE;
        let total = hsize + content_size;
        let mut buf = vec![0; total];

        let bs = &mut buf[..];
        let offset = &mut 0;
        bs.write_with::<u16>(offset, total as u16, LE).unwrap(); // length
        bs.write_with::<u8>(offset, Cmd::ReqClientClosed as u8, LE)
            .unwrap(); // cmd

        let th = THeader::new(ts.req_idx, ts.req_tag);
        let msg_header = &mut buf[3..];
        th.write_to(msg_header);

        // websocket message
        let wmsg = WMessage::new(buf, 0);

        // send to peer, should always succeed
        if let Err(e) = ts.tunnel_tx.send(wmsg) {
            error!(
                "[Tunnel]{} send_request_closed_to_server tx send failed:{}",
                ts.tun_idx, e
            );
        }
    }

    pub fn close_rawfd(&self) {
        info!("[Tunnel]close_rawfd, idx:{}", self.index);
        let r = shutdown(self.rawfd, Shutdown::Both);
        match r {
            Err(e) => {
                info!("[Tunnel]close_rawfd failed:{}", e);
            }
            _ => {}
        }
    }

    pub fn set_udpx_tx(&mut self, tx: UdpxTxType) {
        self.udpx_tx = Some(tx);
    }

    pub fn udp_proxy_north(&self, msg: BytesMut, src_addr: SocketAddr, dst_addr: SocketAddr) {
        // format udp forward packet
        let mut content_size = msg.len();
        if src_addr.is_ipv4() {
            content_size += 7; // 1 + 2 + 4
        } else {
            content_size += 19; // 1 + 2 + 16
        }
        if dst_addr.is_ipv4() {
            content_size += 7; // 1 + 2 + 4
        } else {
            content_size += 19; // 1 + 2 + 16
        }
        let hsize = 2 + 1;
        let total = hsize + content_size;

        let mut buf = vec![0; total];
        let bs = &mut buf[..];
        let offset = &mut 0;
        bs.write_with::<u16>(offset, total as u16, LE).unwrap(); // length
        bs.write_with::<u8>(offset, Cmd::UdpX as u8, LE).unwrap(); // cmd
        *offset = Tunnel::write_socketaddr(bs, *offset, &src_addr);
        *offset = Tunnel::write_socketaddr(bs, *offset, &dst_addr);
        let bss = &mut buf[*offset..];
        bss.copy_from_slice(&msg);

        // websocket message
        let wmsg = WMessage::new(buf, 0);

        let r = self.tx.send(wmsg);
        match r {
            Err(e) => {
                error!(
                    "[Tunnel]{} tunnel on_udp_proxy_forward error:{}",
                    self.index, e
                );
            }
            _ => {}
        }
    }

    fn write_socketaddr(bs: &mut [u8], offset: usize, addr: &SocketAddr) -> usize {
        let mut new_offset = offset;
        let new_offset = &mut new_offset;
        bs.write_with::<u16>(new_offset, addr.port() as u16, LE)
            .unwrap(); // port

        match addr.ip() {
            V4(v4) => {
                bs.write_with::<u8>(new_offset, 0 as u8, LE).unwrap(); // 0, ipv4
                let ipbytes = v4.octets();
                for b in ipbytes.iter() {
                    bs.write_with::<u8>(new_offset, *b, LE).unwrap(); // ip
                }
            }
            V6(v6) => {
                bs.write_with::<u8>(new_offset, 1 as u8, LE).unwrap(); // 0, ipv6
                let ipbytes = v6.segments();
                for b in ipbytes.iter() {
                    bs.write_with::<u16>(new_offset, *b, LE).unwrap(); // ip
                }
            }
        }

        *new_offset
    }

    fn on_udpx_south(&self, mut msg: RMessage) {
        if self.udpx_tx.is_none() {
            error!(
                "[Tunnel]{} tunnel on_udpx_msg error, no udpx_tx valid",
                self.index
            );
            return;
        }

        let offset = &mut 0;
        let bsv = msg.buf.take().unwrap();
        let bs = &bsv[3..]; // skip the length and cmd

        let src_addr = Tunnel::read_socketaddr(bs, offset);
        let dst_addr = Tunnel::read_socketaddr(bs, offset);

        let skip = 3 + *offset;
        let mut bb = bytes::Bytes::from(bsv);
        bb.advance(skip as usize);

        // forward to udpx
        let cmd = SubServiceCtlCmd::UdpRecv((bb, src_addr, dst_addr));
        let r = self.udpx_tx.as_ref().unwrap().send(cmd);
        match r {
            Err(e) => {
                error!("[Tunnel]{} tunnel on_udpx_msg send error:{}", self.index, e);
            }
            _ => {}
        }
    }

    fn read_socketaddr(bs: &[u8], offset: &mut usize) -> SocketAddr {
        let port = bs.read_with::<u16>(offset, LE).unwrap();
        let addr_type = bs.read_with::<u8>(offset, LE).unwrap();
        let ipbytes = if addr_type == 0 {
            // ipv4
            let a = bs.read_with::<u8>(offset, LE).unwrap(); // 4 bytes
            let b = bs.read_with::<u8>(offset, LE).unwrap(); // 4 bytes
            let c = bs.read_with::<u8>(offset, LE).unwrap(); // 4 bytes
            let d = bs.read_with::<u8>(offset, LE).unwrap(); // 4 bytes
            let ip = Ipv4Addr::new(a, b, c, d);
            IpAddr::V4(ip)
        } else {
            // ipv6
            let a = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let b = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let c = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let d = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let e = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let f = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let g = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes
            let h = bs.read_with::<u16>(offset, LE).unwrap(); // 16 bytes

            let ip = Ipv6Addr::new(a, b, c, d, e, f, g, h);
            IpAddr::V6(ip)
        };

        SocketAddr::from((ipbytes, port))
    }
}
