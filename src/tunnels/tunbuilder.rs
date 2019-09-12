use super::TunMgr;
use super::Tunnel;
use futures::{Future, Stream};
use std::sync::Arc;
use tokio;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::stream::PeerAddr;
use url;

use log::{debug, error};

pub fn connect(url: &str, mgr: &Arc<TunMgr>, index: usize) {
    let url = url::Url::parse(&url).unwrap();

    let mgr1 = mgr.clone();
    let mgr2 = mgr.clone();

    let client = connect_async(url)
        .and_then(move |(ws_stream, _)| {
            debug!("[tunbuilder]WebSocket handshake has been successfully completed");
            // let inner = ws_stream.get_inner().get_ref();

            let addr = ws_stream
                .peer_addr()
                .expect("[tunbuilder]connected streams should have a peer address");
            debug!("[tunbuilder]Peer address: {}", addr);

            // Create a channel for our stream, which other sockets will use to
            // send us messages. Then register our address with the stream to send
            // data to us.
            let (tx, rx) = futures::sync::mpsc::unbounded();

            let t = Arc::new(Tunnel::new(tx, index));
            mgr1.on_tunnel_created(&t);

            // `sink` is the stream of messages going out.
            // `stream` is the stream of incoming messages.
            let (sink, stream) = ws_stream.split();

            let receive_fut = stream.for_each(move |message| {
                debug!("[tunbuilder]tunnel read a message");
                // post to manager
                if t.on_tunnel_msg(message) {
                    Ok(())
                } else {
                    Err(tungstenite::Error::ConnectionClosed)
                }
            });

            let rx = rx.map_err(|_| {
                tungstenite::error::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "[tunbuilder] rx-shit",
                ))
            });

            let send_fut = rx.forward(sink);

            // Wait for either of futures to complete.
            receive_fut
                .map_err(|_| ())
                .select(send_fut.map(|_| ()).map_err(|_| ()))
                .then(move |_| {
                    mgr1.on_tunnel_closed(index);
                    Ok(())
                })
            // ok(index)
        })
        .map_err(move |e| {
            error!(
                "[tunbuilder]Error during the websocket handshake occurred: {}",
                e
            );
            mgr2.on_tunnel_build_error(index);

            ()
        });

    tokio::spawn(client);
}
