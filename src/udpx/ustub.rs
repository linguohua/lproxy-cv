
use tokio::sync::mpsc::UnboundedSender;
use std::net::SocketAddr;
use std::io::Error;
use std::result::Result;
use log::{error, info};
use std::os::unix::io::FromRawFd;
use futures_03::prelude::*;
use stream_cancel::{Trigger, Tripwire};
use std::os::unix::io::RawFd;
use nix::sys::socket::{shutdown, Shutdown};
use tokio::net::UdpSocket;
use bytes::Bytes;

type TxType = UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub struct UStub {
    rawfd: RawFd,
    tx: Option<TxType>,
    tigger: Option<Trigger>,
}

impl UStub {
    pub fn new(src_addr: &SocketAddr, ll: super::LongLiveX) -> Result<Self, Error> {
        let mut stub = UStub {
            rawfd: 0,
            tx: None, 
            tigger: None,
        };

        stub.start_udp_socket(src_addr, ll)?;

        Ok(stub)
    }

    pub fn on_udp_proxy_south(&self, msg: Bytes, dst_addr: SocketAddr) {
        if self.tx.is_none() {
            error!("[UStub] on_udp_proxy_south failed, no tx");
            return;
        }

        match self.tx.as_ref().unwrap().send((msg, dst_addr)){
            Err(e) => {
                error!("[UStub]on_udp_proxy_south, send tx msg failed:{}", e);
            }
            _ => {}
        }
    }

    pub fn cleanup(&mut self) {
        // TODO: close socket and send fut
        self.close_rawfd();

        self.tx = None;
        self.tigger = None;
    }

    fn close_rawfd(&self) {
        info!("[UStub]close_rawfd");
        let r = shutdown(self.rawfd, Shutdown::Both);
        match r {
            Err(e) => {
                info!("[UStub]close_rawfd failed:{}", e);
            }
            _ => {}
        }
    }

    fn set_tx(&mut self, tx2: TxType, trigger: Trigger) {
        self.tx = Some(tx2); 
        self.tigger = Some(trigger);
    }

    fn start_udp_socket(&mut self, src_addr: &SocketAddr, ll: super::LongLiveX) -> std::result::Result<(), Error> {
        // let sockudp = std::net::UdpSocket::new();
        let rawfd = super::sas_socket(src_addr)?;
        let socket_udp = unsafe{std::net::UdpSocket::from_raw_fd(0)};
        let a = UdpSocket::from_std(socket_udp)?;

        let udp_framed = tokio_util::udp::UdpFramed::new(a, tokio_util::codec::BytesCodec::new());
        let (a_sink, a_stream) = udp_framed.split();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (trigger, tripwire) = Tripwire::new();

        self.set_tx(tx, trigger);
        self.rawfd = rawfd;

        let ll2 = ll.clone();
        let target_addr = *src_addr;
        // send future
        let send_fut = rx.map(move |x|{Ok(x)}).forward(a_sink);
        let receive_fut = a_stream
        .take_until(tripwire)
        .for_each(move |rr| {
            match rr {
                Ok((message, addr)) => {
                    let rf = ll.borrow();
                    // post to manager
                    rf.on_udp_msg_forward(message, addr, target_addr);
                },
                Err(e) => error!("[UStub] for_each failed:{}", e)
            };

            future::ready(())
        });


        // Wait for one future to complete.
        let select_fut = async move {
            future::select(receive_fut, send_fut).await;
            info!("[UStub] udp both future completed");
            let mut rf = ll2.borrow_mut();
            rf.on_ustub_closed(&target_addr);
        };

        tokio::task::spawn_local(select_fut);

        Ok(())
    }
}