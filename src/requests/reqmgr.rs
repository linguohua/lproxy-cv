use super::Server;
use crate::config::TunCfg;
use crate::config::{LOCAL_SERVER, LOCAL_TPROXY_SERVER_PORT};
use crate::service::SubServiceCtlCmd;
use crate::service::TunMgrStub;
use failure::Error;
use log::{error, info};
use std::cell::RefCell;
use std::rc::Rc;
use std::result::Result;

type LongLive = Rc<RefCell<ReqMgr>>;

pub struct ReqMgr {
    server: Rc<RefCell<Server>>,
    server6: Rc<RefCell<Server>>,
    tmstubs: Vec<TunMgrStub>,
    // tm: Rc<RefCell<TunMgr>>,
    tmindex: usize,
    accepted: usize,
}

impl ReqMgr {
    pub fn new(_cfg: &TunCfg, tmstubs: Vec<TunMgrStub>) -> LongLive {
        info!("[ReqMgr]new ReqMgr");
        Rc::new(RefCell::new(ReqMgr {
            server: Server::new(LOCAL_SERVER, LOCAL_TPROXY_SERVER_PORT),
            server6: Server::new("::1", LOCAL_TPROXY_SERVER_PORT),
            tmstubs: tmstubs,
            tmindex: 0,
            accepted: 0,
        }))
    }

    pub fn init(&self, s: LongLive) -> Result<(), Error> {
        info!("[ReqMgr]init ReqMgr");
        self.server.borrow_mut().start(s.clone())?;
        self.server6.borrow_mut().start(s)
    }

    pub fn stop(&mut self) {
        self.tmstubs.clear();

        {
            let mut s = self.server.borrow_mut();
            s.stop();
        }

        {
            let mut s2 = self.server6.borrow_mut();
            s2.stop();
        }
    }

    pub fn on_accept_tcpstream(&mut self, tcpstream: tokio::net::TcpStream) {
        let index = self.tmindex;

        if index >= self.tmstubs.len() {
            error!("[ReqMgr]no tm to handle tcpstream");
            return;
        }

        let tx = &self.tmstubs[index];
        let cmd = SubServiceCtlCmd::TcpTunnel(tcpstream);
        if let Err(e) = tx.ctl_tx.send(cmd) {
            error!("[ReqMgr]send req to tm failed:{}", e);
        }

        // move to next tm
        self.tmindex = (index + 1) % self.tmstubs.len();
        self.accepted += 1;
        info!("[ReqMgr]send req to tm {}", self.accepted);
    }
}
