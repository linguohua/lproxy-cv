use super::North;
use super::Server;
use crate::tunnels::TunMgr;
use bytes::Bytes;
use bytes::BytesMut;
use futures::sync::mpsc::UnboundedSender;
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

    pub fn on_request_msg(self: Arc<ReqMgr>, _message: BytesMut, _noth: &Arc<North>) {}

    pub fn on_request_closed(self: Arc<ReqMgr>, noth: &Arc<North>) {
        let tm = &self.tm;
        tm.on_request_closed(noth)
    }

    pub fn on_request_created(self: Arc<ReqMgr>, req_tx: UnboundedSender<Bytes>) -> Arc<North> {
        let tm = &self.tm;
        tm.on_request_created(req_tx)
    }
}
