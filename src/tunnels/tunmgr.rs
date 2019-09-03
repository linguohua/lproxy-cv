use super::Tunnel;
use crate::config::TunCfg;

use std::sync::Arc;
use std::sync::Mutex;
use tungstenite::protocol::Message;

type TunnelItem = Option<Tunnel>;

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
            Tunnel::connect(&cfg.url, &mgr, index);
        }
    }

    pub fn on_tunnel_created(self: Arc<TunMgr>, index: usize, tun: Tunnel) {
        let mut tunnels = self.tunnels.lock().unwrap();
        let t = &tunnels[index];

        if t.is_some() {
            panic!("there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun);
    }

    pub fn on_tunnel_closed(self: Arc<TunMgr>, index: usize) {
        let mut tunnels = self.tunnels.lock().unwrap();
        let t = &tunnels[index];

        if t.is_none() {
            panic!("there is no tunnel at {}!", index);
        }

        tunnels[index] = None;
    }

    pub fn on_tunnel_msg(self: Arc<TunMgr>, _msg: Message, _index: usize) {}
}
