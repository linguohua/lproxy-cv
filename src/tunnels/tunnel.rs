use super::Cmd;
use super::THeader;
use crate::requests::TunStub;
use crate::requests::Reqq;
use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use std::sync::Arc;
use std::sync::Mutex;
use tungstenite::protocol::Message;

pub struct Tunnel {
    pub tx: UnboundedSender<Message>,
    pub index: usize,

    pub requests: Mutex<Reqq>,
}

impl Tunnel {
    pub fn on_tunnel_msg(&self, msg: Message) -> bool {
        if msg.is_pong() {
            self.on_pong(msg);

            return true;
        }

        if !msg.is_binary() {
            println!("tunnel should only handle binary msg!");
            return true;
        }

        let bs = msg.into_data();
        let th = THeader::read_from(&bs[..]);
        let cmd = Cmd::from(th.cmd);
        match cmd {
            Cmd::ReqData => {
                // data
                let req_idx = th.req_idx;
                let req_tag = th.req_tag;
                let tx = self.get_request_tx(req_idx, req_tag);
                match tx {
                    None => {
                        println!("no request found for: {}:{}", req_idx, req_tag);
                        return false;
                    }
                    Some(tx) => {
                        let b = Bytes::from(&bs[4..]);
                        let result = tx.unbounded_send(b);
                        match result {
                            Err(e) => {
                                println!("tunnel msg send to request failed:{}", e);
                                return false;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {
                println!("unsupport cmd:{:?}, discard msg", cmd);
            }
        }

        return true;
    }

    fn on_pong(&self, _msg: Message) {}

    fn get_request_tx(&self, req_idx: u16, req_tag: u16) -> Option<UnboundedSender<Bytes>> {
        let requests = &self.requests.lock().unwrap();
        let req_idx = req_idx as usize;
        if req_idx >= requests.elements.len() {
            return None;
        }

        let req = &requests.elements[req_idx];
        if req.tag == req_tag && req.request_tx.is_some() {
            match req.request_tx {
                None => {
                    return None;
                }
                Some(ref tx) => {
                    return Some(tx.clone());
                }
            }
        }

        None
    }

    pub fn on_request_created(&self, req_tx: &UnboundedSender<Bytes>) -> Arc<TunStub> {
        let reqs = &mut self.requests.lock().unwrap();
        let (idx, tag) = reqs.alloc(req_tx);
        let tun_idx = self.index;
        let tx = self.tx.clone();

        Arc::new(TunStub {
            tunnel_tx: tx,
            tun_idx: tun_idx as u16,
            req_idx: idx,
            req_tag: tag,
        })
    }

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        let reqs = &mut self.requests.lock().unwrap();

        reqs.free(tunstub.req_idx, tunstub.req_tag);
    }

    pub fn on_closed(&self) {
        // free all requests
        let reqs = &mut self.requests.lock().unwrap();
        reqs.clear_all();
    }
}
