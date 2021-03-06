use super::LongLiveX;
use failure::Error;
use futures_03::prelude::*;
use log::{error, info};
use nix::sys::socket::setsockopt;
use nix::sys::socket::sockopt::IpTransparent;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use stream_cancel::{Trigger, Tripwire};

type LongLive = Rc<RefCell<UdpServer>>;

pub struct UdpServer {
    listen_addr: String,
    tx: Option<Trigger>,
}

impl UdpServer {
    pub fn new(addr: String) -> LongLive {
        info!("[Udpx-Server]new server, addr:{}", addr);
        Rc::new(RefCell::new(UdpServer {
            listen_addr: addr,
            tx: None,
        }))
    }

    pub fn start(&mut self, llx: LongLiveX) -> Result<(), Error> {
        let listen_addr = &self.listen_addr;
        let addr: SocketAddr = listen_addr.parse().map_err(|e| Error::from(e))?;
        let socket_udp = std::net::UdpSocket::bind(addr)?;

        let rawfd = socket_udp.as_raw_fd();
        info!("[Udpx-Server]listener rawfd:{}", rawfd);
        // enable linux TPROXY
        let enabled = true;
        setsockopt(rawfd, IpTransparent, &enabled).map_err(|e| Error::from(e))?;

        let a_stream = super::UdpSocketEx::new(socket_udp);

        let (trigger, tripwire) = Tripwire::new();
        self.set_trigger(trigger);
        let llx2 = llx.clone();

        // Wait for one future to complete.
        let select_fut = async move {
            let mut a_stream = a_stream.take_until(tripwire);

            while let Some(rr) = a_stream.next().await {
                match rr {
                    Ok((message, saddr, daddr)) => {
                        //let mut rf = ll3.borrow_mut();
                        // post to manager
                        //rf.on_receive_udp_msg(message, addr);
                        info!(
                            "[Udpx-Server] recv udp from:{}, to:{}, len:{}",
                            saddr,
                            daddr,
                            message.len()
                        );
                        let rf = llx.borrow();
                        rf.on_udp_msg_forward(message, saddr, daddr);
                    }
                    Err(e) => {
                        error!("[Udpx-Server] a_stream.next failed:{}", e);
                        break;
                    }
                }
            }

            info!("[Udpx-Server] udp both future completed");
            let mut rf = llx2.borrow_mut();
            rf.on_udp_server_closed();
        };

        tokio::task::spawn_local(select_fut);

        Ok(())
    }

    fn set_trigger(&mut self, trigger: Trigger) {
        self.tx = Some(trigger);
    }

    pub fn stop(&mut self) {
        self.tx = None;
    }
}
