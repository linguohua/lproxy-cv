use super::{LongLive, XTunnel};
use crate::lws::{TcpFramed, WMessage};
use futures_03::prelude::*;
use log::{error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::time::Duration;
use stream_cancel::Tripwire;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

pub fn proxy_request(
    xtun: &mut XTunnel,
    ll: LongLive,
    req_idx: u16,
    req_tag: u16,
    port: u16,
) -> bool {
    let (tx, rx) = unbounded_channel();
    let (trigger, tripwire) = Tripwire::new();
    if let Err(_) = xtun.save_request_tx(tx, trigger, req_idx, req_tag) {
        error!("[XPort]save_request_tx failed");
        return false;
    }

    let ip = "127.0.0.1".parse().unwrap(); // always point to localhost
    let sockaddr = SocketAddr::new(ip, port);
    info!("[XPort] proxy request to ip:{:?}", sockaddr);

    let tl0 = ll.clone();
    let fut = async move {
        let ff = TcpStream::connect(&sockaddr);

        let fut = tokio::time::timeout(Duration::from_millis(5 * 1000), ff); // 10 seconds
        match fut.await {
            Err(e) => {
                error!("[XPort] tcp connect failed:{}", e);
                let mut tun = tl0.borrow_mut();
                tun.on_request_connect_error(req_idx, req_tag);
            }
            Ok(s) => match s {
                Ok(s1) => proxy_request_internal(s1, rx, tripwire, ll, req_idx, req_tag),
                Err(e1) => error!("[XPort] tcp connect failed:{}", e1),
            },
        }
    };

    tokio::task::spawn_local(fut);
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
    let send_fut = rx.map(move |x| Ok(x)).forward(sink);

    let send_fut = async move {
        match send_fut.await {
            Err(e) => {
                error!("[XPort]send_fut error:{}", e);
            }
            _ => {}
        }

        info!("[XPort]send_fut end, index:{}", req_idx);
        // shutdown write direction
        if let Err(e) = shutdown(rawfd, Shutdown::Write) {
            error!("[XPort]shutdown rawfd error:{}", e);
        }

        ()
    };

    let receive_fut = async move {
        let mut stream = stream.take_until(tripwire);
        while let Some(message) = stream.next().await {
            match message {
                Ok(m) => {
                    let mut tun_b = tl2.borrow_mut();
                    // post to manager
                    if !tun_b.on_request_msg(m, req_idx, req_tag) {
                        // return Err(std::io::Error::from(std::io::ErrorKind::Other));
                        info!("[XPort]on_request_msg failed, index:{}", req_idx);
                    }
                }
                Err(e) => {
                    info!("[XPort]stream stream.next faield, index:{}, {}", req_idx, e);
                }
            }
        }

        let mut tun_b = tl3.borrow_mut();
        // client(of request) send finished(FIN), indicate that
        // no more data to send
        tun_b.on_request_recv_finished(req_idx, req_tag);
    };

    // Wait for both futures to complete.
    let join_fut = async move {
        future::join(send_fut, receive_fut).await;
        info!("[XPort] tcp both futures completed");
        let mut tun = tl4.borrow_mut();
        tun.on_request_closed(req_idx, req_tag);

        ()
    };

    tokio::task::spawn_local(join_fut);
}
