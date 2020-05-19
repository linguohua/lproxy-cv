use std::os::unix::io::RawFd;
use tokio::io::PollEvented;
use bytes::BytesMut;
use failure::Error;
use log::{error, info};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use nix::sys::socket::setsockopt;
use nix::sys::socket::sockopt::IpTransparent;
use futures_03::prelude::*;
use futures_03::ready;
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use futures_03::task::{Context, Poll};
use std::os::unix::io::AsRawFd;
use futures_03::prelude::*;
use stream_cancel::{Trigger, Tripwire};
use tokio::sync::mpsc;
use std::io;

pub struct UdpSocketEx {
    io: PollEvented<mio::net::UdpSocket>,
    rawfd: RawFd
}

impl UdpSocketEx {
    pub fn new(raw_sock: std::net::UdpSocket) -> Self {
        let sys = mio::net::UdpSocket::from_socket(raw_sock).unwrap();
        let rawfd = sys.as_raw_fd();
        super::set_ip_recv_origdstaddr(rawfd).unwrap();

        UdpSocketEx {
            io:PollEvented::new(sys).unwrap(),
            rawfd,
        }
    }

    pub fn poll_recv_from(
        &self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, SocketAddr, SocketAddr), io::Error>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;
        
        let rawfd = self.rawfd;
        match super::recv_sas(rawfd, buf) {
            Ok((n, src_addr, dst_addr)) => {
                let src_addr = src_addr.unwrap();
                let dst_addr = dst_addr.unwrap();

                Poll::Ready(Ok((n, src_addr, dst_addr)))
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    self.io.clear_read_ready(cx, mio::Ready::readable())?;
                    Poll::Pending
                } else {
                    Poll::Ready(Err(e))
                }
            }
        }
    }

    pub fn poll_send_to(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: &SocketAddr,
    ) -> Poll<io::Result<usize>> {
        ready!(self.io.poll_write_ready(cx))?;

        match self.io.get_ref().send_to(buf, target) {
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_write_ready(cx)?;
                Poll::Pending
            }
            x => Poll::Ready(x),
        }
    }
}
