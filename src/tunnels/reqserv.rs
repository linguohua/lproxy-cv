use super::Request;
use super::{Cmd, THeader, TunMgr, TunStub, THEADER_SIZE};
use crate::lws::{TMessage, TcpFramed, WMessage};
use byte::*;
use log::{error, info};
use nix::sys::socket::getsockname;
use nix::sys::socket::SockAddr::Inet;
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::time::Duration;
use stream_cancel::{Tripwire};
use tokio;
use tokio::net::TcpStream;
use futures_03::prelude::*;

pub fn serve_sock(socket: TcpStream, mgr: Rc<RefCell<TunMgr>>) {
    // config tcp stream
    socket.set_linger(None).unwrap();
    let kduration = Duration::new(3, 0);
    socket.set_keepalive(Some(kduration)).unwrap();
    socket.set_nodelay(true).unwrap();

    // get real dst address
    let rawfd = socket.as_raw_fd();
    //let result = getsockopt(rawfd, OriginalDst).unwrap();
    let result = getsockname(rawfd).unwrap();

    let ip;
    let port: u16;
    match result {
        Inet(iaddr) => {
            port = iaddr.port();
            ip = iaddr.ip();
        }
        _ => {
            return;
        }
    }

    info!("[ReqServ]serve_sock, ip:{}, port:{}", ip, port);
    {
        let peer_addr = socket.peer_addr().unwrap();
        // log access
        mgr.borrow_mut().log_access(peer_addr.ip(), ip.to_std())
    }

    // set 2 seconds write-timeout TODO:
    // let mut socket = TimeoutStream::new(socket);
    // let wduration = Duration::new(2, 0);
    // socket.set_write_timeout(Some(wduration));

    let framed = TcpFramed::new(socket);
    let (sink, stream) = framed.split();
    let (trigger, tripwire) = Tripwire::new();
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let req = Request::with(tx, trigger);
    let tunstub = mgr.borrow_mut().on_request_created(req, ip.to_std(), port);

    if tunstub.is_none() {
        // invalid tunnel
        error!("[ReqServ]failed to alloc tunnel for request!");
        return;
    }

    info!("[ReqServ]allocated tun:{:?}", tunstub);
    let tunstub = Rc::new(RefCell::new(tunstub.unwrap()));
    let req_idx = tunstub.borrow().req_idx;
    let req_tag = tunstub.borrow().req_tag;
    let request_quota = tunstub.borrow().request_quota;

    // let mgr2 = mgr.clone();
    let tunstub2 = tunstub.clone();

    let mut write_out: u16 = 0;
    let quota_report_threshold = (request_quota / 2) as u16;
    let rx = rx.map(move |item| {
        write_out += 1;
        if write_out == quota_report_threshold {
            let ts = &tunstub2.borrow();
            send_request_quota(ts, write_out);

            write_out = 0;
        }

        item
    });

    // send future
    let send_fut = rx.map(move |x|{Ok(x)}).forward(sink);

    let send_fut = async move {
        match send_fut.await {
            Err(e) => {
                error!("[ReqServ]send_fut error:{}", e);
            }
            _ => {}
        }
        info!(
            "[ReqServ]send_fut end, req_idx:{}, req_tag:{}",
            req_idx, req_tag
        );
        // shutdown write direction
        if let Err(e) = shutdown(rawfd, Shutdown::Write) {
            error!("[ReqServ]shutdown rawfd error:{}", e);
        }

        ()
    };

    let tunstub1 = tunstub.clone();
    let tunstub2 = tunstub1.clone();
    // when peer send finished(FIN), then for-each(recv) future end
    // and wait rx(send) futue end, when the server indicate that
    // no more data, rx's pair tx will be drop, then mpsc end, rx
    // future will end, thus both futures are ended;
    // when peer total closed(RST), then both futures will end

    let receive_fut = async move {
        let mut stream = stream.take_until(tripwire);
        while let Some(message) = stream.next().await {
            match message {
                Ok(m) => {
                    let ts = &tunstub.borrow();
                    if !on_request_msg(m, ts) {
                        error!("[ReqServ] on_request_msg failed");
                        break;
                    }
                }
                Err(e) => {
                    error!("[ReqServ] on_request_msg failed:{}", e);
                    break;
                }
            }
        }

        // client(of request) send finished(FIN), indicate that
        // no more data to send
        let ts = &tunstub1.borrow();
        on_request_recv_finished(ts);

        ()
    };

    let join_fut = async move {
        future::join(send_fut, receive_fut).await;
        let ts = &tunstub2.borrow();
        info!("[ReqServ] tcp both futures completed:{:?}", ts);
        mgr.borrow_mut().on_request_closed(ts);

        ()
    };

    // Wait for both futures to complete.
    
    tokio::task::spawn_local(join_fut);
}

fn on_request_msg(mut message: TMessage, tun: &TunStub) -> bool {
    // info!("[ReqMgr]on_request_msg, tun:{}", tun);
    let vec = message.buf.as_mut().unwrap();
    let ll = vec.len();
    let bs = &mut vec[..];

    let offset = &mut 0;
    bs.write_with::<u16>(offset, ll as u16, LE).unwrap();
    bs.write_with::<u8>(offset, Cmd::ReqData as u8, LE).unwrap();

    let th = THeader::new(tun.req_idx, tun.req_tag);
    let msg_header = &mut vec[3..];
    th.write_to(msg_header);

    let wmsg = WMessage::new(message.buf.take().unwrap(), 0);

    let tx = &tun.tunnel_tx;
    let result = tx.send(wmsg);
    match result {
        Err(e) => {
            error!("[ReqServ]request tun send error:{}, tun_tx maybe closed", e);
            return false;
        }
        _ => {
            //     info!(
            //     "[ReqMgr]unbounded_send request msg, req_idx:{}",
            //     tun.req_idx
            // )
        }
    }

    true
}

fn on_request_recv_finished(tun: &TunStub) {
    info!("[ReqServ]on_request_recv_finished:{}", tun);

    let hsize = 3 + THEADER_SIZE;
    let mut buf = vec![0; hsize];

    let bs = &mut buf[..];
    let offset = &mut 0;
    bs.write_with::<u16>(offset, hsize as u16, LE).unwrap();
    bs.write_with::<u8>(offset, Cmd::ReqClientFinished as u8, LE)
        .unwrap();

    let th = THeader::new(tun.req_idx, tun.req_tag);
    let msg_header = &mut buf[3..];
    th.write_to(msg_header);

    let wmsg = WMessage::new(buf, 0);
    let tx = &tun.tunnel_tx;
    let result = tx.send(wmsg);

    match result {
        Err(e) => {
            error!(
                "[ReqServ]on_request_recv_finished, tun send error:{}, tun_tx maybe closed",
                e
            );
        }
        _ => {}
    }
}

fn send_request_quota(tun: &TunStub, qutoa: u16) {
    // send quota notify to server
    let hsize = 3 + THEADER_SIZE + 2;
    let mut buf = vec![0; hsize];

    let bs = &mut buf[..];
    let offset = &mut 0;
    bs.write_with::<u16>(offset, hsize as u16, LE).unwrap();
    bs.write_with::<u8>(offset, Cmd::ReqClientQuota as u8, LE)
        .unwrap();

    let th = THeader::new(tun.req_idx, tun.req_tag);
    let msg_header = &mut bs[3..];
    th.write_to(msg_header);

    *offset = 3 + THEADER_SIZE;
    bs.write_with::<u16>(offset, qutoa as u16, LE).unwrap();

    let wmsg = WMessage::new(buf, 0);

    let tx = &tun.tunnel_tx;
    // send to peer, should always succeed
    if let Err(e) = tx.send(wmsg) {
        error!(
            "[ReqServ]send_request_closed_to_server tx send failed:{}",
            e
        );
    }
}
