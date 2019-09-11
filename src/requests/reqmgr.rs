use super::Server;
use super::TunStub;
use crate::config::TunCfg;
use crate::tunnels::TunMgr;
use crate::tunnels::THEADER_SIZE;
use crate::tunnels::{Cmd, THeader};
use bytes::Bytes;
use bytes::BytesMut;
use futures::sync::mpsc::UnboundedSender;
use log::{error, info};
use std::sync::Arc;
use tungstenite::protocol::Message;

pub struct ReqMgr {
    server: Arc<Server>,
    tm: Arc<TunMgr>,
}

impl ReqMgr {
    pub fn new(tm: &Arc<TunMgr>, cfg: &TunCfg) -> Arc<ReqMgr> {
        info!("[ReqMgr]new ReqMgr, tuncfg:{:?}", cfg);

        Arc::new(ReqMgr {
            server: Server::new(&cfg.local_server),
            tm: tm.clone(),
        })
    }

    pub fn init(self: Arc<ReqMgr>) {
        info!("[ReqMgr]init ReqMgr");
        let s = self.server.clone();

        s.start(&self);
    }

    pub fn on_request_msg(message: BytesMut, tun: &Arc<TunStub>) -> bool {
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

    pub fn on_request_recv_finished(tun: &Arc<TunStub>) {
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

    pub fn on_request_closed(&self, tunstub: &Arc<TunStub>) {
        info!("[ReqMgr]on_request_closed, tun:{:?}", tunstub);
        let tm = &self.tm;
        tm.on_request_closed(tunstub)
    }

    pub fn on_request_created(
        &self,
        req_tx: &UnboundedSender<Bytes>,
        dst: &libc::sockaddr_in,
    ) -> Option<TunStub> {
        info!("[ReqMgr]on_request_created, dst:{:?}", dst);
        let tm = &self.tm;
        tm.on_request_created(req_tx, dst)
    }
}
