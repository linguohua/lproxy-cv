use super::North;
use super::Request;
use super::Server;
use crate::tunnels::TunMgr;
use bytes::BytesMut;
use std::sync::Arc;

pub struct ReqMgr {
    server: Arc<Server>,
    tm: Arc<TunMgr>,
}

impl ReqMgr {
    pub fn new(tm: &Arc<TunMgr>) -> Arc<ReqMgr> {
        Arc::new(ReqMgr {
            server: Server::new(5555),
            tm: tm.clone(),
        })
    }

    pub fn init(self: Arc<ReqMgr>) {
        let s = self.server.clone();

        s.start(&self);
    }

    pub fn on_request_msg(self: Arc<ReqMgr>, _message: BytesMut, _noth: Arc<North>) {}

    pub fn on_request_closed(self: Arc<ReqMgr>, _noth: Arc<North>) {
        // let requests = &mut self.requests.lock().unwrap();
        // requests.free(idx);
    }

    pub fn on_request_created(self: Arc<ReqMgr>, req: Request) -> Arc<North> {
        let tm = &self.tm;
        tm.on_request_created(req)
    }
}
