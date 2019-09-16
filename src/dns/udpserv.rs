use super::Forwarder;
use super::TxType;
use log::{error, info};
use parking_lot::Mutex;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{UdpFramed, UdpSocket};
use tokio::prelude::*;
use tokio_codec::BytesCodec;

pub struct UdpServer {
    listen_addr: String,
    // TODO: log request count
    tx: Mutex<Option<TxType>>,
}

impl UdpServer {
    pub fn new(addr: &str) -> Arc<UdpServer> {
        info!("[UdpServer]new server, addr:{}", addr);
        Arc::new(UdpServer {
            listen_addr: addr.to_string(),
            tx: Mutex::new(None),
        })
    }

    pub fn start(self: Arc<UdpServer>, forward: &Arc<Forwarder>) {
        let listen_addr = &self.listen_addr;

        let addr: SocketAddr = listen_addr.parse().unwrap();
        let a = UdpSocket::bind(&addr).unwrap();

        let (a_sink, a_stream) = UdpFramed::new(a, BytesCodec::new()).split();

        let forwarder1 = forward.clone();
        let forwarder2 = forward.clone();
        let (tx, rx) = futures::sync::mpsc::unbounded();

        self.set_tx(tx);

        forward.clone().on_dns_udp_created();

        // send future
        let send_fut = a_sink.send_all(rx.map_err(|e| {
            error!("[UdpServer]sink send_all failed:{:?}", e);
            Error::new(ErrorKind::Other, "")
        }));

        let receive_fut = a_stream.for_each(move |(message, addr)| {
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
                forwarder2.on_dns_udp_closed();

                Ok(())
            });

        tokio::spawn(receive_fut);
    }

    fn set_tx(&self, tx2: TxType) {
        let mut tx = self.tx.lock();
        *tx = Some(tx2);
    }

    pub fn get_tx(&self) -> Option<TxType> {
        let tx = self.tx.lock();
        match &*tx {
            Some(tx) => Some(tx.clone()),
            None => None,
        }
    }
}
