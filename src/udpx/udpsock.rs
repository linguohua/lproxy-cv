use std::os::unix::io::RawFd;
use tokio::io::PollEvented;
use bytes::BytesMut;
use failure::Error;
use std::net::SocketAddr;
use futures_03::prelude::*;
use futures_03::ready;
use std::pin::Pin;
use futures_03::task::{Context, Poll};
use std::os::unix::io::AsRawFd;
use std::io;
use bytes::buf::BufMut;


const INITIAL_RD_CAPACITY:usize = 640;

pub struct UdpSocketEx {
    io: PollEvented<mio::net::UdpSocket>,
    rawfd: RawFd,
    rd: BytesMut,
}

impl UdpSocketEx {
    pub fn new(raw_sock: std::net::UdpSocket) -> Self {
        let sys = mio::net::UdpSocket::from_socket(raw_sock).unwrap();
        let rawfd = sys.as_raw_fd();
        super::set_ip_recv_origdstaddr(rawfd).unwrap();

        UdpSocketEx {
            io:PollEvented::new(sys).unwrap(),
            rawfd,
            rd: BytesMut::with_capacity(INITIAL_RD_CAPACITY),
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
}

impl Stream for UdpSocketEx {
    type Item = std::result::Result<(BytesMut, SocketAddr, SocketAddr),Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let pin = self.get_mut();

        pin.rd.reserve(INITIAL_RD_CAPACITY);

        let (_, saddr, daddr) = unsafe {
            // Read into the buffer without having to initialize the memory.
            //
            // safety: we know tokio::net::UdpSocket never reads from the memory
            // during a recv
            let res = {
                let bytes = &mut *(pin.rd.bytes_mut() as *mut _ as *mut [u8]);
                ready!(pin.poll_recv_from(cx, bytes))
            };

            let (n, saddr, daddr) = res?;
            pin.rd.advance_mut(n);
            (n, saddr, daddr)
        };

        let len = pin.rd.len();
        let rd = pin.rd.split_to(len);
        pin.rd.clear();
        Poll::Ready(Some(Ok((rd, saddr, daddr))))
    }
}
