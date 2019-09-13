use super::tunbuilder;
use super::Tunnel;
use crate::config::TunCfg;
use crate::requests::{TunStub, Request};

use crossbeam::queue::ArrayQueue;

use log::info;
use log::{debug, error};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::prelude::*;
use tokio::timer::Interval;
pub const KEEP_ALIVE_INTERVAL: u64 = 5000;

type TunnelItem = Option<Arc<Tunnel>>;

pub struct TunMgr {
    url: String,
    capacity: usize,
    tunnels: Mutex<Vec<TunnelItem>>,
    reconnect_queue: ArrayQueue<u16>,
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
            tunnels: m,
            reconnect_queue: ArrayQueue::new(capacity),
        })
    }

    pub fn init(self: Arc<TunMgr>) {
        info!("[TunMgr]init");
        for n in 0..self.capacity {
            let index = n;
            let mgr = self.clone();
            tunbuilder::connect(&self.url, &mgr, index);
        }

        self.clone().start_keepalive_timer();
    }

    pub fn on_tunnel_created(&self, tun: &Arc<Tunnel>) {
        info!("[TunMgr]on_tunnel_created");
        let index = tun.index;
        let mut tunnels = self.tunnels.lock().unwrap();
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
        if let Err(e) = self.reconnect_queue.push(index as u16) {
            panic!("[TunMgr]on_tunnel_build_error push failed:{}", e);
        }

        info!("[TunMgr]tunnel build error, index:{}, rebuild later", index);
    }

    fn on_tunnel_closed_interal(&self, index: usize) -> TunnelItem {
        info!("[TunMgr]on_tunnel_closed_interal");
        let mut tunnels = self.tunnels.lock().unwrap();
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
        let tunnels = self.tunnels.lock().unwrap();
        let tun = &tunnels[index];

        match tun {
            Some(tun) => Some(tun.clone()),
            None => None,
        }
    }

    pub fn on_request_created(
        &self,
        req: Request,
    ) -> Option<TunStub> {
        info!("[TunMgr]on_request_created");
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
        let tunnels = self.tunnels.lock().unwrap();
        let mut tselected = None;
        let mut rtt = std::i64::MAX;
        let mut req_count = std::u16::MAX;

        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let rtt_tun = tun.get_rtt();
                    let req_count_tun = tun.get_req_count();
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

    fn start_keepalive_timer(self: Arc<TunMgr>) {
        info!("[TunMgr]start_keepalive_timer");
        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .for_each(move |instant| {
                debug!("[TunMgr]keepalive timer fire; instant={:?}", instant);
                self.send_pings();

                self.clone().process_reconnect();

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[TunMgr]start_keepalive_timer interval errored; err={:?}",
                    e
                )
            });

        tokio::spawn(task);
    }

    fn send_pings(&self) {
        let tunnels = self.tunnels.lock().unwrap();
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    if !tun.send_ping() {
                        // TODO: close underlay tcp-stream?
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

                tunbuilder::connect(&self.url, &self, index as usize);
            } else {
                break;
            }
        }
    }
}
