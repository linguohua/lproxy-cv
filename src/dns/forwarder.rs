use super::dnspacket::{BytePacketBuffer, DnsPacket, DnsRecord};
use super::dnstunbuilder;
use super::domap::DomainMap;
use super::netlink::{construct_ipset_packet, NLSocket};
use super::DnsTunnel;
use super::LocalResolver;
use super::UdpServer;
use crate::config::{
    TunCfg, IPSET_TABLE_NULL, KEEP_ALIVE_INTERVAL, LOCAL_SERVER, LOCAL_SERVER_PORT,
    IPSET_TABLE6_NULL,
};
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
    keepalive_trigger: Option<Trigger>,

    domap: DomainMap,

    lresolver: Rc<RefCell<LocalResolver>>,

    nsock: NLSocket,
}

impl Forwarder {
    pub fn new(cfg: &TunCfg, domain_array: Vec<String>) -> Rc<RefCell<Forwarder>> {
        let capacity = cfg.dns_tunnel_number;

        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(None);
        }

        let mut domap = DomainMap::new();
        for it in domain_array.iter() {
            domap.insert(&it);
        }

        info!("[Forwarder]insert {} domain into domap", domain_array.len());

        let local_addr = format!("{}:{}", LOCAL_SERVER, LOCAL_SERVER_PORT);
        let dns_server = UdpServer::new(&local_addr);
        Rc::new(RefCell::new(Forwarder {
            udp_addr: local_addr,
            dns_tun_url: cfg.dns_tun_url.to_string(),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            tunnels: vec,
            reconnect_queue: Vec::with_capacity(capacity),
            capacity: capacity,
            server: dns_server,
            discarded: false,
            keepalive_trigger: None,
            domap,
            lresolver: LocalResolver::new(&cfg.local_dns_server),
            nsock: NLSocket::new(),
        }))
    }

    pub fn init(&mut self, s: LongLife) -> Result<(), Error> {
        info!("[Forwarder]init");
        self.start_keepalive_timer(s.clone());

        let mut serv = self.server.borrow_mut();
        serv.start(self, s.clone())?;

        let mut lresolver = self.lresolver.borrow_mut();
        lresolver.start(self, s)
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

    pub fn on_lresolver_udp_created(&self, _udps: &LocalResolver, _s: LongLife) {}

    pub fn on_tunnel_created(
        &mut self,
        tun: Rc<RefCell<DnsTunnel>>,
    ) -> std::result::Result<(), ()> {
        info!("[Forwarder]on_tunnel_created");
        if self.discarded != false {
            error!("[Forwarder]on_tunnel_created, forwarder is discarded, tun will be discarded");

            return Err(());
        }

        let index = tun.borrow().index;
        let tunnels = &mut self.tunnels;
        let t = &tunnels[index];

        if t.is_some() {
            panic!("[Forwarder]there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());

        info!("[Forwarder]tunnel created, index:{}", index);

        Ok(())
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

    pub fn on_lresolver_udp_closed(&self) {
        error!("[Forwarder]on_lresolver_udp_closed")
    }

    pub fn on_resolver_udp_msg(
        &self,
        mut message: bytes::BytesMut,
        _addr: &std::net::SocketAddr,
    ) -> bool {
        if self.discarded != false {
            error!(
                "[Forwarder]on_resolver_udp_msg, forwarder is discarded, request will be discarded"
            );

            return false;
        }

        // parse dns reply, extract idendity and query name
        let mut bf = BytePacketBuffer::new(message.as_mut());
        // buf.copy_from_slice();
        let dnspacket = DnsPacket::from_buffer(&mut bf);
        match dnspacket {
            Ok(p) => {
                let q = &p.questions[0];
                let qname = &q.name;
                let identify = p.header.id;

                // use identity and query name to found target address
                let sockaddr = self.lresolver.borrow_mut().take(qname, identify);
                match sockaddr {
                    Some(sa) => {
                        let bm = bytes::Bytes::from(message);
                        self.server.borrow().reply(bm, sa);
                    }
                    None => {
                        error!(
                            "[Forwarder]on_resolver_udp_msg, no target address found for:{}",
                            qname
                        );
                    }
                }
                // send to target
            }

            Err(e) => {
                error!(
                    "[Forwarder]on_resolver_udp_msg parse dns packet failed:{}",
                    e
                );
            }
        }

        true
    }

    pub fn on_dns_udp_msg(
        &self,
        mut message: bytes::BytesMut,
        src_addr: std::net::SocketAddr,
    ) -> bool {
        if self.discarded != false {
            error!("[Forwarder]on_dns_udp_msg, forwarder is discarded, request will be discarded");

            return false;
        }

        // let buf = &mut bf.buf[0..message.len()];
        let mut bf = BytePacketBuffer::new(message.as_mut());
        // buf.copy_from_slice();
        let dnspacket = DnsPacket::from_buffer(&mut bf);
        match dnspacket {
            Ok(p) => {
                let q = &p.questions[0];

                if self.domap.has(&q.name) {
                    info!("[Forwarder]dns domain:{}, use remote resolver", q.name);
                    // select tunnel
                    let tun = self.alloc_tunnel_for_req();
                    match tun {
                        Some(tun) => {
                            tun.borrow().on_dns_udp_msg(message, src_addr);
                        }
                        None => {
                            error!("[Forwarder]on_dns_udp_msg, no tunnel");
                        }
                    }
                } else {
                    info!("[Forwarder]dns domain:{}, use local resolver", q.name);
                    let bm = bytes::Bytes::from(message);
                    self.lresolver
                        .borrow_mut()
                        .request(&q.name, p.header.id, bm, src_addr);
                }
            }

            Err(e) => {
                error!("[Forwarder]parse dns packet failed:{}", e);
            }
        }

        true
    }

    pub fn save_ipset(&self, p: &DnsPacket) {
        for a in p.answers.iter() {
            match a {
                DnsRecord::A {
                    domain: _,
                    addr,
                    ttl: _,
                } => {
                    info!("[Forwarder] try to save ipv4 into ipset:{}", addr);
                    let ipv4 = addr.octets();
                    let mut vec = vec![0 as u8; 256];
                    let len = construct_ipset_packet(IPSET_TABLE_NULL, &ipv4[..], &mut vec);
                    match self.nsock.send_to(&vec[..len as usize], 0) {
                        Err(e) => {
                            error!("[Forwarder] save ipset failed:{}", e);
                        }
                        _ => {}
                    }
                }
                DnsRecord::AAAA {
                    domain: _,
                    addr,
                    ttl: _,
                } => {
                    let ipv6 = addr.octets();
                    info!("[Forwarder] try to save ipv6 into ipset:{}", addr);
                    let mut vec = vec![0 as u8; 256];
                    let len = construct_ipset_packet(IPSET_TABLE6_NULL, &ipv6[..], &mut vec);
                    match self.nsock.send_to(&vec[..len as usize], 0) {
                        Err(e) => {
                            error!("[Forwarder] save ipset failed:{}", e);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
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

    fn save_keepalive_trigger(&mut self, trigger: Trigger) {
        self.keepalive_trigger = Some(trigger);
    }

    fn keepalive(&mut self, s: LongLife) {
        if self.discarded != false {
            error!("[Forwarder]keepalive, forwarder is discarded, not do keepalive");

            return;
        }

        self.lresolver.borrow_mut().keepalive();

        self.send_pings();
        self.process_reconnect(s);
    }

    pub fn stop(&mut self) {
        if self.discarded != false {
            error!("[Forwarder]stop, forwarder is already discarded");

            return;
        }

        self.discarded = true;
        self.keepalive_trigger = None;
        let s = &self.server;
        s.borrow_mut().stop();
        let s2 = &self.lresolver;
        s2.borrow_mut().stop();

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

    fn start_keepalive_timer(&mut self, s2: LongLife) {
        info!("[Forwarder]start_keepalive_timer");
        let (trigger, tripwire) = Tripwire::new();
        self.save_keepalive_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .skip(1)
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[Forwarder]keepalive timer fire; instant={:?}", instant);

                let mut rf = s2.borrow_mut();
                rf.keepalive(s2.clone());

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[Forwarder]start_keepalive_timer interval errored; err={:?}",
                    e
                )
            })
            .then(|_| {
                info!("[Forwarder] keepalive timer future completed");
                Ok(())
            });;

        current_thread::spawn(task);
    }
}
