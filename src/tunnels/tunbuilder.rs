use super::TunMgr;
use super::Tunnel;
use crate::{dns, lws};
use futures_03::prelude::*;
use log::{debug, error, info};
use native_tls::TlsConnector;
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::time::Duration;
use tokio;
use tokio::prelude::*;
use url;

pub fn connect(tm: &TunMgr, mgr2: Rc<RefCell<TunMgr>>, index: usize) {
    let relay_domain = &tm.relay_domain;
    let relay_port = tm.relay_port;
    let ws_url = &tm.url;
    let tunnel_req_cap = tm.tunnel_req_cap;
    let request_quota = tm.request_quota;
    let token = &tm.token;

    let ws_url = format!(
        "{}?tok={}&cap={}&quota={}",
        ws_url, token, tunnel_req_cap, request_quota
    );
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
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let t = Rc::new(RefCell::new(Tunnel::new(
                tx,
                rawfd,
                index,
                tunnel_req_cap,
                request_quota,
            )));
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
                std::io::Error::new(std::io::ErrorKind::Other, "[tunbuilder] rx-shit")
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

    tokio::task::spawn_local(client);
}

pub type FrameType = lws::LwsFramed<tokio_tls::TlsStream<tokio::net::TcpStream>>;
pub async fn ws_connect_async(
    relay_domain: &str,
    relay_port: u16,
    url2: url::Url,
) -> std::result::Result<(FrameType, RawFd), std::io::Error> {
    let mut builder = TlsConnector::builder();
    let builder = builder.min_protocol_version(Some(native_tls::Protocol::Tlsv13));
    #[cfg(not(target_arch = "x86_64"))]
    let builder = builder.select_cipher_suit(Some("TLS_CHACHA20_POLY1305_SHA256".to_string()));
    let cx = builder.build().unwrap();

    let cx = tokio_tls::TlsConnector::from(cx);

    let host_str = url2.host_str().unwrap().to_string();
    let path = url2.path().to_string();
    let query = match url2.query() {
        Some(q) => format!("?{}", q),
        None => "".to_string(),
    };

    let path = format!("{}{}", path, query);
    info!(
        "[tunbuilder] ws_connect_async, host:{}, path:{}",
        host_str, path
    );

    let net_fut = async move {
            let ipaddr = dns::MyDns::new(relay_domain.to_string()).await?;
            let addr = std::net::SocketAddr::new(ipaddr, relay_port);
            let socket = tokio::net::TcpStream::connect(&addr).await?;
            let rawfd = socket.as_raw_fd();
            let socket = cx.connect(&host_str, socket).await?;
            let (lsocket,tail) = lws::do_client_hanshake(socket, &host_str, &path).await?;
            let framed = lws::LwsFramed::new(lsocket, tail);
            Ok((framed, rawfd))
    };

    let fut = net_fut
        .timeout(Duration::from_millis(10 * 1000)) // 10 seconds
        .map_err(|err| {
            if err.is_elapsed() {
                std::io::Error::from(std::io::ErrorKind::TimedOut)
            } else if let Some(inner) = err.into_inner() {
                inner
            } else {
                std::io::Error::new(std::io::ErrorKind::Other, "timeout error")
            }
        });

    fut
}
