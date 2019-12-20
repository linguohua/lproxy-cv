use super::xtunel_connect;
use super::XReqq;
use crate::config::{TunCfg, KEEP_ALIVE_INTERVAL};
use crate::lws::{RMessage, TMessage, WMessage};
use crate::tunnels::{Cmd, THeader, THEADER_SIZE};
use byte::*;
use failure::Error;
use futures::sync::mpsc::UnboundedSender;
use log::{debug, error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::Interval;

pub type LongLive = Rc<RefCell<XTunnel>>;

pub struct XTunnel {
    pub url_string: String,
    pub token: String,
    discarded: bool,
    need_reconnect: bool,
    tun_tx: Option<UnboundedSender<WMessage>>,
    keepalive_trigger: Option<Trigger>,
    ping_count: usize,
    rawfd: Option<RawFd>,
    time: Instant,
    req_count: u16,
    requests: XReqq,
    pub req_cap: u16,
}

impl XTunnel {
    pub fn new(cfg: &TunCfg) -> LongLive {
        info!("[XPort]new XTunnel");
        let req_cap = 128;
        let url = &cfg.xport_url;
        let tok = &cfg.token;

        Rc::new(RefCell::new(XTunnel {
            url_string: url.to_string(),
            token: tok.to_string(),
            discarded: false,
            need_reconnect: false,
            tun_tx: None,
            keepalive_trigger: None,
            ping_count: 0,
            rawfd: None,
            time: Instant::now(),
            req_count: 0,
            req_cap,
            requests: XReqq::new(req_cap as usize),
        }))
    }

    pub fn start(&mut self, s: LongLive) -> std::result::Result<(), Error> {
        info!("[XPort]start XTunnel");

        xtunel_connect(self, s.clone());
        self.start_keepalive_timer(s);

        Ok(())
    }

    pub fn stop(&mut self) {
        info!("[XPort]stop XTunnel");
        if self.discarded {
            error!("[XPort]stop, XTunnel is already discarded");

            return;
        }

        self.discarded = true;
        self.keepalive_trigger = None;
    }

    pub fn on_tunnel_created(
        &mut self,
        rawfd: RawFd,
        tx: UnboundedSender<WMessage>,
    ) -> std::result::Result<(), Error> {
        info!("[XPort]on_tunnel_created");
        if self.discarded != false {
            error!("[XPort]on_tunnel_created, tunmgr is discarded, tun will be discarded");

            return Err(failure::err_msg("XTunnel has discarded"));
        }

        self.rawfd = Some(rawfd);
        self.tun_tx = Some(tx);

        Ok(())
    }

    pub fn on_tunnel_closed(&mut self) {
        info!("[XPort]on_tunnel_closed");
        self.rawfd = None;
        let t = self.tun_tx.take();

        if t.is_some() {
            if self.discarded {
                info!("[XPort]on_tunnel_closed, XTunnel is discarded, tun will be discard");
                return;
            } else {
                self.need_reconnect = true;
            }

            info!("[XPort]xtunnel closed, reconnect later");
        }

        let reqs = &mut self.requests;
        reqs.clear_all();
    }

    pub fn on_tunnel_build_error(&mut self) {
        info!("[XPort]on_tunnel_build_error");
        if self.discarded != false {
            error!("[XPort]on_tunnel_build_error, XTunnel is discarded, tun will be not reconnect");

            return;
        }

        self.need_reconnect = true;

        info!("[XPort]tunnel build error, rebuild later");
    }

    fn save_keepalive_trigger(&mut self, trigger: Trigger) {
        self.keepalive_trigger = Some(trigger);
    }

    fn keepalive(&mut self, s: LongLive) {
        if self.discarded != false {
            error!("[XPort]keepalive, XTunnel is discarded, not do keepalive");

            return;
        }

        self.send_ping();
        self.process_reconnect(s.clone());
    }

    fn process_reconnect(&mut self, s: LongLive) {
        if self.need_reconnect {
            xtunel_connect(self, s);
            self.ping_count = 0;
            self.need_reconnect = false;
        }
    }

    fn start_keepalive_timer(&mut self, s2: LongLive) {
        info!("[XPort]start_keepalive_timer");
        let (trigger, tripwire) = Tripwire::new();
        self.save_keepalive_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .skip(1)
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[XPort]keepalive timer fire; instant={:?}", instant);

                let mut rf = s2.borrow_mut();
                rf.keepalive(s2.clone());

                Ok(())
            })
            .map_err(|e| error!("[XPort]start_keepalive_timer interval errored; err={:?}", e))
            .then(|_| {
                info!("[XPort] keepalive timer future completed");
                Ok(())
            });

        current_thread::spawn(task);
    }

    fn send_ping(&mut self) {
        let ping_count = self.ping_count;
        if ping_count > 5 {
            // exceed max ping count
            info!("[XPort] ping exceed max, close rawfd");
            self.close_rawfd();

            return;
        }

        if self.tun_tx.is_none() {
            return;
        }

        let timestamp = self.get_elapsed_milliseconds();
        let mut bs1 = vec![0 as u8; 11]; // 2 bytes length, 1 byte cmd, 8 byte content
        let bs = &mut bs1[..];
        let offset = &mut 0;
        bs.write_with::<u16>(offset, 11, LE).unwrap();
        bs.write_with::<u8>(offset, Cmd::Ping as u8, LE).unwrap();
        bs.write_with::<u64>(offset, timestamp, LE).unwrap();

        let msg = WMessage::new(bs1, 0);
        let r = self.tun_tx.as_ref().unwrap().unbounded_send(msg);
        match r {
            Err(e) => {
                error!("[XPort] tunnel send_ping error:{}", e);
            }
            _ => {
                self.ping_count += 1;
            }
        }
    }

    fn close_rawfd(&mut self) {
        info!("[XPort]close_rawfd");
        if self.rawfd.is_none() {
            return;
        }

        let r = shutdown(self.rawfd.take().unwrap(), Shutdown::Both);
        match r {
            Err(e) => {
                info!("[XPort]close_rawfd failed:{}", e);
            }
            _ => {}
        }
    }

    fn get_elapsed_milliseconds(&self) -> u64 {
        let in_ms = self.time.elapsed().as_millis();
        in_ms as u64
    }

    pub fn on_tunnel_msg(&mut self, msg: RMessage, ll: LongLive) {
        // info!("[XPort]on_tunnel_msg");
        let bs = msg.buf.as_ref().unwrap();
        let bs = &bs[2..]; // skip the length

        let offset = &mut 0;
        let cmd = bs.read_with::<u8>(offset, LE).unwrap();
        let bs = &bs[1..]; // skip cmd
        let cmd = Cmd::from(cmd);

        match cmd {
            Cmd::Ping => {
                // send to per
                self.reply_ping(msg);
            }
            Cmd::Pong => {
                self.on_pong(bs);
            }
            _ => {
                self.on_tunnel_proxy_msg(cmd, msg, ll);
            }
        }
    }

    fn on_tunnel_proxy_msg(&mut self, cmd: Cmd, mut msg: RMessage, tl: LongLive) {
        let vec = msg.buf.take().unwrap();
        let bs = &vec[3..];
        let th = THeader::read_from(&bs[..]);
        let bs = &bs[THEADER_SIZE..];

        match cmd {
            Cmd::ReqData => {
                // data
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                let tx = self.get_request_tx(req_idx, req_tag);
                match tx {
                    None => {
                        info!("[XPort]no request found for: {}:{}", req_idx, req_tag);
                        return;
                    }
                    Some(tx) => {
                        // info!(
                        //     "[XPort]{} proxy request msg, {}:{}",
                        //     self.tunnel_id, req_idx, req_tag
                        // );
                        let wmsg = WMessage::new(vec, (3 + THEADER_SIZE) as u16);
                        let result = tx.unbounded_send(wmsg);
                        match result {
                            Err(e) => {
                                info!("[XPort]tunnel msg send to request failed:{}", e);
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Cmd::ReqClientFinished => {
                // client finished
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                info!("[XPort]ReqClientFinished, idx:{}, tag:{}", req_idx, req_tag);

                self.free_request_tx(req_idx, req_tag);
            }
            Cmd::ReqClientClosed => {
                // client closed
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                info!("[XPort]ReqClientClosed, idx:{}, tag:{}", req_idx, req_tag);

                let reqs = &mut self.requests;
                let r = reqs.free(req_idx, req_tag);
                if r && self.req_count > 0 {
                    self.req_count -= 1;
                }
            }
            Cmd::ReqCreated => {
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;

                let offset = &mut 0;
                // port, u16
                let port = bs.read_with::<u16>(offset, LE).unwrap();
                self.requests.alloc(req_idx, req_tag);

                // start connect to target
                if super::proxy_request(self, tl, req_idx, req_tag, port) {
                    self.req_count += 1;
                }
            }
            _ => {
                error!("[XPort] unsupport cmd:{:?}, discard msg", cmd);
            }
        }
    }

    fn reply_ping(&mut self, mut msg: RMessage) {
        if self.tun_tx.is_none() {
            return;
        }

        //info!("[XPort] reply_ping");
        let mut vec = msg.buf.take().unwrap();
        let bs = &mut vec[2..];
        let offset = &mut 0;
        bs.write_with::<u8>(offset, Cmd::Pong as u8, LE).unwrap();

        let wmsg = WMessage::new(vec, 0);
        let tx = self.tun_tx.as_ref().unwrap();
        let result = tx.unbounded_send(wmsg);
        match result {
            Err(e) => {
                error!(
                    "[XPort]reply_ping tun send error:{}, tun_tx maybe closed",
                    e
                );
            }
            _ => {
                //info!("[XPort]on_dns_reply unbounded_send request msg",)
            }
        }
    }

    fn on_pong(&mut self, bs: &[u8]) {
        //info!("[XPort] on_pong");
        let len = bs.len();
        if len != 8 {
            error!("[XPort]pong data length({}) != 8", len);
            return;
        }

        // reset ping count
        self.ping_count = 0;

        let offset = &mut 0;
        let timestamp = bs.read_with::<u64>(offset, LE).unwrap();

        let in_ms = self.get_elapsed_milliseconds();
        assert!(in_ms >= timestamp, "[XPort]pong timestamp > now!");
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
                "[XPort]free_request_tx, req_idx:{}, req_tag:{}",
                req_idx, req_tag
            );
            req.request_tx = None;
        }
    }

    pub fn on_request_connect_error(&mut self, req_idx: u16, req_tag: u16) {
        info!("[XPort]on_request_connect_error, req_idx:{}", req_idx);

        self.on_request_closed(req_idx, req_tag);
    }

    pub fn on_request_closed(&mut self, req_idx: u16, req_tag: u16) {
        info!("[XPort]on_request_closed, req_idx:{}", req_idx);

        if !self.check_req_valid(req_idx, req_tag) {
            return;
        }

        if self.tun_tx.is_none() {
            return;
        }

        let reqs = &mut self.requests;
        let r = reqs.free(req_idx, req_tag);
        if r {
            info!(
                "[XPort]on_request_closed, tun index:{}, sub req_count by 1",
                req_idx
            );
            self.req_count -= 1;

            // send request to agent
            let hsize = 3 + THEADER_SIZE;
            let mut buf = vec![0; hsize];
            let offset = &mut 0;
            let header = &mut buf[..];
            header.write_with::<u16>(offset, hsize as u16, LE).unwrap();
            header
                .write_with::<u8>(offset, Cmd::ReqServerClosed as u8, LE)
                .unwrap();

            let th = THeader::new(req_idx, req_tag);
            let msg_header = &mut buf[3..];
            th.write_to(msg_header);

            // websocket message
            let wmsg = WMessage::new(buf, 0);

            // send to peer, should always succeed
            if let Err(e) = self.tun_tx.as_ref().unwrap().unbounded_send(wmsg) {
                error!("[XPort]send_request_closed_to_server tx send failed:{}", e);
            }
        }
    }

    pub fn on_request_recv_finished(&mut self, req_idx: u16, req_tag: u16) {
        info!("[XPort] on_request_recv_finished:{}", req_idx);

        if !self.check_req_valid(req_idx, req_tag) {
            return;
        }

        if self.tun_tx.is_none() {
            return;
        }

        // send request to agent
        let hsize = 3 + THEADER_SIZE;
        let mut buf = vec![0; hsize];
        let offset = &mut 0;
        let header = &mut buf[..];
        header.write_with::<u16>(offset, hsize as u16, LE).unwrap();
        header
            .write_with::<u8>(offset, Cmd::ReqServerFinished as u8, LE)
            .unwrap();

        let th = THeader::new(req_idx, req_tag);
        let msg_header = &mut buf[3..];
        th.write_to(msg_header);

        // websocket message
        let wmsg = WMessage::new(buf, 0);
        let result = self.tun_tx.as_ref().unwrap().unbounded_send(wmsg);

        match result {
            Err(e) => {
                error!(
                    "[XPort] on_request_recv_finished, tun send error:{}, tun_tx maybe closed",
                    e
                );
            }
            _ => {}
        }
    }

    pub fn on_request_msg(&mut self, mut message: TMessage, req_idx: u16, req_tag: u16) -> bool {
        if !self.check_req_valid(req_idx, req_tag) {
            error!("[XPort] on_request_msg failed, check_req_valid false");
            return false;
        }

        if self.tun_tx.is_none() {
            error!("[XPort] on_request_msg failed, tun_tx is none");
            return false;
        }

        let mut vec = message.buf.take().unwrap();
        let ll = vec.len();
        let bs = &mut vec[..];
        let offset = &mut 0;
        bs.write_with::<u16>(offset, ll as u16, LE).unwrap();
        bs.write_with::<u8>(offset, Cmd::ReqData as u8, LE).unwrap();

        let th = THeader::new(req_idx, req_tag);
        let msg_header = &mut vec[3..];
        th.write_to(msg_header);

        // info!(
        //     "[XPort] send request response to peer, len:{}",
        //     vec.len()
        // );

        let wmsg = WMessage::new(vec, 0);
        let result = self.tun_tx.as_ref().unwrap().unbounded_send(wmsg);
        match result {
            Err(e) => {
                error!("[XPort]request tun send error:{}, tun_tx maybe closed", e);
                return false;
            }
            _ => {
                // info!("[XPort]unbounded_send request msg, req_idx:{}", req_idx);
            }
        }

        true
    }

    pub fn save_request_tx(
        &mut self,
        tx: UnboundedSender<WMessage>,
        trigger: Trigger,
        req_idx: u16,
        req_tag: u16,
    ) -> std::result::Result<(), Error> {
        if self.discarded {
            return Err(failure::err_msg("xtunnel has been discarded"));
        }

        let requests = &mut self.requests;
        let req_idx = req_idx as usize;
        if req_idx >= requests.elements.len() {
            return Err(failure::err_msg("req_idx is invalid"));
        }

        let req = &mut requests.elements[req_idx];
        if req.tag != req_tag {
            return Err(failure::err_msg("req tag is invalid"));
        }

        req.trigger = Some(trigger);
        req.request_tx = Some(tx);

        Ok(())
    }

    fn check_req_valid(&self, req_idx: u16, req_tag: u16) -> bool {
        let requests = &self.requests;
        let req_idx2 = req_idx as usize;
        if req_idx2 >= requests.elements.len() {
            return false;
        }

        let req = &requests.elements[req_idx2];
        if !req.is_inused {
            return false;
        }

        if req.tag != req_tag {
            return false;
        }

        true
    }
}
