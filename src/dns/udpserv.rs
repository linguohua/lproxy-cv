use super::Forwarder;
use super::TxType;
use failure::Error;
use log::{error, info};
use std::net::SocketAddr;
use std::result::Result;
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::net::{UdpFramed, UdpSocket};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio_codec::BytesCodec;

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
        let a = UdpSocket::bind(&addr).map_err(|e| Error::from(e))?;

        let (a_sink, a_stream) = UdpFramed::new(a, BytesCodec::new()).split();

        let forwarder1 = forward.clone();
        let forwarder2 = forward.clone();
        let (tx, rx) = futures::sync::mpsc::unbounded();
        let (trigger, tripwire) = Tripwire::new();

        self.set_tx(tx, trigger);

        fw.on_dns_udp_created(self, forward);

        // send future
        let send_fut = a_sink.send_all(rx.map_err(|e| {
            error!("[UdpServer]sink send_all failed:{:?}", e);
            std::io::Error::from(std::io::ErrorKind::Other)
        }));

        let receive_fut = a_stream
            .take_until(tripwire)
            .for_each(move |(message, addr)| {
                let rf = forwarder1.borrow();
                // post to manager
                if rf.on_dns_udp_msg(&message, &addr) {
                    Ok(())
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::Other))
                }
            });

        // Wait for both futures to complete.
        let receive_fut = receive_fut
            .map_err(|_| ())
            .select(send_fut.map(|_| ()).map_err(|_| ()))
            .then(move |_| {
                info!("[UdpServer] udp both future completed");
                let rf = forwarder2.borrow();
                rf.on_dns_udp_closed();

                Ok(())
            });

        current_thread::spawn(receive_fut);

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
}
