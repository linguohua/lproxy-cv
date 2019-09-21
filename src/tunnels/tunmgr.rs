use super::tunbuilder;
use super::Tunnel;
use super::{Request, TunStub};
use crate::config::{TunCfg, KEEP_ALIVE_INTERVAL};
use failure::Error;
use log::{debug, error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::Interval;

type TunnelItem = Option<Rc<RefCell<Tunnel>>>;
type LongLive = Rc<RefCell<TunMgr>>;

pub struct TunMgr {
    pub relay_domain: String,
    pub relay_port: u16,
    pub url: String,
    capacity: usize,
    pub tunnel_req_cap: usize,
    tunnels: Vec<TunnelItem>,
    reconnect_queue: Vec<u16>,
    discarded: bool,
    keepalive_trigger: Option<Trigger>,
}

impl TunMgr {
    pub fn new(cfg: &TunCfg) -> LongLive {
        info!("[TunMgr]new TunMgr");
        let capacity = cfg.tunnel_number;

        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(None);
        }

        Rc::new(RefCell::new(TunMgr {
            url: cfg.websocket_url.to_string(),
            capacity: capacity,
            tunnel_req_cap: cfg.tunnel_req_cap,
            tunnels: vec,
            reconnect_queue: Vec::with_capacity(capacity),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            discarded: false,
            keepalive_trigger: None,
        }))
    }

    pub fn init(&mut self, s: LongLive) -> Result<(), Error> {
        info!("[TunMgr]init");
        for n in 0..self.capacity {
            let index = n;
            let mgr = s.clone();
            tunbuilder::connect(self, mgr, index);
        }

        self.start_keepalive_timer(s);

        Ok(())
    }

    pub fn on_tunnel_created(&mut self, tun: Rc<RefCell<Tunnel>>) -> std::result::Result<(), ()> {
        info!("[TunMgr]on_tunnel_created");
        if self.discarded != false {
            error!("[TunMgr]on_tunnel_created, tunmgr is discarded, tun will be discarded");

            return Err(());
        }

        let index = tun.borrow().index;
        let tunnels = &mut self.tunnels;
        let t = &tunnels[index];

        if t.is_some() {
            panic!("[TunMgr]there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());

        info!("[TunMgr]tunnel created, index:{}", index);

        Ok(())
    }

    pub fn on_tunnel_closed(&mut self, index: usize) {
        info!("[TunMgr]on_tunnel_closed");
        let t = self.on_tunnel_closed_interal(index);

        match t {
            Some(t) => {
                let mut t = t.borrow_mut();
                t.on_closed();

                if self.discarded {
                    info!("[TunMgr]on_tunnel_closed, tunmgr is discarded, tun will be discard");
                    return;
                } else {
                    self.reconnect_queue.push(index as u16);
                }

                info!("[TunMgr]tunnel closed, index:{}", index);
            }
            None => {}
        }
    }

    pub fn on_tunnel_build_error(&mut self, index: usize) {
        info!("[TunMgr]on_tunnel_build_error");
        if self.discarded != false {
            error!("[TunMgr]on_tunnel_build_error, tunmgr is discarded, tun will be not reconnect");

            return;
        }

        self.reconnect_queue.push(index as u16);

        info!("[TunMgr]tunnel build error, index:{}, rebuild later", index);
    }

    fn on_tunnel_closed_interal(&mut self, index: usize) -> TunnelItem {
        info!("[TunMgr]on_tunnel_closed_interal");
        let tunnels = &mut self.tunnels;
        let t = &tunnels[index];

        match t {
            Some(t) => {
                let t = t.clone();
                tunnels[index] = None;

                Some(t)
            }
            None => None,
        }
    }

    fn get_tunnel(&self, index: usize) -> TunnelItem {
        info!("[TunMgr]get_tunnel");
        let tunnels = &self.tunnels;
        let tun = &tunnels[index];

        match tun {
            Some(tun) => Some(tun.clone()),
            None => None,
        }
    }

    pub fn on_request_created(&mut self, req: Request) -> Option<TunStub> {
        info!("[TunMgr]on_request_created");
        if self.discarded != false {
            error!("[TunMgr]on_request_created, tunmgr is discarded, request will be discarded");

            return None;
        }

        if let Some(tun) = self.alloc_tunnel_for_req() {
            let mut tun = tun.borrow_mut();
            tun.on_request_created(req)
        } else {
            None
        }
    }

    pub fn on_request_closed(&mut self, tunstub: &TunStub) {
        info!("[TunMgr]on_request_closed:{:?}", tunstub);
        let tidx = tunstub.tun_idx;
        match self.get_tunnel(tidx as usize) {
            Some(tun) => {
                let mut tun = tun.borrow_mut();
                tun.on_request_closed(tunstub);
            }
            None => {
                error!("[TunMgr]on_request_closed:{:?}, not found", tunstub);
            }
        }
    }

    fn alloc_tunnel_for_req(&mut self) -> TunnelItem {
        info!("[TunMgr]alloc_tunnel_for_req");
        let tunnels = &mut self.tunnels;
        let mut tselected = None;
        let mut rtt = std::i64::MAX;
        let mut req_count = std::u16::MAX;

        for t in tunnels.iter() {
            match t {
                Some(tun2) => {
                    let tun = tun2.borrow();
                    let req_count_tun = tun.get_req_count();
                    // skip fulled tunnel
                    if (req_count_tun + 1) >= tun.capacity {
                        continue;
                    }

                    let rtt_tun = tun.get_rtt();

                    let mut selected = false;
                    info!(
                        "[TunMgr]alloc_tunnel_for_req, idx:{}, rtt:{}, req_count:{}",
                        tun.index, rtt_tun, req_count_tun
                    );

                    if rtt_tun < rtt {
                        selected = true;
                    } else if rtt_tun == rtt && req_count_tun < req_count {
                        selected = true;
                    }

                    if selected {
                        rtt = rtt_tun;
                        req_count = req_count_tun;
                        tselected = Some(tun2.clone());
                    }
                }
                None => {}
            }
        }

        tselected
    }

    fn send_pings(&self) {
        let tunnels = &self.tunnels;
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let mut tun = tun.borrow_mut();
                    if !tun.send_ping() {
                        tun.close_rawfd();
                    }
                }
                None => {}
            }
        }
    }

    fn process_reconnect(&mut self, s: LongLive) {
        loop {
            if let Some(index) = self.reconnect_queue.pop() {
                info!("[TunMgr]process_reconnect, index:{}", index);

                tunbuilder::connect(self, s.clone(), index as usize);
            } else {
                break;
            }
        }
    }

    fn save_keepalive_trigger(&mut self, trigger: Trigger) {
        self.keepalive_trigger = Some(trigger);
    }

    fn keepalive(&mut self, s: LongLive) {
        if self.discarded != false {
            error!("[TunMgr]keepalive, tunmgr is discarded, not do keepalive");

            return;
        }

        self.send_pings();
        self.process_reconnect(s.clone());
    }

    pub fn stop(&mut self) {
        if self.discarded != false {
            error!("[TunMgr]stop, tunmgr is already discarded");

            return;
        }

        self.discarded = true;
        self.keepalive_trigger = None;

        // close all tunnel, and forbit reconnect
        let tunnels = &self.tunnels;
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let tun = tun.borrow();
                    tun.close_rawfd();
                }
                None => {}
            }
        }
    }

    fn start_keepalive_timer(&mut self, s2: LongLive) {
        info!("[TunMgr]start_keepalive_timer");
        let (trigger, tripwire) = Tripwire::new();
        self.save_keepalive_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .skip(1)
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[TunMgr]keepalive timer fire; instant={:?}", instant);

                let mut rf = s2.borrow_mut();
                rf.keepalive(s2.clone());

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[TunMgr]start_keepalive_timer interval errored; err={:?}",
                    e
                )
            })
            .then(|_| {
                info!("[TunMgr] keepalive timer future completed");
                Ok(())
            });;

        current_thread::spawn(task);
    }
}
