use crate::config::TunCfg;
use crate::dns;
use crate::requests;
use crate::tunnels;
use crate::udpx;
use crate::xport;

use bytes::BytesMut;
use futures_03::channel::oneshot;
use futures_03::prelude::*;
use log::{error, info};
use std::cell::RefCell;
use std::fmt;
use std::net::SocketAddr;
use std::rc::Rc;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

pub enum SubServiceCtlCmd {
    Stop,
    TcpTunnel(TcpStream),
    DomainsUpdate(Vec<String>),
    TunCfgUpdate(Arc<TunCfg>),
    UdpProxy((BytesMut, SocketAddr, SocketAddr, usize)),
    UdpRecv((bytes::Bytes, SocketAddr, SocketAddr)),
    SetUdpTx(UnboundedSender<SubServiceCtlCmd>),
}

pub enum SubServiceType {
    Forwarder,
    RegMgr,
    TunMgr,
    XTunnel,
    UdpX,
}

impl fmt::Display for SubServiceCtlCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s;
        match self {
            SubServiceCtlCmd::Stop => s = "Stop",
            SubServiceCtlCmd::TcpTunnel(_) => s = "TcpTunnel",
            SubServiceCtlCmd::DomainsUpdate(_) => s = "DomainUpdate",
            SubServiceCtlCmd::TunCfgUpdate(_) => s = "TunCfgUpdate",
            SubServiceCtlCmd::UdpProxy(_) => s = "UdpProxy",
            SubServiceCtlCmd::UdpRecv(_) => s = "UdpRecv",
            SubServiceCtlCmd::SetUdpTx(_) => s = "SetUdpTx",
        }
        write!(f, "({})", s)
    }
}

impl fmt::Display for SubServiceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s;
        match self {
            SubServiceType::Forwarder => s = "Forwarder",
            SubServiceType::RegMgr => s = "RegMgr",
            SubServiceType::TunMgr => s = "TunMgr",
            SubServiceType::XTunnel => s = "XTunnel",
            SubServiceType::UdpX => s = "UdpX",
        }
        write!(f, "({})", s)
    }
}

pub struct SubServiceCtl {
    pub handler: Option<std::thread::JoinHandle<()>>,
    pub ctl_tx: Option<UnboundedSender<SubServiceCtlCmd>>,
    pub sstype: SubServiceType,
}

pub struct TunMgrStub {
    pub ctl_tx: UnboundedSender<SubServiceCtlCmd>,
}

fn start_forwarder(
    service_tx: super::TxType,
    cfg: Arc<TunCfg>,
    domains: Vec<String>,
    r_tx: oneshot::Sender<bool>,
) -> SubServiceCtl {
    let (tx, mut rx) = unbounded_channel();
    let handler = std::thread::spawn(move || {
        let mut basic_rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()
            .unwrap();

        // let handle = rt.handle();
        let local = tokio::task::LocalSet::new();

        let fut = async move {
            let forwarder = dns::Forwarder::new(service_tx, &cfg, domains);
            // thread code
            if let Err(e) = forwarder.borrow_mut().init(forwarder.clone()) {
                error!("[SubService]forwarder start failed:{}", e);

                r_tx.send(false).unwrap();
                return;
            }

            r_tx.send(true).unwrap();

            // wait control signals
            while let Some(cmd) = rx.next().await {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = forwarder.clone();
                        f.borrow_mut().stop();
                        break;
                    }
                    SubServiceCtlCmd::DomainsUpdate(domains) => {
                        let f = forwarder.clone();
                        f.borrow_mut().update_domains(domains);
                    }
                    _ => {
                        error!("[SubService]forwarder unknown ctl cmd:{}", cmd);
                    }
                }
            }
        };

        local.spawn_local(fut);
        basic_rt.block_on(local);
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
        sstype: SubServiceType::Forwarder,
    }
}

fn start_xtunnel(cfg: Arc<TunCfg>, r_tx: oneshot::Sender<bool>) -> SubServiceCtl {
    let (tx, mut rx) = unbounded_channel();
    let handler = std::thread::spawn(move || {
        let mut basic_rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()
            .unwrap();
        // let handle = rt.handle();
        let local = tokio::task::LocalSet::new();

        let fut = async move {
            let xtun = xport::XTunnel::new(&cfg);
            // thread code
            if let Err(e) = xtun.borrow_mut().start(xtun.clone()) {
                error!("[SubService]xtun start failed:{}", e);

                r_tx.send(false).unwrap();
                return;
            }

            r_tx.send(true).unwrap();

            // wait control signals
            while let Some(cmd) = rx.next().await {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = xtun.clone();
                        f.borrow_mut().stop();
                        break;
                    }
                    _ => {
                        error!("[SubService]xtun unknown ctl cmd:{}", cmd);
                        break;
                    }
                }
            }
        };

        local.spawn_local(fut);
        basic_rt.block_on(local);
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
        sstype: SubServiceType::XTunnel,
    }
}

fn start_reqmgr(
    cfg: Arc<TunCfg>,
    r_tx: oneshot::Sender<bool>,
    tmstubs: Vec<TunMgrStub>,
) -> SubServiceCtl {
    info!("[SubService]start_reqmgr, tm count:{}", tmstubs.len());

    let (tx, mut rx) = unbounded_channel();
    let handler = std::thread::spawn(move || {
        let mut basic_rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()
            .unwrap();
        // let handle = rt.handle();
        let local = tokio::task::LocalSet::new();

        let fut = async move {
            let reqmgr = requests::ReqMgr::new(&cfg, tmstubs);
            // thread code
            if let Err(e) = reqmgr.borrow_mut().init(reqmgr.clone()) {
                error!("[SubService]reqmgr start failed:{}", e);

                r_tx.send(false).unwrap();
                return;
            }

            r_tx.send(true).unwrap();

            // wait control signals
            while let Some(cmd) = rx.next().await {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = reqmgr.clone();
                        f.borrow_mut().stop();
                        break;
                    }
                    _ => {
                        error!("[SubService]reqmgr unknown ctl cmd:{}", cmd);
                    }
                }
            }
        };

        local.spawn_local(fut);
        basic_rt.block_on(local);
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
        sstype: SubServiceType::RegMgr,
    }
}

fn start_udpx(
    cfg: Arc<TunCfg>,
    r_tx: oneshot::Sender<bool>,
    tmstubs: Vec<TunMgrStub>,
) -> SubServiceCtl {
    info!("[SubService]start_udpx, tm count:{}", tmstubs.len());

    let (tx, mut rx) = unbounded_channel();
    let tx2 = tx.clone();
    let handler = std::thread::spawn(move || {
        let mut basic_rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()
            .unwrap();
        // let handle = rt.handle();
        let local = tokio::task::LocalSet::new();

        let fut = async move {
            let udpx = udpx::UdpXMgr::new(&cfg, tmstubs, tx);
            // thread code
            if let Err(e) = udpx.borrow_mut().init(udpx.clone()) {
                error!("[SubService]udpx start failed:{}", e);

                r_tx.send(false).unwrap();
                return;
            }

            r_tx.send(true).unwrap();

            // wait control signals
            while let Some(cmd) = rx.next().await {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = udpx.clone();
                        f.borrow_mut().stop();
                        break;
                    }
                    SubServiceCtlCmd::UdpRecv((msg, src_addr, dst_addr)) => {
                        let f = udpx.clone();
                        f.borrow_mut()
                            .on_udp_proxy_south(udpx.clone(), msg, src_addr, dst_addr);
                    }
                    _ => {
                        error!("[SubService]udpx unknown ctl cmd:{}", cmd);
                    }
                }
            }
        };

        local.spawn_local(fut);
        basic_rt.block_on(local);
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx2),
        sstype: SubServiceType::UdpX,
    }
}

fn start_one_tunmgr(
    service_tx: super::TxType,
    cfg: Arc<TunCfg>,
    r_tx: oneshot::Sender<bool>,
    tunnels_count: usize,
) -> SubServiceCtl {
    let (tx, mut rx) = unbounded_channel();
    let handler = std::thread::spawn(move || {
        let mut basic_rt = tokio::runtime::Builder::new()
            .basic_scheduler()
            .enable_all()
            .build()
            .unwrap();
        // let handle = rt.handle();
        let local = tokio::task::LocalSet::new();

        let fut = async move {
            let tunmgr = tunnels::TunMgr::new(service_tx.clone(), tunnels_count, &cfg);
            // thread code
            if let Err(e) = tunmgr.borrow_mut().init(tunmgr.clone()) {
                error!("[SubService]tunmgr start failed:{}", e);

                r_tx.send(true).unwrap();
                return;
            }

            r_tx.send(true).unwrap();

            // wait control signals
            while let Some(cmd) = rx.next().await {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = tunmgr.clone();
                        f.borrow_mut().stop();
                        break;
                    }
                    SubServiceCtlCmd::TcpTunnel(t) => {
                        tunnels::serve_sock(t, tunmgr.clone());
                    }
                    SubServiceCtlCmd::TunCfgUpdate(tuncfg) => {
                        let f = tunmgr.clone();
                        f.borrow_mut().update_tuncfg(&tuncfg);
                    }
                    SubServiceCtlCmd::UdpProxy((msg, src_addr, dst_addr, hash_code)) => {
                        let f = tunmgr.clone();
                        f.borrow_mut()
                            .udp_proxy_north(msg, src_addr, dst_addr, hash_code);
                    }
                    SubServiceCtlCmd::SetUdpTx(tx) => {
                        let f = tunmgr.clone();
                        f.borrow_mut().set_udpx_tx(tx);
                    }
                    _ => {
                        error!("[SubService]tunmgr unknown ctl cmd:{}", cmd);
                    }
                }
            }

            info!("tunmgr sub-service rx fut exit");
        };

        local.spawn_local(fut);
        basic_rt.block_on(local);
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
        sstype: SubServiceType::TunMgr,
    }
}

async fn to_future(
    rx: oneshot::Receiver<bool>,
    ctrl: SubServiceCtl,
) -> std::result::Result<SubServiceCtl, ()> {
    match rx.await {
        Ok(v) => match v {
            true => Ok(ctrl),
            false => Err(()),
        },
        Err(_) => Err(()),
    }
}

type SubsctlVec = Rc<RefCell<Vec<SubServiceCtl>>>;

async fn start_tunmgr(
    service_tx: super::TxType,
    cfg: std::sync::Arc<TunCfg>,
) -> std::result::Result<SubsctlVec, SubsctlVec> {
    let cpus = num_cpus::get();
    let sv = stream::iter(vec![0; cpus]);
    let subservices = Rc::new(RefCell::new(Vec::new()));
    let subservices2 = subservices.clone();
    let subservices3 = subservices.clone();
    let failed = Rc::new(RefCell::new(false));
    let failed3 = failed.clone();
    let mut tunnels_per_mgr = cfg.tunnel_number / cpus;
    if tunnels_per_mgr < 1 {
        tunnels_per_mgr = 1;
    }

    let fut = sv.for_each(move |_| {
        let (tx, rx) = oneshot::channel();
        let subservices22 = subservices2.clone();
        let stx = service_tx.clone();
        let cfgx = cfg.clone();
        let failed2 = failed.clone();
        let ctl = async move {
            let ct = to_future(rx, start_one_tunmgr(stx, cfgx, tx, tunnels_per_mgr)).await;

            match ct {
                Ok(c) => {
                    subservices22.borrow_mut().push(c);
                }
                _ => {
                    *failed2.borrow_mut() = true;
                }
            }
        };

        ctl
    });

    fut.await;

    if *failed3.borrow() {
        Err(subservices3)
    } else {
        Ok(subservices3)
    }
}

pub async fn start_subservice(
    service_tx: super::TxType,
    cfg: std::sync::Arc<TunCfg>,
    domains: Vec<String>,
) -> std::result::Result<SubsctlVec, ()> {
    let cfg2 = cfg.clone();
    let cfg3 = cfg.clone();

    let ff = async move {
        // start tunmgr first
        let subservices = start_tunmgr(service_tx.clone(), cfg.clone()).await?;
        let (tx, rx) = oneshot::channel();
        let mut tmstubs = Vec::new();
        {
            let ss = subservices.borrow();
            for s in ss.iter() {
                let tx = s.ctl_tx.as_ref().unwrap().clone();
                tmstubs.push(TunMgrStub { ctl_tx: tx });
            }
        }
        let mut tmstubs2 = Vec::new();
        {
            let ss = subservices.borrow();
            for s in ss.iter() {
                let tx = s.ctl_tx.as_ref().unwrap().clone();
                tmstubs2.push(TunMgrStub { ctl_tx: tx });
            }
        }

        match to_future(rx, start_reqmgr(cfg.clone(), tx, tmstubs)).await {
            Ok(ctl) => subservices.borrow_mut().push(ctl),
            Err(_) => return Err(subservices),
        }

        let (tx, rx) = oneshot::channel();
        match to_future(
            rx,
            start_forwarder(service_tx.clone(), cfg2.clone(), domains, tx),
        )
        .await
        {
            Ok(ctl) => subservices.borrow_mut().push(ctl),
            Err(_) => return Err(subservices),
        }

        let (tx, rx) = oneshot::channel();
        match to_future(rx, start_udpx(cfg2.clone(), tx, tmstubs2)).await {
            Ok(ctl) => subservices.borrow_mut().push(ctl),
            Err(_) => return Err(subservices),
        }

        if cfg3.xport_url.len() > 0 {
            let (tx, rx) = oneshot::channel();
            match to_future(rx, start_xtunnel(cfg3.clone(), tx)).await {
                Ok(ctl) => subservices.borrow_mut().push(ctl),
                Err(_) => return Err(subservices),
            }    
        }

        Ok(subservices)
    };

    match ff.await {
        Ok(v) => Ok(v),
        Err(v) => {
            let vec_subservices = &mut v.borrow_mut();
            // WARNING: block current thread
            cleanup_subservices(vec_subservices);
            Err(())
        }
    }
}

pub fn cleanup_subservices(subservices: &mut Vec<SubServiceCtl>) {
    for s in subservices.iter_mut() {
        let cmd = super::subservice::SubServiceCtlCmd::Stop;
        if let Err(e) = s.ctl_tx.as_ref().unwrap().send(cmd) {
            info!("[SubService] cleanup_subservices send failed:{}", e);
        }
        s.ctl_tx = None;
    }

    // WARNING: thread block wait!
    for s in subservices.iter_mut() {
        info!("[SubService] type {} try to join handle", s.sstype);
        let h = &mut s.handler;
        let h = h.take();
        h.unwrap().join().unwrap();

        info!("[SubService] type {} join handle completed", s.sstype);
    }
}
