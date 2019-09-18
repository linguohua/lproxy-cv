use crate::config;
use crate::config::KEEP_ALIVE_INTERVAL;
use crate::dns;
use crate::dns::Forwarder;
use crate::htp;
use crate::requests;
use crate::requests::ReqMgr;
use crate::tunnels;
use crate::tunnels::TunMgr;
use futures::sync::mpsc::UnboundedSender;
use log::{debug, error, info};
use std::fmt;
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::Delay;
use tokio::timer::Interval;
const STATE_STOPPED: u8 = 0;
const STATE_STARTING: u8 = 1;
const STATE_RUNNING: u8 = 2;
const STATE_STOPPING: u8 = 3;
use std::cell::RefCell;
use std::rc::Rc;

type LongLive = Rc<RefCell<Service>>;

enum Instruction {
    Auth,
    StartSubServices,
    KeepAlive,
    ServerCfgMonitor,
    Restart,
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s;
        match self {
            Instruction::Auth => s = "Auth",
            Instruction::StartSubServices => s = "StartSubServices",
            Instruction::KeepAlive => s = "KeepAlive",
            Instruction::ServerCfgMonitor => s = "ServerCfgMonitor",
            Instruction::Restart => s = "Restart",
        }
        write!(f, "({})", s)
    }
}

type TxType = UnboundedSender<Instruction>;

pub struct Service {
    state: u8,
    forwarder: Option<Rc<RefCell<Forwarder>>>,
    tunmgr: Option<Rc<RefCell<TunMgr>>>,
    reqmgr: Option<Rc<RefCell<ReqMgr>>>,
    ins_tx: Option<TxType>,
    tuncfg: Option<config::TunCfg>,
    keepalive_trigger: Option<Trigger>,
    instruction_trigger: Option<Trigger>,
}

impl Service {
    pub fn new() -> LongLive {
        Rc::new(RefCell::new(Service {
            forwarder: None,
            tunmgr: None,
            reqmgr: None,
            ins_tx: None,
            tuncfg: None,
            keepalive_trigger: None,
            instruction_trigger: None,
            state: 0,
        }))
    }

    // start config monitor
    pub fn start(s: LongLive) {
        let mut rf = s.borrow_mut();
        if rf.state == STATE_STOPPED {
            rf.state = STATE_STARTING;

            let (tx, rx) = futures::sync::mpsc::unbounded();
            let (trigger, tripwire) = Tripwire::new();
            rf.save_instruction_trigger(trigger);

            let clone = s.clone();
            let fut = rx
                .take_until(tripwire)
                .for_each(move |ins| {
                    Service::process_instruction(clone.clone(), ins);
                    Ok(())
                })
                .then(|_| {
                    info!("[Service] instruction rx future completed");

                    Ok(())
                });

            rf.save_tx(Some(tx));
            rf.fire_instruction(Instruction::Auth);

            current_thread::spawn(fut);
        } else {
            panic!("[Service] start failed, state not stopped");
        }
    }

    pub fn stop(&mut self) {
        info!("[Service]stop");
        if self.state != STATE_RUNNING {
            error!("[Service] do_restart failed, state not running");
            return;
        }

        self.state = STATE_STOPPING;

        {
            // drop trigger will complete keepalive future
            self.keepalive_trigger = None;

            // drop trigger will completed instruction future
            self.instruction_trigger = None;

            // stop all subservices
            match &self.forwarder {
                Some(s) => {
                    let s = s.clone();
                    let mut s = s.borrow_mut();
                    s.stop();
                    self.forwarder = None;
                }
                None => {}
            }

            match &self.reqmgr {
                Some(s) => {
                    let s = s.clone();
                    s.borrow_mut().stop();
                    self.reqmgr = None;
                }
                None => {}
            }

            match &self.tunmgr {
                Some(s) => {
                    let s = s.clone();
                    s.borrow_mut().stop();
                    self.tunmgr = None;
                }
                None => {}
            }
        }

        self.restore_sys();

        self.state = STATE_STOPPED;
    }

    fn process_instruction(s: LongLive, ins: Instruction) {
        match ins {
            Instruction::Auth => {
                Service::do_auth(s.clone());
            }
            Instruction::StartSubServices => {
                Service::do_start_subservices(s.clone());
            }
            Instruction::KeepAlive => {
                let rf = s.borrow();
                rf.do_keepalive();
            }
            Instruction::ServerCfgMonitor => {
                Service::do_cfg_monitor(s.clone());
            }
            Instruction::Restart => {
                Service::do_restart(s.clone());
            }
        }
    }

    fn do_auth(s: LongLive) {
        info!("[Service]do_auth");
        let httpserver = config::server_url();
        let req = htp::HTTPRequest::new(&httpserver).unwrap();
        let sclone = s.clone();
        let ar = config::AuthReq {
            uuid: "abc-efg-hij-klm".to_string(),
        };

        let arstr = ar.to_json_str();
        let fut = req
            .exec(Some(arstr))
            .and_then(move |response| {
                info!("[Service]do_auth http response:{:?}", response);

                if response.status != 200 {
                    // TODO: not support redirect yet!
                    let seconds = 10;
                    error!(
                        "[Service]do_auth http request failed, status: {} not 200, retry {} seconds later",
                        response.status, seconds
                    );
                    Service::delay_post_instruction(sclone.clone(), 30, Instruction::Auth);

                    return Ok(());
                }

                if let Some(ref body) = response.body {
                    let aresp: config::AuthResp = config::AuthResp::from_json_str(body);
                    info!("[Service]AuthResp:{}", aresp);
                }

                // TODO: use reponse to init TunCfg
                let cfg = config::TunCfg::new();
                let mut rf = sclone.borrow_mut();
                rf.save_cfg(cfg);
                rf.fire_instruction(Instruction::StartSubServices);
                Ok(())
            })
            .or_else(move |e| {
                let seconds = 5;
                error!(
                    "[Service]do_auth http request failed, error:{}, retry {} seconds later",
                    e, seconds
                );

                Service::delay_post_instruction(s.clone(), seconds, Instruction::Auth);

                Ok(())
            });

        current_thread::spawn(fut);
    }

    fn do_cfg_monitor(s: LongLive) {
        info!("[Service]do_cfg_monitor");

        let httpserver = config::server_url();
        let req = htp::HTTPRequest::new(&httpserver).unwrap();
        let sclone = s.clone();
        let fut = req
            .exec(None)
            .and_then(move |response| {
                debug!("[Service]do_cfg_monitor http response:{:?}", response);

                // TODO: use reponse to init TunCfg
                let cfg = config::TunCfg::new();
                let mut rf = sclone.borrow_mut();
                rf.save_cfg(cfg);
                rf.fire_instruction(Instruction::Restart);

                Ok(())
            })
            .or_else(move |e| {
                let seconds = 5;
                error!(
                    "[Service]do_cfg_monitor http request failed, error:{}, retry {} seconds later",
                    e, seconds
                );

                Service::delay_post_instruction(s.clone(), seconds, Instruction::Auth);

                Ok(())
            });

        current_thread::spawn(fut);
    }

    fn delay_post_instruction(s: LongLive, seconds: u64, ins: Instruction) {
        info!(
            "[Service]delay_post_instruction, seconds:{}, ins:{}",
            seconds, ins
        );

        // delay 5 seconds
        let when = Instant::now() + Duration::from_millis(seconds * 1000);
        let task = Delay::new(when)
            .and_then(move |_| {
                debug!("[Service]delay_post_instruction retry...");
                s.borrow().fire_instruction(ins);

                Ok(())
            })
            .map_err(|e| {
                error!(
                    "[Service]delay_post_instruction delay retry errored, err={:?}",
                    e
                );
                ()
            });

        current_thread::spawn(task);
    }

    fn do_start_subservices(s: LongLive) {
        info!("[Service]do_start_subservices");
        let good;
        {
            let mut rf = s.borrow_mut();
            good = rf.start_subservices();
        }

        if good {
            Service::start_keepalive_timer(s.clone());
        } else {
            // re-auth
            Service::delay_post_instruction(s.clone(), 5, Instruction::Auth);
        }
    }

    fn do_restart(s: LongLive) {
        info!("[Service]do_restart");
        s.borrow_mut().stop();
        Service::start(s.clone());
    }

    fn save_tx(&mut self, tx: Option<TxType>) {
        self.ins_tx = tx;
    }

    fn save_cfg(&mut self, cfg: config::TunCfg) {
        self.tuncfg = Some(cfg);
    }

    fn save_keepalive_trigger(&mut self, trigger: Trigger) {
        self.keepalive_trigger = Some(trigger);
    }

    fn save_instruction_trigger(&mut self, trigger: Trigger) {
        self.instruction_trigger = Some(trigger);
    }

    fn fire_instruction(&self, ins: Instruction) {
        debug!("[Service]fire_instruction, ins:{}", ins);
        let tx = &self.ins_tx;
        match tx.as_ref() {
            Some(ref tx) => {
                if let Err(e) = tx.unbounded_send(ins) {
                    error!("[Service]fire_instruction failed:{}", e);
                }
            }
            None => {
                error!("[Service]fire_instruction failed: no tx");
            }
        }
    }

    fn start_subservices(&mut self) -> bool {
        info!("[Service]start_subservices");
        let cfg = self.tuncfg.as_ref().unwrap();
        let tunmgr = tunnels::TunMgr::new(cfg);
        let reqmgr = requests::ReqMgr::new(tunmgr.clone(), cfg);
        let forwarder = dns::Forwarder::new(cfg);

        if let Err(e) = forwarder.borrow_mut().init(forwarder.clone()) {
            error!("[Service]forwarder start failed:{}", e);
            return false;
        }

        if let Err(e) = reqmgr.borrow_mut().init(reqmgr.clone()) {
            error!("[Service]reqmgr start failed:{}", e);
            forwarder.borrow_mut().stop();
            return false;
        }

        if let Err(e) = tunmgr.borrow_mut().init(tunmgr.clone()) {
            error!("[Service]tunmgr start failed:{}", e);
            forwarder.borrow_mut().stop();
            reqmgr.borrow_mut().stop();

            return false;
        }

        // save sub-services
        self.forwarder = Some(forwarder);
        self.tunmgr = Some(tunmgr);
        self.reqmgr = Some(reqmgr);

        // change state to running
        if self.state != STATE_STARTING {
            panic!("[Service] start_subservice failed, state not STARTING");
        }

        self.state = STATE_RUNNING;

        self.config_sys();

        true
    }

    pub fn config_sys(&self) {
        info!("[Service]config_sys");
    }

    pub fn restore_sys(&self) {
        info!("[Service]restore_sys");
    }

    fn start_keepalive_timer(s: LongLive) {
        info!("[Service]start_keepalive_timer");
        let clone = s.clone();
        let mut rf2 = s.borrow_mut();
        let mut counter300 = 0;

        let (trigger, tripwire) = Tripwire::new();
        rf2.save_keepalive_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(KEEP_ALIVE_INTERVAL))
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[Service]keepalive timer fire; instant={:?}", instant);

                let rf = clone.borrow_mut();
                rf.fire_instruction(Instruction::KeepAlive);
                counter300 = counter300 + 1;

                if counter300 > 0 && (counter300 % 60) == 0 {
                    rf.fire_instruction(Instruction::ServerCfgMonitor);
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

        current_thread::spawn(task);
    }

    fn do_keepalive(&self) {
        debug!("[Service]do_keepalive");
        let forwarder = &self.forwarder;
        let tunmgr = &self.tunmgr;

        match forwarder.as_ref() {
            Some(f) => {
                f.borrow_mut().keepalive(f.clone());
            }
            None => {}
        }

        match tunmgr.as_ref() {
            Some(t) => {
                t.borrow_mut().keepalive(t.clone());
            }
            None => {}
        }
    }
}
