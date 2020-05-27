use super::Forwarder;
use super::TxType;
use failure::Error;
use log::{error, info};
use std::net::SocketAddr;
use std::result::Result;

use futures_03::prelude::*;
use stream_cancel::{Trigger, Tripwire};
use tokio::net::UdpSocket;

use std::cell::RefCell;
use std::rc::Rc;

type LongLive = Rc<RefCell<UdpServer>>;
type ForwarderType = Rc<RefCell<Forwarder>>;

pub struct UdpServer {
    listen_addr: String,
    tx: (Option<TxType>, Option<Trigger>),
}

impl UdpServer {
    pub fn new(addr: &str) -> LongLive {
        info!("[UdpServer]new server, addr:{}", addr);
        Rc::new(RefCell::new(UdpServer {
            listen_addr: addr.to_string(),
            tx: (None, None),
        }))
    }

    pub fn start(&mut self, fw: &Forwarder, forward: ForwarderType) -> Result<(), Error> {
        let listen_addr = &self.listen_addr;
        let addr: SocketAddr = listen_addr.parse().map_err(|e| Error::from(e))?;
        let socket_udp = std::net::UdpSocket::bind(addr)?;
        let a = UdpSocket::from_std(socket_udp)?;

        let udp_framed = tokio_util::udp::UdpFramed::new(a, tokio_util::codec::BytesCodec::new());
        let (a_sink, a_stream) = udp_framed.split();

        let forwarder1 = forward.clone();
        let forwarder2 = forward.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (trigger, tripwire) = Tripwire::new();

        self.set_tx(tx, trigger);

        fw.on_dns_udp_created(self, forward);

        // send future
        let send_fut = rx.map(move |x| Ok(x)).forward(a_sink);
        let receive_fut = async move {
            let mut a_stream = a_stream.take_until(tripwire);

            while let Some(rr) = a_stream.next().await {
                match rr {
                    Ok((message, addr)) => {
                        let rf = forwarder1.borrow();
                        // post to manager
                        if rf.on_dns_udp_msg(message, addr) {
                        } else {
                            error!("[UdpServer] on_dns_udp_msg failed");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("[UdpServer] a_stream.next failed:{}", e);
                        break;
                    }
                };
            }
        };

        // Wait for one future to complete.
        let select_fut = async move {
            future::select(receive_fut.boxed_local(), send_fut).await;
            info!("[UdpServer] udp both future completed");
            let rf = forwarder2.borrow();
            rf.on_dns_udp_closed();
        };

        tokio::task::spawn_local(select_fut);

        Ok(())
    }

    fn set_tx(&mut self, tx2: TxType, trigger: Trigger) {
        self.tx = (Some(tx2), Some(trigger));
    }

    pub fn get_tx(&self) -> Option<TxType> {
        let tx = &self.tx;
        let tx = &tx.0;
        match tx {
            Some(tx) => Some(tx.clone()),
            None => None,
        }
    }

    pub fn stop(&mut self) {
        self.tx = (None, None);
    }

    pub fn reply(&self, bm: bytes::Bytes, sa: SocketAddr) {
        match self.get_tx() {
            Some(tx) => match tx.send((bm, sa)) {
                Err(e) => {
                    error!("[UdpServer]unbounded_send error:{}", e);
                }
                _ => {}
            },
            None => {}
        }
    }
}
