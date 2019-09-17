use super::Forwarder;
use super::TxType;
use log::{error, info};
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::net::{UdpFramed, UdpSocket};
use tokio::prelude::*;
use tokio_codec::BytesCodec;

use failure::Error;
use std::result::Result;

pub struct UdpServer {
    listen_addr: String,
    // TODO: log request count
    tx: Mutex<(Option<TxType>, Option<Trigger>)>,
}

impl UdpServer {
    pub fn new(addr: &str) -> Arc<UdpServer> {
        info!("[UdpServer]new server, addr:{}", addr);
        Arc::new(UdpServer {
            listen_addr: addr.to_string(),
            tx: Mutex::new((None, None)),
        })
    }

    pub fn start(self: Arc<UdpServer>, forward: &Arc<Forwarder>) -> Result<(), Error> {
        let listen_addr = &self.listen_addr;

        let addr: SocketAddr = listen_addr.parse().map_err(|e| Error::from(e))?;
        let a = UdpSocket::bind(&addr).map_err(|e| Error::from(e))?;

        let (a_sink, a_stream) = UdpFramed::new(a, BytesCodec::new()).split();

        let forwarder1 = forward.clone();
        let forwarder2 = forward.clone();
        let (tx, rx) = futures::sync::mpsc::unbounded();
        let (trigger, tripwire) = Tripwire::new();

        self.set_tx(tx, trigger);

        forward.clone().on_dns_udp_created();

        // send future
        let send_fut = a_sink.send_all(rx.map_err(|e| {
            error!("[UdpServer]sink send_all failed:{:?}", e);
            std::io::Error::from(std::io::ErrorKind::Other)
        }));

        let receive_fut = a_stream
            .take_until(tripwire)
            .for_each(move |(message, addr)| {
                // post to manager
                if forwarder1.on_dns_udp_msg(&message, &addr) {
                    Ok(())
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
                }
            });

        // Wait for both futures to complete.
        let receive_fut = receive_fut
            .map_err(|_| ())
            .join(send_fut.map_err(|_| ()))
            .then(move |_| {
                info!("[UdpServer] udp both future completed");
                forwarder2.on_dns_udp_closed();

                Ok(())
            });

        tokio::spawn(receive_fut);

        Ok(())
    }

    fn set_tx(&self, tx2: TxType, trigger: Trigger) {
        let mut tx = self.tx.lock();
        *tx = (Some(tx2), Some(trigger));
    }

    pub fn get_tx(&self) -> Option<TxType> {
        let tx = self.tx.lock();
        let tx = &tx.0;
        match tx {
            Some(tx) => Some(tx.clone()),
            None => None,
        }
    }

    pub fn stop(&self) {
        let mut tx = self.tx.lock();
        *tx = (None, None);
    }
}
