use super::TunMgr;
use crate::requests::North;
use crate::requests::Reqq;
use byte::*;
use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use futures::{Future, Sink, Stream};
use std::sync::Arc;
use std::sync::Mutex;
use tokio;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::stream::PeerAddr;
use tungstenite::protocol::Message;
use url;

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

    pub fn on_tunnel_msg(&self, msg: Message) {
        if msg.is_pong() {
            self.on_pong(msg);

            return;
        }

        let bs = msg.into_data();
        let offset = &mut 0;
        let cmd = bs.read_with::<u8>(offset, LE).unwrap();
        if cmd == 0 {
            // data
            let req_idx = bs.read_with::<u16>(offset, LE).unwrap();
            let req_tag = bs.read_with::<u16>(offset, LE).unwrap();

            let tx = self.get_request_tx(req_idx, req_tag);
            match tx {
                None => {
                    println!("no request found for: {}:{}", req_idx, req_tag);
                }
                Some(tx) => {
                    let b = Bytes::from(&bs[4..]);
                    tx.unbounded_send(b).unwrap();
                }
            }
        }
    }

    fn on_pong(&self, _msg: Message) {}

    fn get_request_tx(&self, req_idx: u16, req_tag: u16) -> Option<UnboundedSender<Bytes>> {
        let requests = &self.requests.lock().unwrap();
        let req_idx = req_idx as usize;
        if req_idx >= requests.elements.len() {
            return None;
        }

        let req = &requests.elements[req_idx];
        if req.tag == req_tag && req.request_tx.is_some() {
            match req.request_tx {
                None => {
                    return None;
                }
                Some(ref tx) => {
                    return Some(tx.clone());
                }
            }
        }

        None
    }

    pub fn on_request_created(&self, req_tx: &UnboundedSender<Bytes>) -> Arc<North> {
        let reqs = &mut self.requests.lock().unwrap();
        let (idx, tag) = reqs.alloc(req_tx);
        let tun_idx = self.index;
        let tx = self.tx.clone();

        Arc::new(North {
            tunnel_tx: tx,
            tun_idx: tun_idx as u16,
            req_idx: idx,
            req_tag: tag,
        })
    }

    pub fn on_request_closed(&self, north: &Arc<North>) {
        let reqs = &mut self.requests.lock().unwrap();

        reqs.free(north.req_idx, north.req_tag);
    }
}
