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
use super::{LongLiveC,UStub, Cache, UdpServer};
use crate::config::{LOCAL_SERVER, LOCAL_TPROXY_SERVER_PORT};

pub type LongLiveX = Rc<RefCell<UdpXMgr>>;
pub struct UdpXMgr {
    tmstubs: Vec<TunMgrStub>,
    ctl_tx: UnboundedSender<SubServiceCtlCmd>,
    server: Rc<RefCell<super::UdpServer>>,
    server6: Rc<RefCell<super::UdpServer>>,
    cache: LongLiveC,
}

impl UdpXMgr {
    pub fn new(_cfg: &TunCfg, tmstubs: Vec<TunMgrStub>, ctl_tx: UnboundedSender<SubServiceCtlCmd>) -> LongLiveX {
        info!("[UdpXMgr]new UdpXMgr");
        Rc::new(RefCell::new(UdpXMgr {
            tmstubs,
            ctl_tx,
            server: UdpServer::new(format!("{}:{}", LOCAL_SERVER, LOCAL_TPROXY_SERVER_PORT)),
            server6:UdpServer::new(format!("[::1]:{}", LOCAL_TPROXY_SERVER_PORT)),
            cache:  Cache::new(),
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
        self.server6.borrow_mut().start(s.clone())?;

        Ok(())
    }

    pub fn stop(&mut self) {
        self.tmstubs.clear();

        let mut s = self.server.borrow_mut();
        s.stop();
        let mut s = self.server6.borrow_mut();
        s.stop();
        let mut s = self.cache.borrow_mut();
        s.cleanup();
    }

    pub fn on_udp_server_closed(&mut self) {
        // TODO: rebuild udp server
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

    pub fn on_udp_proxy_south(&mut self, lx:LongLiveX, msg: bytes::Bytes, src_addr:SocketAddr, dst_addr: SocketAddr) {
        let cache: &mut Cache = &mut self.cache.borrow_mut();
        let mut stub = cache.get(&src_addr);
        if stub.is_none() {
            // build new stub
            self.build_ustub(lx, cache, &src_addr);
            stub = cache.get(&src_addr);
        }

        match stub {
            Some(stub) => {
                stub.on_udp_proxy_south(msg, dst_addr);
            }
            None => {
                error!("[UdpXMgr] on_udp_proxy_south failed, no stub found");
            }
        }
    }

    fn build_ustub(&self, lx:LongLiveX, c: &mut Cache, src_addr: &SocketAddr) {
        match UStub::new(src_addr, lx) {
            Ok(ustub) => {
                c.insert(self.cache.clone(), *src_addr, ustub);
            }
            Err(e) => {
                error!("[UdpXMgr] build_ustub failed:{}", e);
            }
        }
    }

    pub fn on_ustub_closed(&mut self, src_addr: &SocketAddr) {
        self.cache.borrow_mut().remove(src_addr);
    }
}
