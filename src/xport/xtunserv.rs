use super::{LongLive, XTunnel};
use crate::tunnels::ws_connect_async;
use futures::{Future, Stream};
use log::{debug, error, info};
use nix::sys::socket::{shutdown, Shutdown};
use tokio::runtime::current_thread;

pub fn xtunel_connect(xtun: &mut XTunnel, ll: LongLive) {
    let ws_url = &xtun.url_string;
    let ws_url = format!("{}?cap={}&tok={}", ws_url, xtun.req_cap, xtun.token);
    let url = url::Url::parse(&ws_url).unwrap(); // should not failed
    let relay_domain = url.host_str().unwrap(); // should not failed
    let relay_domain = relay_domain.to_string();
    let relay_port = url.port_or_known_default().unwrap(); // should not failed

    info!(
        "[xtunserv]WebSocket connect to:{}, port:{}",
        relay_domain, relay_port
    );

    let clone3 = ll.clone();

    // TODO: need to specify address and port
    let client = ws_connect_async(&relay_domain, relay_port, url)
        .and_then(move |(framed, rawfd)| {
            debug!("[xtunserv]WebSocket handshake has been successfully completed");
            // let inner = ws_stream.get_inner().get_ref();

            // Create a channel for our stream, which other sockets will use to
            // send us messages. Then register our address with the stream to send
            // data to us.
            let (tx, rx) = futures::sync::mpsc::unbounded();
            let mut rf = ll.borrow_mut();
            if let Err(_) = rf.on_tunnel_created(rawfd, tx) {
                // TODO: should return directly
                if let Err(e) = shutdown(rawfd, Shutdown::Both) {
                    error!("[tunbuilder]shutdown rawfd failed:{}", e);
                }
            }

            // `sink` is the stream of messages going out.
            // `stream` is the stream of incoming messages.
            let (sink, stream) = framed.split();
            let clone1 = ll.clone();
            let clone2 = ll.clone();

            let receive_fut = stream.for_each(move |message| {
                debug!("[tunbuilder]tunnel read a message");
                // post to manager
                let clone_a = clone1.clone();
                let mut clone = clone1.borrow_mut();
                clone.on_tunnel_msg(message, clone_a);

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
                    let mut rf = clone2.borrow_mut();
                    rf.on_tunnel_closed();
                    Ok(())
                })
            // ok(index)
        })
        .map_err(move |e| {
            error!(
                "[tunbuilder]Error during the websocket handshake occurred: {}",
                e
            );
            let mut rf = clone3.borrow_mut();
            rf.on_tunnel_build_error();

            ()
        });

    current_thread::spawn(client);
}
