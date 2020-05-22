use tokio::sync::mpsc::UnboundedSender;
use super::tunbuilder;
use super::Tunnel;
use super::{Request, TunStub};
use crate::config::{TunCfg, KEEP_ALIVE_INTERVAL};
use crate::service::{Instruction,TxType, SubServiceCtlCmd};
use failure::Error;
use log::{debug, error, info};
use std::cell::RefCell;
use std::net::IpAddr;
use std::rc::Rc;
use std::result::Result;
use std::time::{Duration};
use stream_cancel::{Trigger, Tripwire};
use futures_03::prelude::*;
use bytes::BytesMut;
use std::net::SocketAddr;
use fnv::FnvHashSet as HashSet;

type TunnelItem = Option<Rc<RefCell<Tunnel>>>;
type LongLive = Rc<RefCell<TunMgr>>;

pub struct TunMgr {
    pub relay_domain: String,
    pub relay_port: u16,
    pub url: String,
    capacity: usize,
    pub tunnel_req_cap: usize,
    pub request_quota: u16,
    tunnels: Vec<TunnelItem>,
    sorted_tun_indies: Vec<u16>,
    current_tun_idx: u16,
    reconnect_queue: Vec<u16>,
    discarded: bool,
    keepalive_trigger: Option<Trigger>,
    pub token: String,
    service_tx: TxType,
    udpx_tx: Option<UnboundedSender<SubServiceCtlCmd>>,
    access_log: HashSet<(std::net::IpAddr, std::net::IpAddr)>,
}

impl TunMgr {
    pub fn new(service_tx:TxType, tunnel_count: usize, cfg: &TunCfg) -> LongLive {
        info!("[TunMgr]new TunMgr");
        let capacity = tunnel_count;

        let mut vec = Vec::with_capacity(capacity);
        let mut sv = Vec::with_capacity(capacity);
        for n in 0..capacity {
            vec.push(None);
            sv.push(n as u16);
        }

        let token = cfg.token.to_string();
        Rc::new(RefCell::new(TunMgr {
            url: cfg.tunnel_url.to_string(),
            capacity: capacity,
            tunnel_req_cap: cfg.tunnel_req_cap,
            tunnels: vec,
            reconnect_queue: Vec::with_capacity(capacity),
            relay_domain: cfg.relay_domain.to_string(),
            relay_port: cfg.relay_port,
            discarded: false,
            keepalive_trigger: None,
            sorted_tun_indies: sv,
            current_tun_idx: 0,
            request_quota: cfg.request_quota as u16,
            token,
            service_tx:service_tx,
            udpx_tx: None,
            access_log: HashSet::default(),
        }))
    }

    pub fn init(&mut self, s: LongLive) -> Result<(), Error> {
        info!("[TunMgr]init");
        for n in 0..self.capacity {
            let index = n;
            let mgr = s.clone();
            tunbuilder::connect(self, mgr, index);
        }

        self.start_keepalive_timer(s);

        Ok(())
    }

    pub fn update_tuncfg(&mut self, cfg: &TunCfg) {
        self.url = cfg.tunnel_url.to_string();
        self.relay_domain = cfg.relay_domain.to_string();
        self.relay_port = cfg.relay_port;
        self.request_quota = cfg.request_quota as u16;
    }

    pub fn on_tunnel_created(&mut self, tun: Rc<RefCell<Tunnel>>) -> std::result::Result<(), ()> {
        info!("[TunMgr]on_tunnel_created");
        if self.discarded != false {
            error!("[TunMgr]on_tunnel_created, tunmgr is discarded, tun will be discarded");

            return Err(());
        }

        let index = tun.borrow().index;
        let tunnels = &mut self.tunnels;
        let t = &tunnels[index];

        if t.is_some() {
            panic!("[TunMgr]there is tunnel at {} already!", index);
        }

        tunnels[index] = Some(tun.clone());

        if self.udpx_tx.is_some() {
            tun.borrow_mut().set_udpx_tx(self.udpx_tx.as_ref().unwrap().clone());
        }

        info!("[TunMgr]tunnel created, index:{}", index);

        Ok(())
    }

    pub fn on_tunnel_closed(&mut self, index: usize) {
        info!("[TunMgr]on_tunnel_closed");
        let t = self.on_tunnel_closed_interal(index);

        match t {
            Some(t) => {
                let mut t = t.borrow_mut();
                t.on_closed();

                if self.discarded {
                    info!("[TunMgr]on_tunnel_closed, tunmgr is discarded, tun will be discard");
                    return;
                } else {
                    self.reconnect_queue.push(index as u16);
                }

                info!("[TunMgr]tunnel closed, index:{}", index);
            }
            None => {}
        }
    }

    pub fn on_tunnel_build_error(&mut self, index: usize) {
        info!("[TunMgr]on_tunnel_build_error");
        if self.discarded != false {
            error!("[TunMgr]on_tunnel_build_error, tunmgr is discarded, tun will be not reconnect");

            return;
        }

        self.reconnect_queue.push(index as u16);

        info!("[TunMgr]tunnel build error, index:{}, rebuild later", index);
    }

    fn on_tunnel_closed_interal(&mut self, index: usize) -> TunnelItem {
        info!("[TunMgr]on_tunnel_closed_interal");
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

    fn get_tunnel(&self, index: usize) -> TunnelItem {
        // info!("[TunMgr]get_tunnel");
        let tunnels = &self.tunnels;
        let tun = &tunnels[index];

        match tun {
            Some(tun) => Some(tun.clone()),
            None => None,
        }
    }

    pub fn on_request_created(&mut self, req: Request, ip: IpAddr, port: u16) -> Option<TunStub> {
        info!("[TunMgr]on_request_created");
        if self.discarded != false {
            error!("[TunMgr]on_request_created, tunmgr is discarded, request will be discarded");

            return None;
        }

        if let Some(tun) = self.alloc_tunnel_for_req() {
            let mut tun = tun.borrow_mut();
            tun.on_request_created(req, ip, port)
        } else {
            None
        }
    }

    pub fn on_request_closed(&mut self, tunstub: &TunStub) {
        info!("[TunMgr]on_request_closed:{:?}", tunstub);
        let tidx = tunstub.tun_idx;
        match self.get_tunnel(tidx as usize) {
            Some(tun) => {
                let mut tun = tun.borrow_mut();
                tun.on_request_closed(tunstub);
            }
            None => {
                error!("[TunMgr]on_request_closed:{:?}, not found", tunstub);
            }
        }
    }

    fn alloc_tunnel_for_req(&mut self) -> TunnelItem {
        info!("[TunMgr]alloc_tunnel_for_req");
        let length = self.sorted_tun_indies.len();
        let current_idx = self.current_tun_idx as usize;

        for n in current_idx..length {
            let tun_idx = self.sorted_tun_indies[n];
            let tun2 = &self.tunnels[tun_idx as usize];
            if tun2.is_none() {
                continue;
            }

            let tun2 = tun2.as_ref().unwrap();
            let tun = tun2.borrow();
            let req_count_tun = tun.get_req_count();
            // skip fulled tunnel
            if (req_count_tun + 1) >= tun.capacity {
                continue;
            }

            self.current_tun_idx = ((n + 1) % length) as u16;

            return Some(tun2.clone());
        }

        for n in 0..current_idx {
            let tun_idx = self.sorted_tun_indies[n];
            let tun2 = &self.tunnels[tun_idx as usize];
            if tun2.is_none() {
                continue;
            }

            let tun2 = tun2.as_ref().unwrap();
            let tun = tun2.borrow();
            let req_count_tun = tun.get_req_count();
            // skip fulled tunnel
            if (req_count_tun + 1) >= tun.capacity {
                continue;
            }

            self.current_tun_idx = ((n + 1) % length) as u16;

            return Some(tun2.clone());
        }

        return None;
    }

    fn sort_tunnels_by_busy(&mut self) {
        let tunnels = &self.tunnels;
        self.sorted_tun_indies.sort_by(|x, y| {
            let tun1 = &tunnels[*x as usize];
            let tun2 = &tunnels[*y as usize];

            if tun1.is_none() || tun2.is_none() {
                return std::cmp::Ordering::Equal;
            }

            let tun1 = tun1.as_ref().unwrap().borrow();
            let tun2 = tun2.as_ref().unwrap().borrow();

            let busy1 = tun1.get_busy();
            let busy2 = tun2.get_busy();

            busy1.cmp(&busy2)
        });

        let tunnels = &self.tunnels;
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let mut tun = tun.borrow_mut();
                    tun.reset_busy();
                }
                None => {}
            }
        }
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

    fn process_reconnect(&mut self, s: LongLive) {
        loop {
            if let Some(index) = self.reconnect_queue.pop() {
                info!("[TunMgr]process_reconnect, index:{}", index);

                tunbuilder::connect(self, s.clone(), index as usize);
            } else {
                break;
            }
        }
    }

    fn save_keepalive_trigger(&mut self, trigger: Trigger) {
        self.keepalive_trigger = Some(trigger);
    }

    fn keepalive(&mut self, s: LongLive) {
        if self.discarded != false {
            error!("[TunMgr]keepalive, tunmgr is discarded, not do keepalive");

            return;
        }

        self.sort_tunnels_by_busy();

        self.send_pings();
        self.process_reconnect(s.clone());

        // access report
        self.report_access_log();
    }

    pub fn stop(&mut self) {
        if self.discarded != false {
            error!("[TunMgr]stop, tunmgr is already discarded");

            return;
        }

        self.discarded = true;
        self.keepalive_trigger = None;

        // close all tunnel, and forbit reconnect
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

    fn start_keepalive_timer(&mut self, s2: LongLive) {
        info!("[TunMgr]start_keepalive_timer");
        let (trigger, tripwire) = Tripwire::new();
        self.save_keepalive_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = tokio::time::interval(Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .skip(1)
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[TunMgr]keepalive timer fire; instant={:?}", instant);

                let mut rf = s2.borrow_mut();
                rf.keepalive(s2.clone());

                future::ready(())
            });
            
        let t_fut = async move {
            task.await;
            info!("[TunMgr] keepalive timer future completed");
            ()
        };

        tokio::task::spawn_local(t_fut);
    }

    pub fn log_access(&mut self, peer_addr: std::net::IpAddr , target_ip : std::net::IpAddr) {
        // send to service
        self.access_log.insert((peer_addr, target_ip));
    }

    fn report_access_log(&mut self) {
        if self.access_log.len() < 1 {
            return;
        }

        let mut v = Vec::with_capacity(self.access_log.len());
        for k in self.access_log.drain() {
            v.push(k);
        }

        if let Err(e) = self.service_tx.send(Instruction::AccessLog(v)) {
            error!("[TunMgr]report_access_log unbounded_send failed:{}", e);
        }
    }

    pub fn udp_proxy_north(&mut self, msg: BytesMut, src_addr: SocketAddr, dst_addr: SocketAddr, hash_code:usize) {
        // select a tunnel, forward udp msg to dv via that tunnnel
        let tun_idx = hash_code % self.tunnels.len();
        
        match self.tunnels.get(tun_idx) {
            Some(tun_some) => {
                match tun_some {
                    Some(tun) => {
                        let tun = tun.borrow();
                        tun.udp_proxy_north(msg, src_addr, dst_addr);

                        self.access_log.insert((src_addr.ip(), dst_addr.ip()));
                    }
                    None => {
                        error!("[TunMgr]udp_proxy_north, no tunnel found at:{}", tun_idx);
                    }
                }
            }
            None => {}
        }
    }

    pub fn set_udpx_tx(&mut self, tx: UnboundedSender<SubServiceCtlCmd>) {
        let tunnels = &self.tunnels;
        for t in tunnels.iter() {
            match t {
                Some(tun) => {
                    let mut tun = tun.borrow_mut();
                    tun.set_udpx_tx(tx.clone());
                }
                None => {}
            }
        }
    }
}
