use super::DnsTunnel;
use super::Forwarder;
use crate::tunnels::ws_connect_async;
use tokio::sync::mpsc::UnboundedSender;
use futures_03::prelude::*;
use log::{debug, error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::rc::Rc;
use tokio;
use url;

pub type TxType = UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub fn connect(fw: &Forwarder, mgr2: Rc<RefCell<Forwarder>>, index: usize, udp_tx: TxType) {
    let relay_domain = &fw.relay_domain;
    let relay_port = fw.relay_port;
    let ws_url = &fw.dns_tun_url;
    let token = &fw.token;
    let ws_url = format!("{}?dns=1&tok={}", ws_url, token);
    let url = url::Url::parse(&ws_url).unwrap();

    let mgr1 = mgr2.clone();
    let mgr3 = mgr2.clone();
    let mgr4 = mgr2.clone();
    let mgr5 = mgr2.clone();

    // TODO: need to specify address and port
    let client = ws_connect_async(relay_domain, relay_port, url)
        .and_then(move |(ws_stream, rawfd)| {
            debug!("[dnstunbuilder]WebSocket handshake has been successfully completed");
            // let inner = ws_stream.get_inner().get_ref();

            // Create a channel for our stream, which other sockets will use to
            // send us messages. Then register our address with the stream to send
            // data to us.
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            let t = Rc::new(RefCell::new(DnsTunnel::new(tx, rawfd, udp_tx, index)));
            let mut rf = mgr1.borrow_mut();
            if let Err(_) = rf.on_tunnel_created(t.clone()) {
                // TODO: should return directly
                if let Err(e) = shutdown(rawfd, Shutdown::Both) {
                    error!("[dnstunbuilder]shutdown rawfd failed:{}", e);
                }
            }

            // `sink` is the stream of messages going out.
            // `stream` is the stream of incoming messages.
            let (sink, stream) = ws_stream.split();

            let receive_fut = stream.for_each(move |message| {
                debug!("[dnstunbuilder]tunnel read a message");
                // post to manager
                let mut clone = t.borrow_mut();
                clone.on_tunnel_msg(message, &mgr5.borrow());
                Ok(())
            });

            let rx = rx.map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "[dnstunbuilder] rx-shit")
            });

            let send_fut = rx.forward(sink);

            // Wait for either of futures to complete.
            let fut = receive_fut
                .map(|_| ())
                .map_err(|_| ())
                .select(send_fut.map(|_| ()).map_err(|_| ()))
                .then(move |_| {
                    info!("[dnstunbuilder] both websocket futures completed");
                    let mut rf = mgr3.borrow_mut();
                    rf.on_tunnel_closed(index);
                    Ok(())
                });

            fut
            // ok(index)
        })
        .map_err(move |e| {
            error!(
                "[dnstunbuilder]Error during the websocket handshake occurred: {}",
                e
            );
            let mut rf = mgr4.borrow_mut();
            rf.on_tunnel_build_error(index);

            ()
        });

    tokio::task::spawn_local(client);
}
