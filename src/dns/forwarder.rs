use super::dnstunbuilder;
use super::DnsTunnel;
use crate::config::TunCfg;
use std::sync::Arc;

use crossbeam::queue::ArrayQueue;
use parking_lot::Mutex;

use super::UdpServer;
use crate::config::KEEP_ALIVE_INTERVAL;
use log::{debug, error, info};
use std::time::{Duration, Instant};
use tokio::prelude::*;
use tokio::timer::Interval;
type TunnelItem = Option<Arc<DnsTunnel>>;

pub struct Forwarder {
    pub udp_addr: String,
    pub relay_domain: String,
    pub relay_port: u16,
    pub dns_tun_url: String,

    tunnels: Mutex<Vec<TunnelItem>>,
    reconnect_queue: ArrayQueue<u16>,
    capacity: usize,

    server: Arc<UdpServer>,
}

impl Forwarder {
    pub fn new(cfg: &TunCfg) -> Arc<Forwarder> {
        let capacity = cfg.dns_tunnel_number;

        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(None);
        }

        let m = Mutex::new(vec);

        Arc::new(Forwarder {
            udp_addr: cfg.dns_udp_addr.to_string(),
            dns_tun_url: cfg.dns_tun_url.to_string(),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            tunnels: m,
            reconnect_queue: ArrayQueue::new(capacity),
            capacity: capacity,
            server: UdpServer::new(&cfg.dns_udp_addr),
        })
    }

    pub fn init(self: Arc<Forwarder>) {
        info!("[Forwarder]init");
        self.server.clone().start(&self);
    }

    pub fn on_dns_udp_created(self: Arc<Forwarder>) {
        for n in 0..self.capacity {
            let index = n;
            let mgr = self.clone();
            dnstunbuilder::connect(&mgr, index, self.server.get_tx());
        }

        self.clone().start_keepalive_timer();
    }

    pub fn on_tunnel_created(&self, tun: &Arc<DnsTunnel>) {
        info!("[Forwarder]on_tunnel_created");
        let index = tun.index;
        let mut tunnels = self.tunnels.lock();
        let t = &tunnels[index];

        if t.is_some() {
            panic!("[Forwarder]there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());

        info!("[Forwarder]tunnel created, index:{}", index);
    }

    pub fn on_tunnel_closed(&self, index: usize) {
        info!("[Forwarder]on_tunnel_closed");
        let t = self.on_tunnel_closed_interal(index);
        match t {
            Some(t) => {
                t.on_closed();
                if let Err(e) = self.reconnect_queue.push(index as u16) {
                    panic!("[Forwarder]reconnect_queue push failed:{}", e);
                }

                info!("[Forwarder]tunnel closed, index:{}", index);
            }
            None => {}
        }
    }

    fn on_tunnel_closed_interal(&self, index: usize) -> TunnelItem {
        info!("[Forwarder]on_tunnel_closed_interal");
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

    pub fn on_tunnel_build_error(&self, index: usize) {
        info!("[Forwarder]on_tunnel_build_error");
        if let Err(e) = self.reconnect_queue.push(index as u16) {
            panic!("[Forwarder]on_tunnel_build_error push failed:{}", e);
        }

        info!(
            "[Forwarder]tunnel build error, index:{}, rebuild later",
            index
        );
    }

    fn start_keepalive_timer(self: Arc<Forwarder>) {
        info!("[Forwarder]start_keepalive_timer");
        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .for_each(move |instant| {
                debug!("[Forwarder]keepalive timer fire; instant={:?}", instant);
                self.send_pings();

                self.clone().process_reconnect();

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[Forwarder]start_keepalive_timer interval errored; err={:?}",
                    e
                )
            });

        tokio::spawn(task);
    }

    fn send_pings(&self) {
        let tunnels = self.tunnels.lock();
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

    fn process_reconnect(self: Arc<Forwarder>) {
        loop {
            if let Ok(index) = self.reconnect_queue.pop() {
                info!("[Forwarder]process_reconnect, index:{}", index);

                dnstunbuilder::connect(&self, index as usize, self.server.get_tx());
            } else {
                break;
            }
        }
    }

    pub fn on_dns_udp_closed(&self) {
        error!("[Forwarder]on_dns_udp_closed")
    }

    pub fn on_dns_udp_msg(&self, message: &bytes::BytesMut, addr: &std::net::SocketAddr) -> bool {
        info!(
            "[Forwarder]on_dns_udp_msg, len:{}, src:{}",
            message.len(),
            addr
        );
        // select tunnel

        // write port, ip

        // write content

        true
    }
}
