use super::Tunnel;
use crate::config::TunCfg;
use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use std::sync::Arc;
use super::tunbuilder;
use crate::requests::TunStub;
use std::sync::Mutex;
use tungstenite::protocol::Message;

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
    }

    pub fn on_tunnel_created(&self, index: usize, tun: Tunnel) {
        let mut tunnels = self.tunnels.lock().unwrap();
        let t = &tunnels[index];

        if t.is_some() {
            panic!("there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(Arc::new(tun));
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

    pub fn on_tunnel_msg(&self, msg: Message, index: usize) -> bool {
        let tun = self.get_tunnel(index);
        match tun {
            Some(tun) => {
                return tun.on_tunnel_msg(msg);
            }
            None => {
                println!("no tunnel found for:{}, discard msg", index);
            }
        }

        return true;
    }

    fn get_tunnel(&self, index: usize) -> TunnelItem {
        let tunnels = self.tunnels.lock().unwrap();
        let tun = &tunnels[index];

        match tun {
            Some(tun) => Some(tun.clone()),
            None => None,
        }
    }

    pub fn on_request_created(&self, req_tx: &UnboundedSender<Bytes>) -> Arc<TunStub> {
        let tidx = 0;
        let tun = self.get_tunnel(tidx).unwrap();
        tun.on_request_created(req_tx)
    }

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        let tidx = tunstub.tun_idx;
        let tun = self.get_tunnel(tidx as usize).unwrap();
        tun.on_request_closed(tunstub);
    }
}
