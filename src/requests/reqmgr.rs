use super::Server;
use crate::config::TunCfg;
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
    tmstub: Vec<TunMgrStub>,
    // tm: Rc<RefCell<TunMgr>>,
    tmindex: usize,
}

impl ReqMgr {
    pub fn new(cfg: &TunCfg, tmstub: Vec<TunMgrStub>) -> LongLive {
        info!("[ReqMgr]new ReqMgr");

        Rc::new(RefCell::new(ReqMgr {
            server: Server::new(&cfg.local_server),
            tmstub: tmstub,
            tmindex: 0,
        }))
    }

    pub fn init(&self, s: LongLive) -> Result<(), Error> {
        info!("[ReqMgr]init ReqMgr");
        self.server.borrow_mut().start(s)
    }

    pub fn stop(&mut self) {
        self.tmstub.clear();

        let mut s = self.server.borrow_mut();
        s.stop();
    }

    pub fn on_accept_tcpstream(&mut self, tcpstream: tokio_tcp::TcpStream) {
        let index = self.tmindex;
        if index >= self.tmstub.len() {
            error!("[ReqMgr]no tm to handle tcpstream");
            return;
        }

        let tx = &self.tmstub[index];
        let cmd = SubServiceCtlCmd::TcpTunnel(tcpstream);
        if let Err(e) = tx.ctl_tx.unbounded_send(cmd) {
            error!("[ReqMgr]send req to tm failed:{}", e);
        }

        // move to next tm
        self.tmindex = (index + 1) % self.tmstub.len();
    }
}
