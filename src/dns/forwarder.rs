use super::dnspacket::{BytePacketBuffer, DnsPacket, DnsRecord};
use super::dnstunbuilder;
use super::domap::DomainMap;
use super::netlink::{self, construct_iphash_packet, construct_v4nethash_packet, NLSocket};
use super::DnsTunnel;
use super::LocalResolver;
use super::UdpServer;
use crate::config::{
    TunCfg, IPSET_NETHASH_TABLE_NULL, IPSET_TABLE6_NULL, IPSET_TABLE_NULL, KEEP_ALIVE_INTERVAL,
    LOCAL_DNS_SERVER_PORT, LOCAL_SERVER,
};
use crate::service::{DNSAddRecord, Instruction, TxType};
use failure::Error;
use futures_03::prelude::*;
use log::{debug, error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;
use std::time::Duration;
use stream_cancel::{Trigger, Tripwire};

type TunnelItem = Option<Rc<RefCell<DnsTunnel>>>;

type LongLife = Rc<RefCell<Forwarder>>;

pub struct Forwarder {
    pub udp_addr: String,
    pub relay_domain: String,
    pub relay_port: u16,
    pub dns_tun_url: String,
    dns_server: String,
    tunnels: Vec<TunnelItem>,
    reconnect_queue: Vec<u16>,
    capacity: usize,

    server: Rc<RefCell<UdpServer>>,
    discarded: bool,
    keepalive_trigger: Option<Trigger>,

    domap: DomainMap,

    lresolver: Rc<RefCell<LocalResolver>>,

    nsock: NLSocket,
    pub token: String,

    service_tx: TxType,

    work_as_global: bool,
}

impl Forwarder {
    pub fn new(
        service_tx: TxType,
        cfg: &TunCfg,
        domain_array: Vec<String>,
    ) -> Rc<RefCell<Forwarder>> {
        let capacity = cfg.dns_tunnel_number;

        let mut vec = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            vec.push(None);
        }

        let nsock = NLSocket::new();
        let domap = Forwarder::new_domap(&nsock, &domain_array);

        info!("[Forwarder]insert {} domain into domap", domain_array.len());

        let local_addr = format!("{}:{}", LOCAL_SERVER, LOCAL_DNS_SERVER_PORT);
        let my_dns_server = UdpServer::new(&local_addr);
        let token = cfg.token.to_string();
        Rc::new(RefCell::new(Forwarder {
            udp_addr: local_addr,
            dns_tun_url: cfg.tunnel_url.to_string(),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            dns_server: cfg.default_dns_server.to_string(),
            tunnels: vec,
            reconnect_queue: Vec::with_capacity(capacity),
            capacity: capacity,
            server: my_dns_server,
            discarded: false,
            keepalive_trigger: None,
            domap,
            lresolver: LocalResolver::new(&cfg.default_dns_server),
            nsock,
            token,
            service_tx,
            work_as_global: cfg.work_as_global,
        }))
    }

    pub fn update_domains(&mut self, domain_array: Vec<String>) {
        info!(
            "[Forwarder]update_domains, array len:{}",
            domain_array.len()
        );

        let domap = Forwarder::new_domap(&self.nsock, &domain_array);
        self.domap = domap;
    }

    fn new_domap(nsock: &NLSocket, domain_array: &[String]) -> DomainMap {
        let mut domap = DomainMap::new();
        for it in domain_array.iter() {
            match netlink::ipv4range_parse(&it) {
                Ok(ir) => {
                    if !ir.addr.is_loopback() && !ir.addr.is_multicast() {
                        if ir.mask < 32 {
                            let ipv4 = ir.addr.octets();
                            Forwarder::ipset_add_nethash(nsock, &ipv4[..], ir.mask);
                        } else {
                            let ipv4 = ir.addr.octets();
                            Forwarder::ipset_add_iphash(nsock, &ipv4[..], IPSET_TABLE_NULL);
                        }
                    }
                }
                Err(_) => {
                    domap.insert(&it);
                }
            }
        }

        domap
    }

    pub fn init(&mut self, s: LongLife) -> Result<(), Error> {
        info!("[Forwarder]init");
        self.start_keepalive_timer(s.clone());

        {
            let mut serv = self.server.borrow_mut();
            serv.start(self, s.clone())?;
        }

        {
            let mut lresolver = self.lresolver.borrow_mut();
            lresolver.start(self, s)
        }
    }

    pub fn on_dns_udp_created(&self, udps: &UdpServer, s: LongLife) {
        let tx = udps.get_tx();
        match tx {
            Some(tx) => {
                for n in 0..self.capacity {
                    let index = n;
                    dnstunbuilder::connect(self, s.clone(), index, tx.clone(), self.dns_server.to_string());
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

                    dnstunbuilder::connect(self, s.clone(), index as usize, tx.clone(), self.dns_server.to_string());
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

    pub fn on_lresolver_udp_closed(&mut self) {
        error!("[Forwarder]on_lresolver_udp_closed");
        self.lresolver.borrow_mut().invalid_tx();
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

                if self.work_as_global || self.domap.has(&q.name) {
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
                        .request(self, &q.name, p.header.id, bm, src_addr);
                }
            }

            Err(e) => {
                error!("[Forwarder]parse dns packet failed:{}", e);
            }
        }

        true
    }

    pub fn save_ipset(&self, p: &DnsPacket) {
        if !self.work_as_global {
            for a in p.answers.iter() {
                match a {
                    DnsRecord::A {
                        domain: _,
                        addr,
                        ttl: _,
                    } => {
                        if !addr.is_loopback() && !addr.is_multicast() {
                            info!("[Forwarder] try to save ipv4 into ipset:{}", addr);
                            let ipv4 = addr.octets();
                            Forwarder::ipset_add_iphash(&self.nsock, &ipv4[..], IPSET_TABLE_NULL);
                        }
                    }
                    DnsRecord::AAAA {
                        domain: _,
                        addr,
                        ttl: _,
                    } => {
                        if !addr.is_loopback() && !addr.is_multicast() {
                            let ipv6 = addr.octets();
                            info!("[Forwarder] try to save ipv6 into ipset:{}", addr);
                            Forwarder::ipset_add_iphash(&self.nsock, &ipv6[..], IPSET_TABLE6_NULL);
                        }
                    }
                    _ => {}
                }
            }
        }

        self.save_domain_record(p);
    }

    fn save_domain_record(&self, p: &DnsPacket) {
        let mut da = DNSAddRecord::new();
        for a in p.answers.iter() {
            match a {
                DnsRecord::A {
                    domain: d,
                    addr,
                    ttl: _,
                } => {
                    da.add(std::net::IpAddr::V4(*addr), d);
                }
                DnsRecord::AAAA {
                    domain: d,
                    addr,
                    ttl: _,
                } => {
                    da.add(std::net::IpAddr::V6(*addr), d);
                }
                _ => {}
            }
        }

        if let Err(e) = self.service_tx.send(Instruction::DNSAdd(da)) {
            error!("[Forwarder]save_ipset unbounded_send failed:{}", e);
        }
    }

    fn ipset_add_iphash(sock: &NLSocket, ipvec: &[u8], tbname: &str) {
        let mut vec = vec![0 as u8; 256];
        let len = construct_iphash_packet(tbname, ipvec, &mut vec);
        match sock.send_to(&vec[..len as usize], 0) {
            Err(e) => {
                error!("[Forwarder] save ipset failed:{}", e);
            }
            _ => {}
        }
    }

    fn ipset_add_nethash(sock: &NLSocket, ipvec: &[u8], mask: u8) {
        let mut vec = vec![0 as u8; 256];
        let len = construct_v4nethash_packet(IPSET_NETHASH_TABLE_NULL, ipvec, mask, &mut vec);
        match sock.send_to(&vec[..len as usize], 0) {
            Err(e) => {
                error!("[Forwarder] save ipset failed:{}", e);
            }
            _ => {}
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
        let task = tokio::time::interval(Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .skip(1)
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[Forwarder]keepalive timer fire; instant={:?}", instant);

                let mut rf = s2.borrow_mut();
                rf.keepalive(s2.clone());

                future::ready(())
            });

        let t_fut = async move {
            task.await;
            info!("[Forwarder] keepalive timer future completed");
            ()
        };
        tokio::task::spawn_local(t_fut);
    }
}
