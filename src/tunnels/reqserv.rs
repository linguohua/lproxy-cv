use super::Request;
use super::{Cmd, THeader, TunMgr, TunStub, THEADER_SIZE};
use crate::config::DEFAULT_REQ_QUOTA;
use crate::lws::{TMessage, TcpFramed, WMessage};
use byte::*;
use log::{error, info};
use nix::sys::socket::getsockname;
use nix::sys::socket::InetAddr::V4;
use nix::sys::socket::SockAddr::Inet;
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::time::Duration;
use stream_cancel::{StreamExt, Tripwire};
use tokio;
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio_io_timeout::TimeoutStream;
use tokio_tcp::TcpStream;

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

    let mut ip_be = 0; // result.sin_addr.s_addr;
    let mut port_be = 0; // result.sin_port.to_be();
    match result {
        Inet(iaddr) => match iaddr {
            V4(v) => {
                ip_be = v.sin_addr.s_addr.to_be();
                port_be = v.sin_port.to_be();
            }
            _ => {
                // TODO: ipv6
            }
        },
        _ => {}
    }

    let ipaddr = std::net::Ipv4Addr::from(ip_be); // ip_le.to_be()
    info!("[ReqServ]serve_sock, ip:{}, port:{}", ipaddr, port_be);

    // set 2 seconds write-timeout
    let mut socket = TimeoutStream::new(socket);
    let wduration = Duration::new(2, 0);
    socket.set_write_timeout(Some(wduration));

    let framed = TcpFramed::new(socket);
    let (sink, stream) = framed.split();
    let (trigger, tripwire) = Tripwire::new();
    let (tx, rx) = futures::sync::mpsc::unbounded();

    let req = Request::with(tx, trigger, ip_be, port_be);
    let tunstub = mgr.borrow_mut().on_request_created(req);

    if tunstub.is_none() {
        // invalid tunnel
        error!("[ReqServ]failed to alloc tunnel for request!");
        return;
    }

    info!("[ReqServ]allocated tun:{:?}", tunstub);
    let tunstub = Rc::new(RefCell::new(tunstub.unwrap()));
    let req_idx = tunstub.borrow().req_idx;
    let req_tag = tunstub.borrow().req_tag;

    // let mgr2 = mgr.clone();
    let tunstub2 = tunstub.clone();

    let mut write_out: u16 = 0;
    let rx = rx.map(move |item| {
        write_out += 1;
        if write_out == (DEFAULT_REQ_QUOTA / 8) {
            let ts = &tunstub2.borrow();
            send_request_quota(ts, write_out);

            write_out = 0;
        }

        item
    });

    // send future
    let send_fut = sink.send_all(rx.map_err(|e| {
        error!("[ReqServ]sink send_all failed:{:?}", e);
        std::io::Error::from(std::io::ErrorKind::Other)
    }));

    let send_fut = send_fut.and_then(move |_| {
        info!(
            "[ReqServ]send_fut end, req_idx:{}, req_tag:{}",
            req_idx, req_tag
        );
        // shutdown read direction
        if let Err(e) = shutdown(rawfd, Shutdown::Read) {
            error!("[ReqServ]shutdown rawfd error:{}", e);
        }

        Ok(())
    });

    let tunstub1 = tunstub.clone();

    // when peer send finished(FIN), then for-each(recv) future end
    // and wait rx(send) futue end, when the server indicate that
    // no more data, rx's pair tx will be drop, then mpsc end, rx
    // future will end, thus both futures are ended;
    // when peer total closed(RST), then both futures will end
    let receive_fut = stream.take_until(tripwire).for_each(move |message| {
        let ts = &tunstub.borrow();
        // post to manager
        if on_request_msg(message, ts) {
            Ok(())
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
        }
    });

    let tunstub2 = tunstub1.clone();
    let receive_fut = receive_fut.and_then(move |_| {
        // client(of request) send finished(FIN), indicate that
        // no more data to send
        let ts = &tunstub1.borrow();
        on_request_recv_finished(ts);

        Ok(())
    });

    // Wait for both futures to complete.
    let receive_fut = receive_fut
        .map_err(|_| ())
        .join(send_fut.map_err(|_| ()))
        .then(move |_| {
            let ts = &tunstub2.borrow();
            info!("[ReqServ] tcp both futures completed:{:?}", ts);
            mgr.borrow_mut().on_request_closed(ts);

            Ok(())
        });

    current_thread::spawn(receive_fut);
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
    let result = tx.unbounded_send(wmsg);
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
    let result = tx.unbounded_send(wmsg);

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
    if let Err(e) = tx.unbounded_send(wmsg) {
        error!("[Tunnel]send_request_closed_to_server tx send failed:{}", e);
    }
}
