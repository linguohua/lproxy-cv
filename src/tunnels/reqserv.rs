use super::Request;
use super::{Cmd, THeader, TunMgr, TunStub, THEADER_SIZE};
use bytes::BytesMut;
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
use tokio::codec::Decoder;
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio_codec::BytesCodec;
use tokio_io_timeout::TimeoutStream;
use tokio_tcp::TcpStream;
use tungstenite::protocol::Message;

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

    let mut ip_le = 0; // result.sin_addr.s_addr;
    let mut port_le = 0; // result.sin_port.to_be();
    match result {
        Inet(iaddr) => match iaddr {
            V4(v) => {
                ip_le = v.sin_addr.s_addr;
                port_le = v.sin_port.to_be();
            }
            _ => {
                // TODO: ipv6
            }
        },
        _ => {}
    }

    let ipaddr = std::net::Ipv4Addr::from(ip_le.to_be()); // ip_le.to_be()
    info!("[Server]serve_sock, ip:{}, port:{}", ipaddr, port_le);

    // set 2 seconds write-timeout
    let mut socket = TimeoutStream::new(socket);
    let wduration = Duration::new(2, 0);
    socket.set_write_timeout(Some(wduration));

    let framed = BytesCodec::new().framed(socket);
    let (sink, stream) = framed.split();
    let (trigger, tripwire) = Tripwire::new();
    let (tx, rx) = futures::sync::mpsc::unbounded();

    let req = Request::with(tx, trigger, ip_le, port_le);
    let tunstub = mgr.borrow_mut().on_request_created(req);

    if tunstub.is_none() {
        // invalid tunnel
        error!("[Server]failed to alloc tunnel for request!");
        return;
    }

    info!("[Server]allocated tun:{:?}", tunstub);
    let tunstub = Rc::new(RefCell::new(tunstub.unwrap()));
    let req_idx = tunstub.borrow().req_idx;

    // send future
    let send_fut = sink.send_all(rx.map_err(|e| {
        error!("[Server]sink send_all failed:{:?}", e);
        std::io::Error::from(std::io::ErrorKind::Other)
    }));

    let send_fut = send_fut.and_then(move |_| {
        info!("[Server]send_fut end, index:{}", req_idx);
        // shutdown read direction
        if let Err(e) = shutdown(rawfd, Shutdown::Read) {
            error!("[Server]shutdown rawfd error:{}", e);
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
            mgr.borrow_mut().on_request_closed(ts);

            Ok(())
        });

    current_thread::spawn(receive_fut);
}

fn on_request_msg(message: BytesMut, tun: &TunStub) -> bool {
    info!("[ReqMgr]on_request_msg, tun:{}", tun);
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

fn on_request_recv_finished(tun: &TunStub) {
    info!("[ReqMgr]on_request_recv_finished:{}", tun);

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
