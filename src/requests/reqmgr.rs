use super::TunStub;
use super::Server;
use crate::tunnels::THeader;
use crate::tunnels::TunMgr;
use crate::tunnels::THEADER_SIZE;
use bytes::Bytes;
use bytes::BytesMut;
use futures::sync::mpsc::UnboundedSender;
use std::sync::Arc;
use tungstenite::protocol::Message;

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

    pub fn on_request_msg(&self, message: BytesMut, tun: &Arc<TunStub>) -> bool {
        let size = message.len();
        let hsize = THEADER_SIZE;
        let buf = &mut vec![0; hsize + size];

        let th = THeader::new_data_header(tun.req_idx, tun.req_tag);
        let msg_header = &mut buf[0..hsize];
        th.write_to(msg_header);
        let msg_body = &mut buf[hsize..];
        msg_body.copy_from_slice(message.as_ref());

        let wmsg = Message::from(&buf[..]);
        let tx = &tun.tunnel_tx;
        let result = tx.unbounded_send(wmsg);
        match result {
            Err(e) => {
                println!("request tun send error:{}, tun_tx maybe closed", e);
                return false;
            }
            _ => {}
        }

        true
    }

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        let tm = &self.tm;
        tm.on_request_closed(tunstub)
    }

    pub fn on_request_created(&self, req_tx: &UnboundedSender<Bytes>) -> Arc<TunStub> {
        let tm = &self.tm;
        tm.on_request_created(req_tx)
    }
}
