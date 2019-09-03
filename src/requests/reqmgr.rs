use super::Reqq;
use super::Request;
use super::Server;
use bytes::BytesMut;
use std::sync::Arc;
use std::sync::Mutex;

pub struct ReqMgr {
    server: Arc<Server>,
    requests: Mutex<Reqq>,
}

impl ReqMgr {
    pub fn new() -> Arc<ReqMgr> {
        Arc::new(ReqMgr {
            server: Server::new(5555),
            requests: Mutex::new(Reqq::new(1)),
        })
    }

    pub fn init(self: Arc<ReqMgr>) {
        let s = self.server.clone();

        s.start(&self);
    }

    pub fn on_request_msg(self: Arc<ReqMgr>, _message: BytesMut, _idx: usize) {}

    pub fn on_request_closed(self: Arc<ReqMgr>, idx: usize) {
        let requests = &mut self.requests.lock().unwrap();
        requests.free(idx);
    }

    pub fn on_request_created(self: Arc<ReqMgr>, req: Request) -> usize {
        let requests = &mut self.requests.lock().unwrap();

        requests.alloc(req)
    }
}
