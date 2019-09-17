use super::dnstunbuilder;
use super::DnsTunnel;
use crate::config::TunCfg;
use crossbeam::queue::ArrayQueue;
use failure::Error;
use parking_lot::Mutex;
use std::result::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::UdpServer;
use log::{error, info};
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
    discarded: AtomicBool,
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
            discarded: AtomicBool::new(false),
        })
    }

    pub fn init(self: Arc<Forwarder>) -> Result<(), Error> {
        info!("[Forwarder]init");
        self.server.clone().start(&self)
    }

    pub fn on_dns_udp_created(self: Arc<Forwarder>) {
        let tx = self.server.get_tx();
        match tx {
            Some(tx) => {
                for n in 0..self.capacity {
                    let index = n;
                    let mgr = self.clone();
                    dnstunbuilder::connect(&mgr, index, tx.clone());
                }
            }
            None => {}
        }
    }

    pub fn on_tunnel_created(&self, tun: &Arc<DnsTunnel>) {
        info!("[Forwarder]on_tunnel_created");
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[Forwarder]on_tunnel_created, forwarder is discarded, tun will be discarded");

            return;
        }

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
        if self.discarded.load(Ordering::SeqCst) != false {
            error!(
                "[Forwarder]on_tunnel_build_error, forwarder is discarded, tun will not reconnect"
            );

            return;
        }

        if let Err(e) = self.reconnect_queue.push(index as u16) {
            panic!("[Forwarder]on_tunnel_build_error push failed:{}", e);
        }

        info!(
            "[Forwarder]tunnel build error, index:{}, rebuild later",
            index
        );
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

    fn process_reconnect(self: Arc<Forwarder>) {
        let tx = self.server.get_tx();
        match tx {
            Some(tx) => loop {
                if let Ok(index) = self.reconnect_queue.pop() {
                    info!("[Forwarder]process_reconnect, index:{}", index);

                    dnstunbuilder::connect(&self, index as usize, tx.clone());
                } else {
                    break;
                }
            },
            None => {}
        }
    }

    pub fn on_dns_udp_closed(&self) {
        error!("[Forwarder]on_dns_udp_closed")
    }

    pub fn on_dns_udp_msg(&self, message: &bytes::BytesMut, addr: &std::net::SocketAddr) -> bool {
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[Forwarder]on_dns_udp_msg, forwarder is discarded, request will be discarded");

            return false;
        }

        // select tunnel
        let tun = self.alloc_tunnel_for_req();
        match tun {
            Some(tun) => {
                tun.on_dns_udp_msg(message, addr);
            }
            None => {
                error!("[Forwarder]on_dns_udp_msg, no tunnel");
            }
        }

        true
    }

    fn alloc_tunnel_for_req(&self) -> TunnelItem {
        info!("[Forwarder]alloc_tunnel_for_req");
        let tunnels = self.tunnels.lock();
        let mut tselected = None;
        let mut rtt = std::i64::MAX;

        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let rtt_tun = tun.get_rtt();

                    let mut selected = false;
                    info!(
                        "[Forwarder]alloc_tunnel_for_req, idx:{}, rtt:{}",
                        tun.index, rtt_tun,
                    );

                    if rtt_tun < rtt {
                        selected = true;
                    }
                    if selected {
                        rtt = rtt_tun;
                        tselected = Some(tun.clone());
                    }
                }
                None => {}
            }
        }

        tselected
    }

    pub fn keepalive(self: Arc<Forwarder>) {
        if self.discarded.load(Ordering::SeqCst) != false {
            error!("[Forwarder]keepalive, forwarder is discarded, not do keepalive");

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
            error!("[Forwarder]stop, forwarder is already discarded");

            return;
        }

        self.server.stop();

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
