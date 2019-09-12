use super::Cmd;
use super::THeader;
use crate::requests::Reqq;
use crate::requests::{TunStub,Request};
use crate::tunnels::theader::THEADER_SIZE;
use byte::*;
use bytes::Bytes;
use crossbeam::queue::ArrayQueue;
use futures::sync::mpsc::UnboundedSender;
use log::{error, info};
use std::sync::atomic::{AtomicI64, AtomicU16, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use tungstenite::protocol::Message;

pub struct Tunnel {
    pub tx: UnboundedSender<Message>,
    pub index: usize,

    pub requests: Mutex<Reqq>,

    rtt_queue: ArrayQueue<i64>,

    rtt_sum: AtomicI64,
    req_count: AtomicU16,
    ping_count: AtomicU8,

    time: Instant,
}

impl Tunnel {
    pub fn new(tx: UnboundedSender<Message>, idx: usize) -> Tunnel {
        info!("[Tunnel]new Tunnel, idx:{}", idx);
        let size = 5;
        let rtt_queue = ArrayQueue::new(size);
        for _ in 0..size {
            if let Err(e) = rtt_queue.push(0) {
                panic!("init rtt_queue failed:{}", e);
            }
        }

        Tunnel {
            tx: tx,
            index: idx,
            requests: Mutex::new(Reqq::new(1)),

            rtt_queue: rtt_queue,
            rtt_sum: AtomicI64::new(0),
            req_count: AtomicU16::new(0),
            ping_count: AtomicU8::new(0),
            time: Instant::now(),
        }
    }

    pub fn on_tunnel_msg(&self, msg: Message) -> bool {
        // info!("[Tunnel]on_tunnel_msg");
        if msg.is_pong() {
            self.on_pong(msg);

            return true;
        }

        if !msg.is_binary() {
            info!("[Tunnel]tunnel should only handle binary msg!");
            return true;
        }

        let bs = msg.into_data();
        let th = THeader::read_from(&bs[..]);
        let cmd = Cmd::from(th.cmd);
        match cmd {
            Cmd::ReqData => {
                // data
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                let tx = self.get_request_tx(req_idx, req_tag);
                match tx {
                    None => {
                        info!("[Tunnel]no request found for: {}:{}", req_idx, req_tag);
                        return false;
                    }
                    Some(tx) => {
                        let b = Bytes::from(&bs[THEADER_SIZE..]);
                        let result = tx.unbounded_send(b);
                        match result {
                            Err(e) => {
                                info!("[Tunnel]tunnel msg send to request failed:{}", e);
                                return false;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Cmd::ReqServerFinished => {
                // server finished
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                self.free_request_tx(req_idx, req_tag);
            }
            Cmd::ReqServerClosed => {
                // server finished
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                // TODO: extract new method
                let reqs = &mut self.requests.lock().unwrap();
                let r = reqs.free(req_idx, req_tag);
                if r {
                    self.req_count.fetch_sub(1, Ordering::SeqCst);
                }
            }
            _ => {
                error!("[Tunnel]unsupport cmd:{:?}, discard msg", cmd);
            }
        }

        return true;
    }

    fn on_pong(&self, msg: Message) {
        let bs = msg.into_data();
        let len = bs.len();
        if len != 8 {
            error!("[Tunnel]pong data length({}) != 8", len);
            return;
        }

        // reset ping count
        self.ping_count.store(0, Ordering::SeqCst);

        let offset = &mut 0;
        let timestamp = bs.read_with::<u64>(offset, LE).unwrap();

        let in_ms = self.get_elapsed_milliseconds();
        assert!(in_ms >= timestamp, "[Tunnel]pong timestamp > now!");

        let rtt = in_ms - timestamp;
        let rtt = rtt as i64;
        self.append_rtt(rtt);
    }

    fn append_rtt(&self, rtt: i64) {
        if let Ok(rtt_remove) = self.rtt_queue.pop() {
            if let Err(e) = self.rtt_queue.push(rtt) {
                panic!("[Tunnel]rtt_quque push failed:{}", e);
            }

            self.rtt_sum.fetch_add(rtt - rtt_remove, Ordering::SeqCst);
        }
    }

    pub fn get_rtt(&self) -> i64 {
        let rtt_sum = self.rtt_sum.load(Ordering::SeqCst);
        rtt_sum / (self.rtt_queue.len() as i64)
    }

    pub fn get_req_count(&self) -> u16 {
        self.req_count.load(Ordering::SeqCst)
    }

    fn get_elapsed_milliseconds(&self) -> u64 {
        let in_ms = self.time.elapsed().as_millis();
        in_ms as u64
    }

    fn get_request_tx(&self, req_idx: u16, req_tag: u16) -> Option<UnboundedSender<Bytes>> {
        let requests = &self.requests.lock().unwrap();
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

    fn free_request_tx(&self, req_idx: u16, req_tag: u16) {
        let requests = &mut self.requests.lock().unwrap();
        let req_idx = req_idx as usize;
        if req_idx >= requests.elements.len() {
            return;
        }

        let req = &mut requests.elements[req_idx];
        if req.tag == req_tag && req.request_tx.is_some() {
            info!(
                "[Tunnel]free_request_tx, req_idx:{}, req_tag:{}",
                req_idx, req_tag
            );
            req.request_tx = None;
        }
    }

    pub fn on_request_created(
        &self,
        req: Request,
        dst: &libc::sockaddr_in,
    ) -> Option<TunStub> {
        info!("[Tunnel]on_request_created");
        let ts = self.on_request_created_internal(req);
        match ts {
            Some(ts) => {
                Tunnel::send_request_created_to_server(&ts, dst);

                Some(ts)
            }
            None => None,
        }
    }

    fn on_request_created_internal(
        &self,
        req:Request
    ) -> Option<TunStub> {
        let reqs = &mut self.requests.lock().unwrap();
        let (idx, tag) = reqs.alloc(req);
        let tun_idx;
        if idx != std::u16::MAX {
            tun_idx = self.index as u16;
        } else {
            return None;
        }

        let tx = self.tx.clone();
        self.req_count.fetch_add(1, Ordering::SeqCst);

        Some(TunStub {
            tunnel_tx: tx,
            tun_idx: tun_idx,
            req_idx: idx,
            req_tag: tag,
        })
    }

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        info!("[Tunnel]on_request_closed, tun index:{}", self.index);
        let reqs = &mut self.requests.lock().unwrap();
        let r = reqs.free(tunstub.req_idx, tunstub.req_tag);

        if r {
            info!(
                "[Tunnel]on_request_closed, tun index:{}, sub req_count by 1",
                self.index
            );
            self.req_count.fetch_sub(1, Ordering::SeqCst);
            Tunnel::send_request_closed_to_server(tunstub);
        }
    }

    pub fn on_closed(&self) {
        // free all requests
        let reqs = &mut self.requests.lock().unwrap();
        reqs.clear_all();

        info!(
            "[Tunnel]tunnel live duration {} minutes",
            self.time.elapsed().as_secs() / 60
        );
    }

    pub fn send_ping(&self) -> bool {
        let ping_count = self.ping_count.load(Ordering::SeqCst) as i64;
        if ping_count > 10 {
            return true;
        }

        let timestamp = self.get_elapsed_milliseconds();
        let mut bs1 = vec![0 as u8; 8];
        let bs = &mut bs1[..];
        let offset = &mut 0;
        bs.write_with::<u64>(offset, timestamp, LE).unwrap();

        let msg = Message::Ping(bs1);
        let r = self.tx.unbounded_send(msg);
        match r {
            Err(e) => {
                error!("[Tunnel]tunnel send_ping error:{}", e);
            }
            _ => {
                self.ping_count.fetch_add(1, Ordering::SeqCst);
                if ping_count > 0 {
                    // TODO: fix accurate RTT?
                    self.append_rtt(ping_count * (super::KEEP_ALIVE_INTERVAL as i64));
                }
            }
        }

        true
    }

    fn send_request_created_to_server(ts: &TunStub, dst: &libc::sockaddr_in) {
        info!("[Tunnel]send_request_created_to_server, dst:{:?}", dst);

        // send request to server
        let size = 1 + 4 + 2; // family + ipv4 + port;
        let hsize = THEADER_SIZE;
        let buf = &mut vec![0; hsize + size];

        let th = THeader::new(Cmd::ReqCreated, ts.req_idx, ts.req_tag);
        let msg_header = &mut buf[0..hsize];
        th.write_to(msg_header);
        let msg_body = &mut buf[hsize..];

        let offset = &mut 0;
        msg_body
            .write_with::<u8>(offset, dst.sin_family as u8, LE)
            .unwrap();
        msg_body
            .write_with::<u32>(offset, dst.sin_addr.s_addr as u32, LE)
            .unwrap();
        msg_body
            .write_with::<u16>(offset, dst.sin_port as u16, LE)
            .unwrap();

        // websocket message
        let wmsg = Message::from(&buf[..]);

        // send to peer, should always succeed
        if let Err(e) = ts.tunnel_tx.unbounded_send(wmsg) {
            error!(
                "[Tunnel]send_request_created_to_server tx send failed:{}",
                e
            );
        }
    }

    fn send_request_closed_to_server(ts: &TunStub) {
        info!("[Tunnel]send_request_closed_to_server, dst:{:?}", ts);

        // send request to server
        let hsize = THEADER_SIZE;
        let buf = &mut vec![0; hsize];

        let th = THeader::new(Cmd::ReqClientClosed, ts.req_idx, ts.req_tag);
        let msg_header = &mut buf[0..hsize];
        th.write_to(msg_header);

        // websocket message
        let wmsg = Message::from(&buf[..]);

        // send to peer, should always succeed
        if let Err(e) = ts.tunnel_tx.unbounded_send(wmsg) {
            error!("[Tunnel]send_request_closed_to_server tx send failed:{}", e);
        }
    }
}
