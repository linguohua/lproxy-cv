use crate::auth;
use crate::config;
use crate::dns;
use crate::dns::Forwarder;
use crate::requests;
use crate::requests::ReqMgr;
use crate::tunnels;
use crate::tunnels::TunMgr;

use crate::config::KEEP_ALIVE_INTERVAL;
use futures::sync::mpsc::UnboundedSender;
use log::{debug, error, info};
use parking_lot::Mutex;
use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::prelude::*;
use tokio::timer::Delay;
use tokio::timer::Interval;
const STATE_STOPPED: u8 = 0;
const STATE_STARTING: u8 = 1;
const STATE_RUNNING: u8 = 2;
const STATE_STOPPING: u8 = 3;

enum Instruction {
    Auth,
    StartSubServices,
    KeepAlive,
    ServerCfgMonitor,
    Stop,
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s;
        match self {
            Instruction::Auth => s = "Auth",
            Instruction::StartSubServices => s = "StartSubServices",
            Instruction::KeepAlive => s = "KeepAlive",
            Instruction::ServerCfgMonitor => s = "ServerCfgMonitor",
            Instruction::Stop => s = "Stop",
        }
        write!(f, "({})", s)
    }
}

type TxType = UnboundedSender<Instruction>;

struct SubServices {
    pub forwarder: Option<Arc<Forwarder>>,
    pub tunmgr: Option<Arc<TunMgr>>,
    pub reqmgr: Option<Arc<ReqMgr>>,
    pub ins_tx: Option<TxType>,
    pub tuncfg: Option<config::TunCfg>,
    pub keepalive_trigger: Option<Trigger>,
    pub instruction_trigger: Option<Trigger>,
}

pub struct Service {
    state: AtomicU8,
    ss: Mutex<SubServices>,
}

impl Service {
    pub fn new() -> Arc<Service> {
        let ss = SubServices {
            forwarder: None,
            tunmgr: None,
            reqmgr: None,
            ins_tx: None,
            tuncfg: None,
            keepalive_trigger: None,
            instruction_trigger: None,
        };

        Arc::new(Service {
            ss: Mutex::new(ss),
            state: AtomicU8::new(0),
        })
    }

    // start config monitor
    pub fn start(self: Arc<Service>) {
        if self
            .state
            .compare_and_swap(STATE_STOPPED, STATE_STARTING, Ordering::SeqCst)
            == STATE_STOPPED
        {
            let (tx, rx) = futures::sync::mpsc::unbounded();
            let (trigger, tripwire) = Tripwire::new();
            self.save_instruction_trigger(trigger);

            let s = self.clone();
            let fut = rx
                .take_until(tripwire)
                .for_each(move |ins| {
                    s.clone().process_instruction(ins);
                    Ok(())
                })
                .then(|_| {
                    info!("[Service] instruction rx future completed");

                    Ok(())
                });

            self.save_tx(Some(tx));
            self.fire_instruction(Instruction::Auth);

            tokio::spawn(fut);
        } else {
            panic!("[Service] start failed, state not stopped");
        }
    }

    fn process_instruction(self: Arc<Service>, ins: Instruction) {
        match ins {
            Instruction::Auth => {
                self.clone().do_auth();
            }
            Instruction::StartSubServices => {
                self.clone().do_start_subservices();
            }
            Instruction::KeepAlive => {
                self.do_keepalive();
            }
            Instruction::ServerCfgMonitor => {
                self.do_cfg_monitor();
            }
            Instruction::Stop => {
                self.do_stop();
            }
        }
    }

    fn do_auth(self: Arc<Service>) {
        info!("[Service]do_auth");
        let httpserver = config::server_url();
        let req = auth::HTTPRequest::new(&httpserver).unwrap();
        let sclone = self.clone();
        let fut = req
            .exec()
            .and_then(move |response| {
                debug!("[Service]do_auth http response:{:?}", response);

                // TODO: use reponse to init TunCfg
                let cfg = config::TunCfg::new();
                sclone.save_cfg(cfg);
                sclone.fire_instruction(Instruction::StartSubServices);
                Ok(())
            })
            .or_else(move |e| {
                let seconds = 5;
                error!(
                    "[Service]do_auth http request failed, error:{}, retry {} seconds later",
                    e, seconds
                );

                self.delay_post_instruction(seconds, Instruction::Auth);

                Ok(())
            });

        tokio::spawn(fut);
    }

    fn do_cfg_monitor(self: Arc<Service>) {
        info!("[Service]do_cfg_monitor");

        let httpserver = config::server_url();
        let req = auth::HTTPRequest::new(&httpserver).unwrap();
        let sclone = self.clone();
        let fut = req
            .exec()
            .and_then(move |response| {
                debug!("[Service]do_cfg_monitor http response:{:?}", response);

                // TODO: use reponse to init TunCfg
                let cfg = config::TunCfg::new();
                sclone.save_cfg(cfg);
                sclone.fire_instruction(Instruction::Stop);

                Ok(())
            })
            .or_else(move |e| {
                let seconds = 5;
                error!(
                    "[Service]do_cfg_monitor http request failed, error:{}, retry {} seconds later",
                    e, seconds
                );

                self.delay_post_instruction(seconds, Instruction::Auth);

                Ok(())
            });

        tokio::spawn(fut);
    }

    fn delay_post_instruction(self: Arc<Service>, seconds: u64, ins: Instruction) {
        info!(
            "[Service]delay_post_instruction, seconds:{}, ins:{}",
            seconds, ins
        );

        // delay 5 seconds
        let when = Instant::now() + Duration::from_millis(seconds * 1000);
        let task = Delay::new(when)
            .and_then(move |_| {
                debug!("[Service]delay_post_instruction retry...");
                self.fire_instruction(ins);

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[Service]delay_post_instruction delay retry errored, err={:?}",
                    e
                );
                ()
            });

        tokio::spawn(task);
    }

    fn do_start_subservices(self: Arc<Service>) {
        info!("[Service]do_start_subservices");
        if self.start_subservices() {
            self.start_keepalive_timer();
        } else {
            // re-auth
            self.delay_post_instruction(5, Instruction::Auth);
        }
    }

    fn do_stop(&self) {
        info!("[Service]do_stop");
        if self
            .state
            .compare_and_swap(STATE_RUNNING, STATE_STOPPING, Ordering::SeqCst)
            != STATE_RUNNING
        {
            error!("[Service] do_stop failed, state not running");
            return;
        }

        let mut ss = self.ss.lock();
        // drop trigger will complete keepalive future
        ss.keepalive_trigger = None;

        // drop trigger will completed instruction future
        ss.instruction_trigger = None;

        // stop all subservices
        match &ss.forwarder {
            Some(s) => {
                s.stop();
                ss.forwarder = None;
            }
            None => {}
        }

        match &ss.reqmgr {
            Some(s) => {
                s.stop();
                ss.reqmgr = None;
            }
            None => {}
        }

        match &ss.tunmgr {
            Some(s) => {
                s.stop();
                ss.tunmgr = None;
            }
            None => {}
        }

        self.restore_sys();
    }

    fn save_tx(&self, tx: Option<TxType>) {
        let mut ss = self.ss.lock();
        ss.ins_tx = tx;
    }

    fn save_cfg(&self, cfg: config::TunCfg) {
        let mut ss = self.ss.lock();
        ss.tuncfg = Some(cfg);
    }

    fn save_keepalive_trigger(&self, trigger: Trigger) {
        let mut ss = self.ss.lock();
        ss.keepalive_trigger = Some(trigger);
    }

    fn save_instruction_trigger(&self, trigger: Trigger) {
        let mut ss = self.ss.lock();
        ss.instruction_trigger = Some(trigger);
    }

    fn fire_instruction(&self, ins: Instruction) {
        debug!("[Service]fire_instruction, ins:{}", ins);
        let ss = self.ss.lock();
        let tx = &ss.ins_tx;
        match tx.as_ref() {
            Some(ref tx) => {
                if let Err(e) = tx.unbounded_send(ins) {
                    error!("[Service]fire_instruction failed:{}", e);
                }
            }
            None => {}
        }
    }

    fn start_subservices(&self) -> bool {
        info!("[Service]start_subservices");
        let mut ss = self.ss.lock();
        let cfg = ss.tuncfg.as_ref().unwrap();
        let tunmgr = tunnels::TunMgr::new(cfg);
        let reqmgr = requests::ReqMgr::new(&tunmgr, cfg);
        let forwarder = dns::Forwarder::new(cfg);

        if let Err(e) = forwarder.clone().init() {
            error!("[Service]forwarder start failed:{}", e);
            return false;
        }

        if let Err(e) = reqmgr.clone().init() {
            error!("[Service]reqmgr start failed:{}", e);
            forwarder.stop();
            return false;
        }

        if let Err(e) = tunmgr.clone().init() {
            error!("[Service]tunmgr start failed:{}", e);
            forwarder.stop();
            reqmgr.stop();

            return false;
        }

        self.config_sys();

        // save sub-services
        ss.forwarder = Some(forwarder);
        ss.tunmgr = Some(tunmgr);
        ss.reqmgr = Some(reqmgr);

        // change state to running
        if self
            .state
            .compare_and_swap(STATE_STARTING, STATE_RUNNING, Ordering::SeqCst)
            != STATE_STARTING
        {
            panic!("[Service] start_subservice failed, state not STARTING");
        }

        true
    }

    pub fn config_sys(&self) {
        info!("[Service]config_sys");
    }

    pub fn restore_sys(&self) {
        info!("[Service]restore_sys");
    }

    fn start_keepalive_timer(self: Arc<Service>) {
        info!("[Service]start_keepalive_timer");
        let mut counter300 = 0;

        let (trigger, tripwire) = Tripwire::new();
        self.save_keepalive_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[Service]keepalive timer fire; instant={:?}", instant);
                self.fire_instruction(Instruction::KeepAlive);
                counter300 = counter300 + 1;

                if counter300 > 0 && (counter300 % 60) == 0 {
                    self.fire_instruction(Instruction::ServerCfgMonitor);
                }

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[Service]start_keepalive_timer interval errored; err={:?}",
                    e
                )
            })
            .then(|_| {
                info!("[Service] keepalive timer future completed");
                Ok(())
            });;

        tokio::spawn(task);
    }

    fn do_keepalive(&self) {
        debug!("[Service]do_keepalive");
        let ss = self.ss.lock();
        let forwarder = &ss.forwarder;
        let tunmgr = &ss.tunmgr;

        match forwarder.as_ref() {
            Some(f) => {
                f.clone().keepalive();
            }
            None => {}
        }

        match tunmgr.as_ref() {
            Some(t) => {
                t.clone().keepalive();
            }
            None => {}
        }
    }
}
