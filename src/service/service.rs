use crate::config::{self, CFG_MONITOR_INTERVAL, DEFAULT_DNS_SERVER};
use crate::htp;
use futures::sync::mpsc::UnboundedSender;
use log::{debug, error, info};
use std::fmt;
use std::time::{Duration, Instant};
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::{Delay, Interval};
const STATE_STOPPED: u8 = 0;
const STATE_STARTING: u8 = 1;
const STATE_RUNNING: u8 = 2;
const STATE_STOPPING: u8 = 3;
use super::SubServiceCtl;
use std::cell::RefCell;
use std::rc::Rc;

type LongLive = Rc<RefCell<Service>>;

enum Instruction {
    Auth,
    StartSubServices,
    ServerCfgMonitor,
    Restart,
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s;
        match self {
            Instruction::Auth => s = "Auth",
            Instruction::StartSubServices => s = "StartSubServices",
            Instruction::ServerCfgMonitor => s = "ServerCfgMonitor",
            Instruction::Restart => s = "Restart",
        }
        write!(f, "({})", s)
    }
}

type TxType = UnboundedSender<Instruction>;

pub struct Service {
    state: u8,
    subservices: Vec<SubServiceCtl>,
    ins_tx: Option<TxType>,
    tuncfg: Option<std::sync::Arc<config::TunCfg>>,
    monitor_trigger: Option<Trigger>,
    instruction_trigger: Option<Trigger>,
    domains: Option<Vec<String>>,
}

impl Service {
    pub fn new() -> LongLive {
        Rc::new(RefCell::new(Service {
            subservices: Vec::new(),
            ins_tx: None,
            tuncfg: None,
            monitor_trigger: None,
            instruction_trigger: None,
            state: 0,
            domains: None,
        }))
    }

    // start config monitor
    pub fn start(&mut self, s: LongLive) {
        if self.state == STATE_STOPPED {
            self.state = STATE_STARTING;

            let (tx, rx) = futures::sync::mpsc::unbounded();
            let (trigger, tripwire) = Tripwire::new();
            self.save_instruction_trigger(trigger);

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

            self.save_tx(Some(tx));
            self.fire_instruction(Instruction::Auth);

            current_thread::spawn(fut);

            // replace uci dnsmasq forward server to default
            super::set_uci_dnsmasq_to_default();
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

        // drop trigger will complete monitor future
        self.monitor_trigger = None;

        // drop trigger will completed instruction future
        self.instruction_trigger = None;

        super::cleanup_subservices(&mut self.subservices);

        self.subservices.clear();

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
            Instruction::ServerCfgMonitor => {
                Service::do_cfg_monitor(s.clone());
            }
            Instruction::Restart => {
                Service::do_restart(s.clone());
            }
        }
    }

    fn parse_auth_reply(response: &htp::HTTPResponse) -> Option<config::AuthResp> {
        if response.status != 200 {
            return None;
        }

        if let Some(ref body) = response.body {
            let aresp: config::AuthResp = config::AuthResp::from_json_str(body);
            // info!("[Service]AuthResp:{:?}", aresp);

            return Some(aresp);
        } else {
            return None;
        }
    }

    fn do_auth(s: LongLive) {
        info!("[Service]do_auth");
        let httpserver = config::server_url();
        let dns_server = Some(DEFAULT_DNS_SERVER.to_string());
        let req =
            htp::HTTPRequest::new(&httpserver, Some(Duration::from_secs(10)), dns_server).unwrap();

        let sclone = s.clone();
        let ar = config::AuthReq {
            uuid: "abc-efg-hij-klm".to_string(),
        };

        let arstr = ar.to_json_str();
        let fut = req
            .exec(Some(arstr))
            .and_then(move |response| {
                // info!("[Service]do_auth http response:{:?}", response);

                let mut retry = true;
                if let Some(mut rsp) = Service::parse_auth_reply(&response) {
                    if rsp.tuncfg.is_some() {
                        let mut cfg = rsp.tuncfg.take().unwrap();
                        let mut rf = sclone.borrow_mut();
                        rf.domains = Some(cfg.domain_array.take().unwrap());

                        info!(
                            "[Service]do_auth http response, tunnel count:{}, req cap:{}",
                            cfg.tunnel_number, cfg.tunnel_req_cap
                        );

                        rf.save_cfg(cfg);
                        rf.fire_instruction(Instruction::StartSubServices);
                        retry = false;
                    }
                }

                if retry {
                    let seconds = 30;
                    error!(
                        "[Service]do_auth http request failed, no body, retry {} seconds later",
                        seconds
                    );
                    Service::delay_post_instruction(sclone.clone(), seconds, Instruction::Auth);
                }
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
        let req = htp::HTTPRequest::new(&httpserver, Some(Duration::from_secs(10)), None).unwrap();
        let sclone = s.clone();
        let fut = req
            .exec(None)
            .and_then(move |response| {
                // debug!("[Service]do_cfg_monitor http response:{:?}", response);

                if let Some(mut rsp) = Service::parse_auth_reply(&response) {
                    if rsp.restart && rsp.tuncfg.is_some() {
                        let cfg = rsp.tuncfg.take().unwrap();
                        let mut rf = sclone.borrow_mut();
                        rf.save_cfg(cfg);
                        rf.fire_instruction(Instruction::Restart);
                    }
                }

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
        let cfg;
        {
            cfg = s.borrow().tuncfg.as_ref().unwrap().clone();
        }
        let domains;
        {
            let mut ss = s.borrow_mut();
            if ss.domains.is_some() {
                domains = ss.domains.take().unwrap();
            } else {
                domains = Vec::new();
            }
        }

        let clone = s.clone();
        let clone2 = s.clone();
        let fut = super::start_subservice(cfg, domains)
            .and_then(move |subservices| {
                let s2 = &mut clone.borrow_mut();
                let vec_subservices = &mut subservices.borrow_mut();
                while let Some(ctl) = vec_subservices.pop() {
                    s2.subservices.push(ctl);
                }

                s2.state = STATE_RUNNING;
                s2.config_sys();

                Service::start_monitor_timer(s2, clone.clone());
                Ok(())
            })
            .or_else(|_| {
                Service::delay_post_instruction(clone2, 5, Instruction::Auth);
                Err(())
            });

        current_thread::spawn(fut);

        // enable uci dnsmasq forward server, point to this
        super::set_uci_dnsmasq_to_me();
    }

    fn do_restart(s1: LongLive) {
        info!("[Service]do_restart");
        let mut s = s1.borrow_mut();
        s.stop();
        s.start(s1.clone());
    }

    fn save_tx(&mut self, tx: Option<TxType>) {
        self.ins_tx = tx;
    }

    fn save_cfg(&mut self, cfg: config::TunCfg) {
        self.tuncfg = Some(std::sync::Arc::new(cfg));
    }

    fn save_monitor_trigger(&mut self, trigger: Trigger) {
        self.monitor_trigger = Some(trigger);
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

    pub fn config_sys(&self) {
        info!("[Service]config_sys");
        // let tuncfg = self.tuncfg.as_ref().unwrap();
        super::iptable_rule::set_iptables_rules();
        super::ip_rules::set_ip_rules();
    }

    pub fn restore_sys(&self) {
        info!("[Service]restore_sys");
        super::iptable_rule::unset_iptables_rules();
        super::ip_rules::unset_ip_rules();
    }

    fn start_monitor_timer(&mut self, s2: LongLive) {
        info!("[Service]start_monitor_timer");
        let (trigger, tripwire) = Tripwire::new();
        self.save_monitor_trigger(trigger);

        // tokio timer, every 3 seconds
        let task = Interval::new(Instant::now(), Duration::from_millis(CFG_MONITOR_INTERVAL))
            .skip(1)
            .take_until(tripwire)
            .for_each(move |instant| {
                debug!("[Service]monitor timer fire; instant={:?}", instant);

                let rf = s2.borrow_mut();
                rf.fire_instruction(Instruction::ServerCfgMonitor);

                Ok(())
            })
            .map_err(|e| error!("[Service]start_monitor_timer interval errored; err={:?}", e))
            .then(|_| {
                info!("[Service] monitor timer future completed");
                Ok(())
            });;

        current_thread::spawn(task);
    }
}
