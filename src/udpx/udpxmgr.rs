use tokio::sync::mpsc::UnboundedSender;
use std::net::SocketAddr;
use std::cell::RefCell;
use std::rc::Rc;
use log::{error, info};
use std::hash::{Hash, Hasher};
use crate::service::{TunMgrStub,SubServiceCtlCmd};
use bytes::BytesMut;
use crate::config::TunCfg;
use failure::Error;

pub type LongLiveX = Rc<RefCell<UdpXMgr>>;
pub struct UdpXMgr {
    tmstubs: Vec<TunMgrStub>,
    ctl_tx: UnboundedSender<SubServiceCtlCmd>,
    server: Rc<RefCell<super::UdpServer>>,
}

impl UdpXMgr {
    pub fn new(_cfg: &TunCfg, tmstubs: Vec<TunMgrStub>, ctl_tx: UnboundedSender<SubServiceCtlCmd>) -> LongLiveX {
        info!("[UdpXMgr]new UdpXMgr");
        Rc::new(RefCell::new(UdpXMgr {
            tmstubs,
            ctl_tx,
            server: super::UdpServer::new("127.0.0.1:5555"),
        }))
    }

    pub fn init(&self, s: LongLiveX) -> Result<(), Error> {
        info!("[UdpXMgr]init UdpXMgr");
        // set tx to each tm
        for tm in self.tmstubs.iter() {
            match tm.ctl_tx.send(SubServiceCtlCmd::SetUdpTx(self.ctl_tx.clone())) {
                Err(e) => {
                    error!("[UdpXMgr]init UdpXMgr error, send SetUpdTx failed:{}", e);
                }
                _ => {}
            }
        }

        self.server.borrow_mut().start(s.clone())?;

        Ok(())
    }

    pub fn stop(&mut self) {
        self.tmstubs.clear();

        let mut s = self.server.borrow_mut();
        s.stop();
    }

    pub fn on_udp_server_closed(&mut self) {

    }

    pub fn calc_hash_code(src_addr: SocketAddr, dst_addr: SocketAddr) -> usize {
        let mut hasher = fnv::FnvHasher::default();
        (src_addr,dst_addr).hash(&mut hasher);
        hasher.finish() as usize
    }

    pub fn on_udp_msg_forward(&self, msg: BytesMut, src_addr:SocketAddr, dst_addr: SocketAddr) {
        if self.tmstubs.len() < 1 {
            error!("[UdpXMgr]no tm to handle udp forward");
            return;
        }
        
        let hash_code = UdpXMgr::calc_hash_code(src_addr, dst_addr);
        let tm_index = hash_code % self.tmstubs.len();
        let tx = self.tmstubs[tm_index].ctl_tx.clone();
        let cmd = SubServiceCtlCmd::UdpProxy((msg, src_addr, dst_addr, hash_code));
        match tx.send(cmd) {
            Err(e) => {
                error!("[UdpXMgr] send UdpProxy to tm failed:{}", e);
            }
            _ => {}
        }
    }

    pub fn on_udp_proxy_south(&mut self, msg: std::io::Cursor<Vec<u8>>, src_addr:SocketAddr, dst_addr: SocketAddr) {

    }
}
