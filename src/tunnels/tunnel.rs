use crate::requests::North;
use crate::requests::Request;
use super::TunMgr;
use futures::sync::mpsc::UnboundedSender;
use futures::{Future, Sink, Stream};
use std::sync::Arc;
use std::sync::Mutex;

use tokio;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::stream::PeerAddr;
use tungstenite::protocol::Message;
use url;
use crate::requests::Reqq;

pub struct Tunnel {
    pub tx: UnboundedSender<Message>,
    pub index: usize,

    requests: Mutex<Reqq>,
}

impl Tunnel {
    pub fn connect(url: &str, mgr: &Arc<TunMgr>, index: usize) {
        let url = url::Url::parse(&url).unwrap();

        let mgr1 = mgr.clone();
        let client = connect_async(url)
            .and_then(move |(ws_stream, _)| {
                println!("WebSocket handshake has been successfully completed");

                let addr = ws_stream
                    .peer_addr()
                    .expect("connected streams should have a peer address");
                println!("Peer address: {}", addr);

                // Create a channel for our stream, which other sockets will use to
                // send us messages. Then register our address with the stream to send
                // data to us.
                let (tx, rx) = futures::sync::mpsc::unbounded();

                let t = Tunnel {
                    tx: tx,
                    index: index,
                    requests: Mutex::new(Reqq::new(1)),
                };

                let mgr2 = mgr1.clone();
                let mgr3 = mgr1.clone();
                mgr1.on_tunnel_created(index, t);

                // `sink` is the stream of messages going out.
                // `stream` is the stream of incoming messages.
                let (sink, stream) = ws_stream.split();

                let receive_fut = stream.for_each(move |message| {
                    // post to manager
                    let mgr22 = mgr2.clone();
                    mgr22.on_tunnel_msg(message, index);

                    Ok(())
                });

                // Whenever we receive a string on the Receiver, we write it to
                // `WriteHalf<WebSocketStream>`.
                let send_fut = rx.fold(sink, |mut sink, msg| {
                    sink.start_send(msg).unwrap();
                    Ok(sink)
                });

                // Wait for either of futures to complete.
                receive_fut
                    .map(|_| ())
                    .map_err(|_| ())
                    .select(send_fut.map(|_| ()).map_err(|_| ()))
                    .then(move |_| {
                        mgr3.on_tunnel_closed(index);
                        Ok(())
                    })
                // ok(index)
            })
            .map_err(|e| {
                println!("Error during the websocket handshake occurred: {}", e);
                ()
            });

        tokio::spawn(client);
    }

    pub fn on_request_created(&self, req:Request) -> Arc<North> {
        let reqs = &mut self.requests.lock().unwrap();
        let req_idx = reqs.alloc(req);
        let tun_idx = self.index;
        let tx = self.tx.clone();

        Arc::new(North {
            tx: tx,
            tun_idx: tun_idx as u16,
            req_idx: req_idx as u16,
            req_tag: 0,
        })
    }

    pub fn on_tunnel_msg(&self, msg: Message) {
        if msg.is_pong() {
            self.on_pong(msg);

            return;
        }

        // req_idx

        // req_tag
    }

    fn on_pong(&self, msg:Message) {}
}
