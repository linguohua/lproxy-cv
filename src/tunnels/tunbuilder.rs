use crate::config::DEFAULT_REQ_QUOTA;
use super::TunMgr;
use super::Tunnel;
use futures::{Future, Stream};
use log::{debug, error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::rc::Rc;
use tokio;
use tokio::runtime::current_thread;
use url;
use native_tls::TlsConnector;
use crate::lws;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;

pub fn connect(tm: &TunMgr, mgr2: Rc<RefCell<TunMgr>>, index: usize) {
    let relay_domain = &tm.relay_domain;
    let relay_port = tm.relay_port;
    let ws_url = &tm.url;
    let tunnel_req_cap = tm.tunnel_req_cap;
    let ws_url = format!("{}?cap={}&quota={}", ws_url, tunnel_req_cap, DEFAULT_REQ_QUOTA);
    let url = url::Url::parse(&ws_url).unwrap();

    let mgr1 = mgr2.clone();
    let mgr3 = mgr2.clone();
    let mgr4 = mgr2.clone();

    info!(
        "[tunbuilder]WebSocket connect to:{}, port:{}",
        relay_domain, relay_port
    );

    // TODO: need to specify address and port
    let client = ws_connect_async(relay_domain, relay_port, url)
        .and_then(move |(framed, rawfd)| {
            debug!("[tunbuilder]WebSocket handshake has been successfully completed");
            // let inner = ws_stream.get_inner().get_ref();

            // Create a channel for our stream, which other sockets will use to
            // send us messages. Then register our address with the stream to send
            // data to us.
            let (tx, rx) = futures::sync::mpsc::unbounded();
            let t = Rc::new(RefCell::new(Tunnel::new(tx, rawfd, index, tunnel_req_cap)));
            let mut rf = mgr1.borrow_mut();
            if let Err(_) = rf.on_tunnel_created(t.clone()) {
                // TODO: should return directly
                if let Err(e) = shutdown(rawfd, Shutdown::Both) {
                    error!("[tunbuilder]shutdown rawfd failed:{}", e);
                }
            }

            // `sink` is the stream of messages going out.
            // `stream` is the stream of incoming messages.
            let (sink, stream) = framed.split();

            let receive_fut = stream.for_each(move |message| {
                debug!("[tunbuilder]tunnel read a message");
                // post to manager
                let mut clone = t.borrow_mut();
                clone.on_tunnel_msg(message);

                Ok(())
            });

            let rx = rx.map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "[tunbuilder] rx-shit",
                )
            });

            let send_fut = rx.forward(sink);

            // Wait for either of futures to complete.
            receive_fut
                .map(|_| ())
                .map_err(|_| ())
                .select(send_fut.map(|_| ()).map_err(|_| ()))
                .then(move |_| {
                    info!("[tunbuilder] both websocket futures completed");
                    let mut rf = mgr3.borrow_mut();
                    rf.on_tunnel_closed(index);
                    Ok(())
                })
            // ok(index)
        })
        .map_err(move |e| {
            error!(
                "[tunbuilder]Error during the websocket handshake occurred: {}",
                e
            );
            let mut rf = mgr4.borrow_mut();
            rf.on_tunnel_build_error(index);

            ()
        });

    current_thread::spawn(client);
}

pub type FrameType = lws::LwsFramed<tokio_tls::TlsStream<tokio_tcp::TcpStream>>;
pub fn ws_connect_async(relay_domain:&str, relay_port:u16, url2: url::Url) -> impl Future<Item=(FrameType, RawFd), Error=std::io::Error> {
    let cx = TlsConnector::builder().build().unwrap();
    let cx = tokio_tls::TlsConnector::from(cx);
    let host_str = url2.host_str().unwrap().to_string();
    let path = url2.path().to_string();

    let fut = tokio_dns::TcpStream::connect((relay_domain, relay_port))
        .and_then(move |socket| {
            let rawfd = socket.as_raw_fd();
            let tls_handshake = cx.connect(&host_str, socket);
            let fut = tls_handshake
                .map_err(|tslerr| {
                    println!("TLS connect error:{}", tslerr);
                    std::io::Error::from(std::io::ErrorKind::NotConnected)
                })
                .and_then(move |socket| {
                    let handshake =
                        lws::do_client_hanshake(socket, &host_str, &path);
                    let handshake = handshake.and_then(move |(lsocket, tail)| {
                        let framed = lws::LwsFramed::new(lsocket, tail);

                        Ok((framed, rawfd))
                    });

                    handshake
                });

            fut
        });
    fut
}
