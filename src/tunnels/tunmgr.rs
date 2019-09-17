use super::tunbuilder;
use super::Tunnel;
use crate::config::TunCfg;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::requests::{Request, TunStub};

use crossbeam::queue::ArrayQueue;
use failure::Error;
use std::result::Result;

use log::error;
use log::info;
use parking_lot::Mutex;
use std::sync::Arc;

type TunnelItem = Option<Arc<Tunnel>>;

pub struct TunMgr {
    pub relay_domain: String,
    pub relay_port: u16,
    pub url: String,
    capacity: usize,
    pub tunnel_req_cap: usize,
    tunnels: Mutex<Vec<TunnelItem>>,
    reconnect_queue: ArrayQueue<u16>,
    discarded: AtomicBool,
}

impl TunMgr {
    pub fn new(cfg: &TunCfg) -> Arc<TunMgr> {
        info!("[TunMgr]new TunMgr, cfg:{:?}", cfg);
        let capacity = cfg.tunnel_number;

        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(None);
        }

        let m = Mutex::new(vec);

        Arc::new(TunMgr {
            url: cfg.websocket_url.to_string(),
            capacity: capacity,
            tunnel_req_cap: cfg.tunnel_req_cap,
            tunnels: m,
            reconnect_queue: ArrayQueue::new(capacity),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            discarded: AtomicBool::new(false),
        })
    }

    pub fn init(self: Arc<TunMgr>) -> Result<(), Error> {
        info!("[TunMgr]init");
        for n in 0..self.capacity {
            let index = n;
            let mgr = self.clone();
            tunbuilder::connect(&mgr, index);
        }

        Ok(())
    }

    pub fn on_tunnel_created(&self, tun: &Arc<Tunnel>) {
        info!("[TunMgr]on_tunnel_created");
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[TunMgr]on_tunnel_created, tunmgr is discarded, tun will be discarded");

            return;
        }

        let index = tun.index;
        let mut tunnels = self.tunnels.lock();
        let t = &tunnels[index];

        if t.is_some() {
            panic!("[TunMgr]there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());

        info!("[TunMgr]tunnel created, index:{}", index);
    }

    pub fn on_tunnel_closed(&self, index: usize) {
        info!("[TunMgr]on_tunnel_closed");
        let t = self.on_tunnel_closed_interal(index);
        match t {
            Some(t) => {
                t.on_closed();
                if let Err(e) = self.reconnect_queue.push(index as u16) {
                    panic!("[TunMgr]reconnect_queue push failed:{}", e);
                }

                info!("[TunMgr]tunnel closed, index:{}", index);
            }
            None => {}
        }
    }

    pub fn on_tunnel_build_error(&self, index: usize) {
        info!("[TunMgr]on_tunnel_build_error");
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[TunMgr]on_tunnel_build_error, tunmgr is discarded, tun will be not reconnect");

            return;
        }

        if let Err(e) = self.reconnect_queue.push(index as u16) {
            panic!("[TunMgr]on_tunnel_build_error push failed:{}", e);
        }

        info!("[TunMgr]tunnel build error, index:{}, rebuild later", index);
    }

    fn on_tunnel_closed_interal(&self, index: usize) -> TunnelItem {
        info!("[TunMgr]on_tunnel_closed_interal");
        let mut tunnels = self.tunnels.lock();
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
        let tunnels = self.tunnels.lock();
        let tun = &tunnels[index];

        match tun {
            Some(tun) => Some(tun.clone()),
            None => None,
        }
    }

    pub fn on_request_created(&self, req: Request) -> Option<TunStub> {
        info!("[TunMgr]on_request_created");
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[TunMgr]on_request_created, tunmgr is discarded, request will be discarded");

            return None;
        }

        if let Some(tun) = self.alloc_tunnel_for_req() {
            tun.on_request_created(req)
        } else {
            None
        }
    }

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        info!("[TunMgr]on_request_closed:{:?}", tunstub);
        let tidx = tunstub.tun_idx;
        match self.get_tunnel(tidx as usize) {
            Some(tun) => tun.on_request_closed(tunstub),
            None => {
                error!("[TunMgr]on_request_closed:{:?}, not found", tunstub);
            }
        }
    }

    fn alloc_tunnel_for_req(&self) -> TunnelItem {
        info!("[TunMgr]alloc_tunnel_for_req");
        let tunnels = self.tunnels.lock();
        let mut tselected = None;
        let mut rtt = std::i64::MAX;
        let mut req_count = std::u16::MAX;

        for t in tunnels.iter() {
            match t {
                Some(tun) => {
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
                        tselected = Some(tun.clone());
                    }
                }
                None => {}
            }
        }

        tselected
    }

    fn send_pings(&self) {
        let tunnels = self.tunnels.lock();
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    if !tun.send_ping() {
                        tun.close_rawfd();
                    }
                }
                None => {}
            }
        }
    }

    fn process_reconnect(self: Arc<TunMgr>) {
        loop {
            if let Ok(index) = self.reconnect_queue.pop() {
                info!("[TunMgr]process_reconnect, index:{}", index);

                tunbuilder::connect(&self, index as usize);
            } else {
                break;
            }
        }
    }

    pub fn keepalive(self: Arc<TunMgr>) {
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[TunMgr]keepalive, tunmgr is discarded, not do keepalive");

            return;
        }
        self.send_pings();
        self.process_reconnect();
    }

    pub fn stop(&self) {
        if self
            .discarded
            .compare_and_swap(false, true, Ordering::SeqCst)
            != false
        {
            error!("[TunMgr]stop, tunmgr is already discarded");

            return;
        }

        // close all tunnel, and forbit reconnect
        let tunnels = self.tunnels.lock();
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    tun.close_rawfd();
                }
                None => {}
            }
        }
    }
}
