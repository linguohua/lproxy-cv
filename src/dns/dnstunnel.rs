use super::dnspacket::{BytePacketBuffer, DnsPacket};
use super::Forwarder;
use crate::config::KEEP_ALIVE_INTERVAL;
use crate::lws::{RMessage, WMessage};
use crate::tunnels::Cmd;
use byte::*;
use tokio::sync::mpsc::UnboundedSender;
use log::{error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::net::IpAddr::V4;
use std::net::IpAddr::V6;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::unix::io::RawFd;
use std::time::Instant;

type TxType = UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub struct DnsTunnel {
    pub tx: UnboundedSender<WMessage>,
    pub index: usize,

    rtt_queue: Vec<i64>,
    rtt_index: usize,
    rtt_sum: i64,

    ping_count: u8,
    time: Instant,

    udp_tx: Option<TxType>,
    rawfd: RawFd,
}

impl DnsTunnel {
    pub fn new(
        tx: UnboundedSender<WMessage>,
        rawfd: RawFd,
        udp_tx: TxType,
        idx: usize,
    ) -> DnsTunnel {
        info!("[DnsTunnel]new Tunnel, idx:{}", idx);
        let size = 5;
        let rtt_queue = vec![0; size];
        DnsTunnel {
            tx: tx,
            index: idx,
            rtt_queue: rtt_queue,
            rtt_index: 0,
            rtt_sum: 0,
            ping_count: 0,
            time: Instant::now(),
            udp_tx: Some(udp_tx),
            rawfd,
        }
    }

    pub fn on_tunnel_msg(&mut self, mut msg: RMessage, fw: &Forwarder) {
        let bs = msg.buf.as_mut().unwrap();
        // info!("[DnsTunnel]on_tunnel_msg, len:{}", bs.len());
        let bs = &mut bs[2..]; // skip the length

        let offset = &mut 0;
        let cmd = bs.read_with::<u8>(offset, LE).unwrap();
        let bs = &mut bs[1..]; // skip cmd
        let cmd = Cmd::from(cmd);

        match cmd {
            Cmd::Ping => {
                self.reply_ping(msg);
            }
            Cmd::Pong => {
                self.on_pong(bs);
            }
            Cmd::ReqData => {
                let len = bs.len();
                if len < 6 {
                    error!("[DnsTunnel]on_tunnel_msg data length({}) != 8", len);
                    return;
                }

                match &self.udp_tx {
                    Some(tx) => {
                        let offset = &mut 0;
                        let port = bs.read_with::<u16>(offset, LE).unwrap();
                        let ip32 = bs.read_with::<u32>(offset, LE).unwrap();
                        // info!("[DnsTunnel]on_tunnel_msg, port:{}, ip:{}", port, ip32);

                        {
                            let content = &mut bs[6..];

                            // parse dns reponse and store to ipset
                            // let buf = &mut bf.buf[0..message.len()];
                            let mut bf = BytePacketBuffer::new(content);
                            // buf.copy_from_slice();
                            let dnspacket = DnsPacket::from_buffer(&mut bf);
                            match dnspacket {
                                Ok(p) => {
                                    fw.save_ipset(&p);
                                }

                                Err(e) => {
                                    error!("[DnsTunnel]parse dns packet failed:{}", e);
                                }
                            }
                        }

                        {
                            let content2 = &bs[6..];
                            let sockaddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip32)), port);

                            tx.unbounded_send((bytes::Bytes::from(content2), sockaddr))
                                .unwrap(); // udp send should not failed!
                        }
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    pub fn on_closed(&self) {
        info!(
            "[DnsTunnel]tunnel live duration {} minutes",
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
        let r = self.tx.unbounded_send(msg);
        match r {
            Err(e) => {
                error!("[DnsTunnel]tunnel send_ping error:{}", e);
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

    fn reply_ping(&mut self, mut msg: RMessage) {
        // info!("[DnsTunnel] reply_ping");
        let mut vec = msg.buf.take().unwrap();
        let bs = &mut vec[2..];
        let offset = &mut 0;
        bs.write_with::<u8>(offset, Cmd::Pong as u8, LE).unwrap();

        let wmsg = WMessage::new(vec, 0);
        let tx = &self.tx;
        let result = tx.unbounded_send(wmsg);
        match result {
            Err(e) => {
                error!(
                    "[DnsTunnel]{} reply_ping tun send error:{}, tun_tx maybe closed",
                    self.index, e
                );
            }
            _ => {
                //info!("[DnsTunnel]on_dns_reply unbounded_send request msg",)
            }
        }
    }

    fn on_pong(&mut self, bs: &[u8]) {
        // info!("[DnsTunnel] on_pong");
        let len = bs.len();
        if len != 8 {
            error!("[DnsTunnel]{} pong data length({}) != 8", self.index, len);
            return;
        }

        // reset ping count
        self.ping_count = 0;

        let offset = &mut 0;
        let timestamp = bs.read_with::<u64>(offset, LE).unwrap();

        let in_ms = self.get_elapsed_milliseconds();
        assert!(
            in_ms >= timestamp,
            "[DnsTunnel] {} pong timestamp > now!",
            self.index
        );

        let rtt = in_ms - timestamp;
        let rtt = rtt as i64;
        self.append_rtt(rtt);
    }

    fn get_elapsed_milliseconds(&self) -> u64 {
        let in_ms = self.time.elapsed().as_millis();
        in_ms as u64
    }

    fn append_rtt(&mut self, rtt: i64) {
        let rtt_remove = self.rtt_queue[self.rtt_index];
        self.rtt_queue[self.rtt_index] = rtt;
        let len = self.rtt_queue.len();
        self.rtt_index = (self.rtt_index + 1) % len;

        self.rtt_sum = self.rtt_sum + rtt - rtt_remove;
    }

    pub fn get_rtt(&self) -> i64 {
        let rtt_sum = self.rtt_sum;
        rtt_sum / (self.rtt_queue.len() as i64)
    }

    pub fn on_dns_udp_msg(&self, message: bytes::BytesMut, addr: std::net::SocketAddr) {
        let port = addr.port();
        let mut ip: u32 = 0;
        match addr.ip() {
            V4(v4) => {
                ip = v4.into();
            }
            V6(_) => {
                // TODO: support v6
                error!("[DnsTunnel]ip6 address not supported yet");
            }
        }

        let size = message.len();
        let hsize = 9; // length + cmd + port + ipv4
        let total = hsize + size;
        let mut buf = vec![0; total];
        let header = &mut buf[0..hsize];
        let offset = &mut 0;

        info!("[DnsTunnel]on_dns_udp_msg, port:{}, ip:{}", port, ip);

        header.write_with::<u16>(offset, total as u16, LE).unwrap(); // length
        header
            .write_with::<u8>(offset, Cmd::ReqData as u8, LE)
            .unwrap(); // cmd
        header.write_with::<u16>(offset, port, LE).unwrap(); // port
        header.write_with::<u32>(offset, ip, LE).unwrap(); // ipv4

        let msg_body = &mut buf[hsize..];
        msg_body.copy_from_slice(message.as_ref());

        let wmsg = WMessage::new(buf, 0);
        let tx = &self.tx;
        let result = tx.unbounded_send(wmsg);
        match result {
            Err(e) => {
                error!(
                    "[DnsTunnel]request tun send error:{}, tun_tx maybe closed",
                    e
                );
            }
            _ => info!("[DnsTunnel]unbounded_send request msg",),
        }
    }

    pub fn close_rawfd(&self) {
        info!("[DnsTunnel]close_rawfd idx:{}", self.index);
        let r = shutdown(self.rawfd, Shutdown::Both);
        match r {
            Err(e) => {
                info!("[DnsTunnel]close_rawfd failed:{}", e);
            }
            _ => {}
        }
    }
}
