use super::Server;
use super::TunStub;
use crate::config::TunCfg;
use crate::tunnels::TunMgr;
use crate::tunnels::THEADER_SIZE;
use crate::tunnels::{Cmd, THeader};
use failure::Error;
use bytes::BytesMut;
use log::{error, info};
use std::result::Result;
use tungstenite::protocol::Message;
use std::cell::RefCell;
use std::rc::Rc;

type LongLive = Rc<RefCell<ReqMgr>>;
pub struct ReqMgr {
    server: Rc<RefCell<Server>>,
    tm: Rc<RefCell<TunMgr>>,
}

impl ReqMgr {
    pub fn new(tm: Rc<RefCell<TunMgr>>, cfg: &TunCfg) -> LongLive {
        info!("[ReqMgr]new ReqMgr, tuncfg:{:?}", cfg);

        Rc::new(RefCell::new(ReqMgr {
            server: Server::new(&cfg.local_server),
            tm: tm,
        }))
    }

    pub fn init(&self, s: LongLive) -> Result<(), Error> {
        info!("[ReqMgr]init ReqMgr");
         self.server.borrow_mut().start(s)
    }

    pub fn on_request_msg(message: BytesMut, tun: &TunStub) -> bool {
        info!("[ReqMgr]on_request_msg, tun:{:?}", tun);
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
                error!("[ReqMgr]request tun send error:{}, tun_tx maybe closed", e);
                return false;
            }
            _ => info!(
                "[ReqMgr]unbounded_send request msg, req_idx:{}",
                tun.req_idx
            ),
        }

        true
    }

    pub fn on_request_recv_finished(tun: &TunStub) {
        info!("[ReqMgr]on_request_recv_finished:{:?}", tun);

        let hsize = THEADER_SIZE;
        let buf = &mut vec![0; hsize];

        let th = THeader::new(Cmd::ReqClientFinished, tun.req_idx, tun.req_tag);
        let msg_header = &mut buf[0..hsize];
        th.write_to(msg_header);

        let wmsg = Message::from(&buf[..]);
        let tx = &tun.tunnel_tx;
        let result = tx.unbounded_send(wmsg);

        match result {
            Err(e) => {
                error!(
                    "[ReqMgr]on_request_recv_finished, tun send error:{}, tun_tx maybe closed",
                    e
                );
            }
            _ => {}
        }
    }

    pub fn on_request_closed(&self, tunstub: &TunStub) {
        info!("[ReqMgr]on_request_closed, tun:{:?}", tunstub);
        let tm = &self.tm;
        tm.borrow_mut().on_request_closed(tunstub)
    }

    pub fn on_request_created(&self, req: super::Request) -> Option<TunStub> {
        info!("[ReqMgr]on_request_created, req:{:?}", req);
        let tm = &self.tm;
        tm.borrow_mut().on_request_created(req)
    }

    pub fn stop(&mut self) {
        let mut s = self.server.borrow_mut();
        s.stop();
    }
}
