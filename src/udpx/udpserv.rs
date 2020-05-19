use bytes::BytesMut;
use failure::Error;
use log::{error, info};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use nix::sys::socket::setsockopt;
use nix::sys::socket::sockopt::IpTransparent;

use std::os::unix::io::AsRawFd;
use tokio::net::{UdpSocket};
use futures_03::prelude::*;
use stream_cancel::{Trigger, Tripwire};
use tokio::sync::mpsc;

type TxType = mpsc::UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;
type LongLive = Rc<RefCell<UdpX>>;

pub struct UdpX {
    listen_addr: String,
    tx: (Option<TxType>, Option<Trigger>),
}

impl UdpX {
    pub fn new(addr: &str) -> LongLive {
        info!("[Udpx-Server]new server, addr:{}", addr);
        Rc::new(RefCell::new(UdpX {
            listen_addr: addr.to_string(),
            tx: (None, None),
        }))
    }

    pub fn start(&mut self, ll :LongLive) -> Result<(), Error> {
        let listen_addr = &self.listen_addr;
        let addr: SocketAddr = listen_addr.parse().map_err(|e| Error::from(e))?;
        let socket_udp = std::net::UdpSocket::bind(addr)?;
        let a = UdpSocket::from_std(socket_udp)?;

        let rawfd = a.as_raw_fd();
        info!("[Udpx-Server]listener rawfd:{}", rawfd);
        // enable linux TPROXY
        let enabled = true;
        setsockopt(rawfd, IpTransparent, &enabled).map_err(|e| Error::from(e))?;

        let ll2 = ll.clone();
        let ll3 = ll.clone();

        let udp_framed = tokio_util::udp::UdpFramed::new(a, tokio_util::codec::BytesCodec::new());
        let (a_sink, a_stream) = udp_framed.split();

        let (tx, rx) = mpsc::unbounded_channel();
        let (trigger, tripwire) = Tripwire::new();

        self.set_tx(tx, trigger);

        let send_fut = rx.map(move |x|{Ok(x)}).forward(a_sink);

        let receive_fut = a_stream
            .take_until(tripwire)
            .for_each(move |rr| {
               match rr {
                    Ok((message, addr)) => {
                        let mut rf = ll3.borrow_mut();
                        // post to manager
                        rf.on_receive_udp_msg(message, addr);
                    },
                    Err(e) => error!("[Udpx-Server] for_each failed:{}", e)
                };

                future::ready(())
            });

        // Wait for one future to complete.
        let select_fut = async move {
            future::select(receive_fut, send_fut).await;
            info!("[Udpx-Server] udp both future completed");
            let mut rf = ll2.borrow_mut();
            rf.on_udp_socket_closed();
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
                    error!("[Udpx-Server]unbounded_send error:{}", e);
                }
                _ => {}
            },
            None => {}
        }
    }

    fn on_udp_socket_closed(&mut self) {

    }

    fn on_receive_udp_msg(&mut self, msg: BytesMut, dst_addr: std::net::SocketAddr) {
        
    }
}
