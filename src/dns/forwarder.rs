use super::dnstunbuilder;
use super::DnsTunnel;
use crate::config::TunCfg;
use failure::Error;
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;

use super::UdpServer;
use log::{error, info};
type TunnelItem = Option<Rc<RefCell<DnsTunnel>>>;

type LongLife = Rc<RefCell<Forwarder>>;

pub struct Forwarder {
    pub udp_addr: String,
    pub relay_domain: String,
    pub relay_port: u16,
    pub dns_tun_url: String,

    tunnels: Vec<TunnelItem>,
    reconnect_queue: Vec<u16>,
    capacity: usize,

    server: Rc<RefCell<UdpServer>>,
    discarded: bool,
}

impl Forwarder {
    pub fn new(cfg: &TunCfg) -> Rc<RefCell<Forwarder>> {
        let capacity = cfg.dns_tunnel_number;

        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(None);
        }

        Rc::new(RefCell::new(Forwarder {
            udp_addr: cfg.dns_udp_addr.to_string(),
            dns_tun_url: cfg.dns_tun_url.to_string(),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            tunnels: vec,
            reconnect_queue: Vec::with_capacity(capacity),
            capacity: capacity,
            server: UdpServer::new(&cfg.dns_udp_addr),
            discarded: false,
        }))
    }

    pub fn init(&self, s: LongLife) -> Result<(), Error> {
        info!("[Forwarder]init");
        let mut serv = self.server.borrow_mut();
        serv.start(self, s)
    }

    pub fn on_dns_udp_created(&self, udps: &UdpServer, s: LongLife) {
        let tx = udps.get_tx();
        match tx {
            Some(tx) => {
                for n in 0..self.capacity {
                    let index = n;
                    dnstunbuilder::connect(self, s.clone(), index, tx.clone());
                }
            }
            None => {}
        }
    }

    pub fn on_tunnel_created(&mut self, tun: Rc<RefCell<DnsTunnel>>) {
        info!("[Forwarder]on_tunnel_created");
        if self.discarded != false {
            error!("[Forwarder]on_tunnel_created, forwarder is discarded, tun will be discarded");

            return;
        }

        let index = tun.borrow().index;
        let tunnels = &mut self.tunnels;
        let t = &tunnels[index];

        if t.is_some() {
            panic!("[Forwarder]there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());

        info!("[Forwarder]tunnel created, index:{}", index);
    }

    pub fn on_tunnel_closed(&mut self, index: usize) {
        info!("[Forwarder]on_tunnel_closed");
        let t = self.on_tunnel_closed_interal(index);
        match t {
            Some(t) => {
                t.borrow().on_closed();
                self.reconnect_queue.push(index as u16);

                info!("[Forwarder]tunnel closed, index:{}", index);
            }
            None => {}
        }
    }

    fn on_tunnel_closed_interal(&mut self, index: usize) -> TunnelItem {
        info!("[Forwarder]on_tunnel_closed_interal");
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

    pub fn on_tunnel_build_error(&mut self, index: usize) {
        info!("[Forwarder]on_tunnel_build_error");
        if self.discarded != false {
            error!(
                "[Forwarder]on_tunnel_build_error, forwarder is discarded, tun will not reconnect"
            );

            return;
        }

        self.reconnect_queue.push(index as u16);

        info!(
            "[Forwarder]tunnel build error, index:{}, rebuild later",
            index
        );
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

    fn process_reconnect(&mut self, s: LongLife) {
        let tx = self.server.borrow().get_tx();
        match tx {
            Some(tx) => loop {
                if let Some(index) = self.reconnect_queue.pop() {
                    info!("[Forwarder]process_reconnect, index:{}", index);

                    dnstunbuilder::connect(self, s.clone(), index as usize, tx.clone());
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
        if self.discarded != false {
            error!("[Forwarder]on_dns_udp_msg, forwarder is discarded, request will be discarded");

            return false;
        }

        // select tunnel
        let tun = self.alloc_tunnel_for_req();
        match tun {
            Some(tun) => {
                tun.borrow().on_dns_udp_msg(message, addr);
            }
            None => {
                error!("[Forwarder]on_dns_udp_msg, no tunnel");
            }
        }

        true
    }

    fn alloc_tunnel_for_req(&self) -> TunnelItem {
        info!("[Forwarder]alloc_tunnel_for_req");
        let tunnels = &self.tunnels;
        let mut tselected = None;
        let mut rtt = std::i64::MAX;

        for t in tunnels.iter() {
            match t {
                Some(tun2) => {
                    let tun = tun2.borrow();
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
                        tselected = Some(tun2.clone());
                    }
                }
                None => {}
            }
        }

        tselected
    }

    pub fn keepalive(&mut self, s: LongLife) {
        if self.discarded != false {
            error!("[Forwarder]keepalive, forwarder is discarded, not do keepalive");

            return;
        }

        self.send_pings();
        self.process_reconnect(s);
    }

    pub fn stop(&mut self) {
        if self.discarded != false {
            error!("[Forwarder]stop, forwarder is already discarded");

            return;
        }

        self.discarded = true;

        let s = &self.server;
        s.borrow_mut().stop();

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
}
