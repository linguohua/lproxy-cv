use crate::config::TunCfg;
use crate::dns;
use crate::requests;
use crate::tunnels;
use crate::xport;
use futures::future::lazy;
use futures::stream::iter_ok;
use futures::stream::Stream;
use futures::sync::mpsc::{unbounded, UnboundedSender};
use futures::Future;
use log::{error, info};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use tokio::runtime::current_thread::{self, Runtime};
use tokio_tcp::TcpStream;

pub enum SubServiceCtlCmd {
    Stop,
    TcpTunnel(TcpStream),
}

impl fmt::Display for SubServiceCtlCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s;
        match self {
            SubServiceCtlCmd::Stop => s = "Stop",
            SubServiceCtlCmd::TcpTunnel(_) => s = "TcpTunnel",
        }
        write!(f, "({})", s)
    }
}

pub struct SubServiceCtl {
    pub handler: Option<std::thread::JoinHandle<()>>,
    pub ctl_tx: Option<UnboundedSender<SubServiceCtlCmd>>,
}

pub struct TunMgrStub {
    pub ctl_tx: UnboundedSender<SubServiceCtlCmd>,
}

fn start_forwarder(
    cfg: Arc<TunCfg>,
    domains: Vec<String>,
    r_tx: futures::Complete<bool>,
) -> SubServiceCtl {
    let (tx, rx) = unbounded();
    let handler = std::thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
        let fut = lazy(move || {
            let forwarder = dns::Forwarder::new(&cfg, domains);
            // thread code
            if let Err(e) = forwarder.borrow_mut().init(forwarder.clone()) {
                error!("[SubService]forwarder start failed:{}", e);

                r_tx.send(false).unwrap();
                return Ok(());
            }

            r_tx.send(true).unwrap();

            // wait control signals
            let fut = rx.for_each(move |cmd| {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = forwarder.clone();
                        f.borrow_mut().stop();
                    }
                    _ => {
                        error!("[SubService]forwarder unknown ctl cmd:{}", cmd);
                    }
                }

                Ok(())
            });

            current_thread::spawn(fut);

            Ok(())
        });

        rt.spawn(fut);
        rt.run().unwrap();
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
    }
}

fn start_xtunnel(cfg: Arc<TunCfg>, r_tx: futures::Complete<bool>) -> SubServiceCtl {
    let (tx, rx) = unbounded();
    let handler = std::thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
        let fut = lazy(move || {
            let xtun = xport::XTunnel::new(&cfg);
            // thread code
            if let Err(e) = xtun.borrow_mut().start(xtun.clone()) {
                error!("[SubService]xtun start failed:{}", e);

                r_tx.send(false).unwrap();
                return Ok(());
            }

            r_tx.send(true).unwrap();

            // wait control signals
            let fut = rx.for_each(move |cmd| {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = xtun.clone();
                        f.borrow_mut().stop();
                    }
                    _ => {
                        error!("[SubService]xtun unknown ctl cmd:{}", cmd);
                    }
                }

                Ok(())
            });

            current_thread::spawn(fut);

            Ok(())
        });

        rt.spawn(fut);
        rt.run().unwrap();
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
    }
}

fn start_reqmgr(
    cfg: Arc<TunCfg>,
    r_tx: futures::Complete<bool>,
    tmstubs: Vec<TunMgrStub>,
) -> SubServiceCtl {
    info!("[SubService]start_reqmgr, tm count:{}", tmstubs.len());

    let (tx, rx) = unbounded();
    let handler = std::thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
        let fut = lazy(move || {
            let reqmgr = requests::ReqMgr::new(&cfg, tmstubs);
            // thread code
            if let Err(e) = reqmgr.borrow_mut().init(reqmgr.clone()) {
                error!("[SubService]reqmgr start failed:{}", e);

                r_tx.send(false).unwrap();
                return Ok(());
            }

            r_tx.send(true).unwrap();

            // wait control signals
            let fut = rx.for_each(move |cmd| {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = reqmgr.clone();
                        f.borrow_mut().stop();
                    }
                    _ => {
                        error!("[SubService]reqmgr unknown ctl cmd:{}", cmd);
                    }
                }

                Ok(())
            });

            current_thread::spawn(fut);

            Ok(())
        });

        rt.spawn(fut);
        rt.run().unwrap();
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
    }
}

fn start_one_tunmgr(
    cfg: Arc<TunCfg>,
    r_tx: futures::Complete<bool>,
    tunnels_count: usize,
) -> SubServiceCtl {
    let (tx, rx) = unbounded();
    let handler = std::thread::spawn(move || {
        let mut rt = Runtime::new().unwrap();
        let fut = lazy(move || {
            let tunmgr = tunnels::TunMgr::new(tunnels_count, &cfg);
            // thread code
            if let Err(e) = tunmgr.borrow_mut().init(tunmgr.clone()) {
                error!("[SubService]tunmgr start failed:{}", e);

                r_tx.send(true).unwrap();
                return Ok(());
            }

            r_tx.send(true).unwrap();

            // wait control signals
            let fut = rx.for_each(move |cmd| {
                match cmd {
                    SubServiceCtlCmd::Stop => {
                        let f = tunmgr.clone();
                        f.borrow_mut().stop();
                    }
                    SubServiceCtlCmd::TcpTunnel(t) => {
                        tunnels::serve_sock(t, tunmgr.clone());
                    } // _ => {
                      //     error!("[SubService]tunmgr unknown ctl cmd:{}", cmd);
                      // }
                }

                Ok(())
            });

            current_thread::spawn(fut);

            Ok(())
        });

        rt.spawn(fut);
        rt.run().unwrap();
    });

    SubServiceCtl {
        handler: Some(handler),
        ctl_tx: Some(tx),
    }
}

fn to_future(
    rx: futures::Oneshot<bool>,
    ctrl: SubServiceCtl,
) -> impl Future<Item = SubServiceCtl, Error = ()> {
    let fut = rx
        .and_then(|v| if v { Ok(ctrl) } else { Err(futures::Canceled) })
        .or_else(|_| Err(()));

    fut
}

type SubsctlVec = Rc<RefCell<Vec<SubServiceCtl>>>;

fn start_tunmgr(cfg: std::sync::Arc<TunCfg>) -> impl Future<Item = SubsctlVec, Error = ()> {
    let cpus = num_cpus::get();
    let stream = iter_ok::<_, ()>(vec![0; cpus]);
    let subservices = Rc::new(RefCell::new(Vec::new()));
    let subservices2 = subservices.clone();
    let subservices3 = subservices.clone();
    let tunnels_per_mgr = cfg.tunnel_number / cpus;

    let fut = stream
        .for_each(move |_| {
            let (tx, rx) = futures::oneshot();
            let subservices = subservices.clone();
            to_future(rx, start_one_tunmgr(cfg.clone(), tx, tunnels_per_mgr)).and_then(move |ctl| {
                subservices.borrow_mut().push(ctl);
                Ok(())
            })
        })
        .and_then(move |_| Ok(subservices2))
        .or_else(move |_| {
            let vec_subservices = &mut subservices3.borrow_mut();
            // WARNING: block current thread
            cleanup_subservices(vec_subservices);

            Err(())
        });

    fut
}

pub fn start_subservice(
    cfg: std::sync::Arc<TunCfg>,
    domains: Vec<String>,
) -> impl Future<Item = SubsctlVec, Error = ()> {
    let cfg2 = cfg.clone();
    let cfg3 = cfg.clone();

    // start tunmgr first
    let tunmgr_fut = start_tunmgr(cfg.clone());

    let reqmgr_fut = tunmgr_fut.and_then(move |subservices| {
        let (tx, rx) = futures::oneshot();
        let mut tmstubs = Vec::new();
        {
            let ss = subservices.borrow();
            for s in ss.iter() {
                let tx = s.ctl_tx.as_ref().unwrap().clone();
                tmstubs.push(TunMgrStub { ctl_tx: tx });
            }
        }

        let v = subservices.clone();
        // then start reqmgr
        to_future(rx, start_reqmgr(cfg.clone(), tx, tmstubs))
            .and_then(|ctl| {
                subservices.borrow_mut().push(ctl);
                Ok(subservices)
            })
            .or_else(move |_| {
                let vec_subservices = &mut v.borrow_mut();
                // WARNING: block current thread
                cleanup_subservices(vec_subservices);
                Err(())
            })
    });

    // finally start forwarder
    let forward_fut = reqmgr_fut.and_then(move |subservices| {
        let (tx, rx) = futures::oneshot();
        let v = subservices.clone();
        to_future(rx, start_forwarder(cfg2.clone(), domains, tx))
            .and_then(|ctl| {
                subservices.borrow_mut().push(ctl);
                Ok(subservices)
            })
            .or_else(move |_| {
                let vec_subservices = &mut v.borrow_mut();
                // WARNING: block current thread
                cleanup_subservices(vec_subservices);
                Err(())
            })
    });

    // xtunnel
    let xtun_fut = forward_fut.and_then(move |subservices| {
        let (tx, rx) = futures::oneshot();
        let v = subservices.clone();
        to_future(rx, start_xtunnel(cfg3.clone(), tx))
            .and_then(|ctl| {
                subservices.borrow_mut().push(ctl);
                Ok(subservices)
            })
            .or_else(move |_| {
                let vec_subservices = &mut v.borrow_mut();
                // WARNING: block current thread
                cleanup_subservices(vec_subservices);
                Err(())
            })
    });

    xtun_fut
}

pub fn cleanup_subservices(subservices: &mut Vec<SubServiceCtl>) {
    for s in subservices.iter_mut() {
        let cmd = super::subservice::SubServiceCtlCmd::Stop;
        s.ctl_tx.as_ref().unwrap().unbounded_send(cmd).unwrap();
        s.ctl_tx = None;
    }

    // WARNING: thread block wait!
    for s in subservices.iter_mut() {
        let h = &mut s.handler;
        let h = h.take();
        h.unwrap().join().unwrap();

        info!("[SubService] jon handle completed");
    }
}
