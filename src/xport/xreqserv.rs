use super::{LongLive, XTunnel};
use crate::lws::{TcpFramed, WMessage};
use futures::future::Future;
use futures::sync::mpsc::{unbounded, UnboundedReceiver};
use log::{error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::time::Duration;
use stream_cancel::{StreamExt, Tripwire};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio::timer::Timeout;
use tokio_tcp::TcpStream;

pub fn proxy_request(
    xtun: &mut XTunnel,
    ll: LongLive,
    req_idx: u16,
    req_tag: u16,
    port: u16,
) -> bool {
    let (tx, rx) = unbounded();
    let (trigger, tripwire) = Tripwire::new();
    if let Err(_) = xtun.save_request_tx(tx, trigger, req_idx, req_tag) {
        error!("[XPort]save_request_tx failed");
        return false;
    }

    let ip = "127.0.0.1".parse().unwrap(); // always point to localhost
    let sockaddr = SocketAddr::new(ip, port);
    info!("[XPort] proxy request to ip:{:?}", sockaddr);

    let tl0 = ll.clone();
    let fut = TcpStream::connect(&sockaddr).and_then(move |socket| {
        proxy_request_internal(socket, rx, tripwire, ll, req_idx, req_tag);

        Ok(())
    });

    let fut = Timeout::new(fut, Duration::from_secs(5)).map_err(move |e| {
        error!("[XPort] tcp connect failed:{}", e);
        let mut tun = tl0.borrow_mut();
        tun.on_request_connect_error(req_idx, req_tag);
        ()
    });

    current_thread::spawn(fut);
    true
}

fn proxy_request_internal(
    socket: TcpStream,
    rx: UnboundedReceiver<WMessage>,
    tripwire: Tripwire,
    tl: LongLive,
    req_idx: u16,
    req_tag: u16,
) {
    // config tcp stream
    socket.set_linger(None).unwrap();
    let kduration = Duration::new(3, 0);
    socket.set_keepalive(Some(kduration)).unwrap();
    // socket.set_nodelay(true).unwrap();

    let rawfd = socket.as_raw_fd();
    let framed = TcpFramed::new(socket);
    let (sink, stream) = framed.split();

    let tl2 = tl.clone();
    let tl3 = tl.clone();
    let tl4 = tl.clone();

    // send future
    let send_fut = sink.send_all(rx.map_err(|e| {
        error!("[XPort]sink send_all failed:{:?}", e);
        std::io::Error::from(std::io::ErrorKind::Other)
    }));

    let send_fut = send_fut.and_then(move |_| {
        info!("[XPort]send_fut end, index:{}", req_idx);
        // shutdown read direction
        if let Err(e) = shutdown(rawfd, Shutdown::Read) {
            error!("[XPort]shutdown rawfd error:{}", e);
        }

        Ok(())
    });

    let receive_fut = stream.take_until(tripwire).for_each(move |message| {
        let mut tun_b = tl2.borrow_mut();
        // post to manager
        if !tun_b.on_request_msg(message, req_idx, req_tag) {
            return Err(std::io::Error::from(std::io::ErrorKind::Other));
        }

        Ok(())
    });

    let receive_fut = receive_fut.and_then(move |_| {
        let mut tun_b = tl3.borrow_mut();
        // client(of request) send finished(FIN), indicate that
        // no more data to send
        tun_b.on_request_recv_finished(req_idx, req_tag);

        Ok(())
    });

    // Wait for both futures to complete.
    let receive_fut = receive_fut
        .map_err(|err| {
            error!("[XPort] tcp receive_fut error:{}", err);
            ()
        })
        .join(send_fut.map_err(|err| {
            error!("[XPort] tcp send_fut error:{}", err);
            ()
        }))
        .then(move |_| {
            info!("[XPort] tcp both futures completed");
            let mut tun = tl4.borrow_mut();
            tun.on_request_closed(req_idx, req_tag);

            Ok(())
        });

    current_thread::spawn(receive_fut);
}
