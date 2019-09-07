use super::tunbuilder;
use super::Tunnel;
use crate::config::TunCfg;
use crate::requests::TunStub;
use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use log::{debug, error};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::timer::Interval;

type TunnelItem = Option<Arc<Tunnel>>;

pub struct TunMgr {
    tunnels: Mutex<Vec<TunnelItem>>,
}

impl TunMgr {
    pub fn new(capacity: usize) -> Arc<TunMgr> {
        let vec = Vec::with_capacity(capacity);
        let m = Mutex::new(vec);
        Arc::new(TunMgr { tunnels: m })
    }

    pub fn init(self: Arc<TunMgr>, cfg: &TunCfg) {
        let mut vec = self.tunnels.lock().unwrap();

        for _ in 1..cfg.number {
            vec.push(None);
        }

        for n in 1..cfg.number {
            let index = n;
            let mgr = self.clone();
            tunbuilder::connect(&cfg.url, &mgr, index);
        }

        self.clone().start_keepalive_timer();
    }

    pub fn on_tunnel_created(&self, tun: &Arc<Tunnel>) {
        let index = tun.index;
        let mut tunnels = self.tunnels.lock().unwrap();
        let t = &tunnels[index];

        if t.is_some() {
            panic!("there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());
    }

    pub fn on_tunnel_closed(&self, index: usize) {
        let t = self.on_tunnel_closed_interal(index);
        match t {
            Some(t) => {
                t.on_closed();
            }
            None => {}
        }
    }

    fn on_tunnel_closed_interal(&self, index: usize) -> TunnelItem {
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
        let tunnels = self.tunnels.lock().unwrap();
        let tun = &tunnels[index];

        match tun {
            Some(tun) => Some(tun.clone()),
            None => None,
        }
    }

    pub fn on_request_created(
        &self,
        req_tx: &UnboundedSender<Bytes>,
        dst: &libc::sockaddr_in,
    ) -> Option<TunStub> {
        let tun = self.alloc_tunnel_for_req().unwrap();
        tun.on_request_created(req_tx, dst)
    }

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        let tidx = tunstub.tun_idx;
        let tun = self.get_tunnel(tidx as usize).unwrap();
        tun.on_request_closed(tunstub);
    }

    fn alloc_tunnel_for_req(&self) -> TunnelItem {
        let tunnels = self.tunnels.lock().unwrap();
        let mut tselected = None;
        let mut rtt = std::u64::MAX;
        let mut req_count = std::u16::MAX;

        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let rtt_tun = tun.get_rtt();
                    let req_count_tun = tun.get_req_count();
                    if rtt_tun < rtt {
                        rtt = rtt_tun;
                        tselected = Some(tun.clone());
                    } else if req_count_tun < req_count {
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
        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(3000))
            .for_each(move |instant| {
                debug!("keepalive timer fire; instant={:?}", instant);
                self.send_pings();

                Ok(())
            })
            .map_err(|e| error!("interval errored; err={:?}", e));

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
}
