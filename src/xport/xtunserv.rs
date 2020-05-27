use super::{LongLive, XTunnel};
use crate::tunnels::ws_connect_async;
use futures_03::prelude::*;
use log::{debug, error, info};
use nix::sys::socket::{shutdown, Shutdown};
use tokio::task;

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
    let f_fut = async move {
        // TODO: need to specify address and port
        let (framed, rawfd) = match ws_connect_async(&relay_domain, relay_port, url).await {
            Ok((f, r)) => (f, r),
            Err(e) => {
                error!(
                    "[xtunserv]Error during the websocket handshake occurred: {}",
                    e
                );
                let mut rf = clone3.borrow_mut();
                rf.on_tunnel_build_error();
                return;
            }
        };

        debug!("[xtunserv]WebSocket handshake has been successfully completed");
        // Create a channel for our stream, which other sockets will use to
        // send us messages. Then register our address with the stream to send
        // data to us.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        {
            let mut rf = ll.borrow_mut();
            if let Err(_) = rf.on_tunnel_created(rawfd, tx) {
                // TODO: should return directly
                if let Err(e) = shutdown(rawfd, Shutdown::Both) {
                    error!("[xtunserv]shutdown rawfd failed:{}", e);
                }
                return;
            }
        }

        // `sink` is the stream of messages going out.
        // `stream` is the stream of incoming messages.
        let (sink, mut stream) = framed.split();
        let clone1 = ll.clone();
        let clone2 = ll.clone();

        let receive_fut = async move {
            while let Some(message) = stream.next().await {
                debug!("[xtunserv]tunnel read a message");

                // post to manager
                match message {
                    Ok(m) => {
                        // post to manager
                        let clone_a = clone1.clone();
                        let mut clone = clone1.borrow_mut();
                        clone.on_tunnel_msg(m, clone_a);
                    }
                    Err(e) => {
                        error!("[xtunserv]tunnel read a message error {}", e);
                    }
                }
            }
        };

        let send_fut = rx.map(move |x|{Ok(x)}).forward(sink);

        // Wait for either of futures to complete.
        future::select(receive_fut.boxed_local(), send_fut).await;
        info!("[xtunserv] both websocket futures completed");
        {
            let mut rf = clone2.borrow_mut();
            rf.on_tunnel_closed();
        }

        ()
    };

    task::spawn_local(f_fut);
}
