use bytes::{BufMut, BytesMut};
use futures_03::prelude::*;
use futures_03::ready;
use futures_03::task::{Context, Poll};
use std::io::Error;
use std::pin::Pin;
use tokio::io::{AsyncRead, AsyncWrite};

pub enum CHState {
    WritingHeader,
    ReadingResponse,
    Done,
}

pub struct CHandshake<T> {
    io: Option<T>,
    read_buf: BytesMut,
    wmsg: super::WMessage,
    state: CHState,
}

/// Generate a random key for the `Sec-WebSocket-Key` header.
fn generate_key() -> String {
    // a base64-encoded (see Section 4 of [RFC4648]) value that,
    // when decoded, is 16 bytes in length (RFC 6455)
    let r: [u8; 16] = rand::random();
    base64::encode(&r)
}

pub fn do_client_hanshake<T>(io: T, host: &str, pth: &str) -> CHandshake<T> {
    // format http header
    let key = generate_key();
    let h = format!(
        "\
         GET {} HTTP/1.1\r\n\
         Host: {}\r\n\
         Connection: upgrade\r\n\
         Upgrade: websocket\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Key: {}\r\n\
         \r\n",
        pth, host, key
    );

    let write_buf = h.as_bytes();

    CHandshake {
        io: Some(io),
        read_buf: BytesMut::with_capacity(1024),
        wmsg: super::WMessage::new(write_buf.to_vec(), 0),
        state: CHState::WritingHeader,
    }
}

use bytes::buf::Buf;
impl<T> CHandshake<T> {
    pub fn parse_response(&mut self) -> bool {
        let needle = b"\r\n\r\n";
        let bm = &mut self.read_buf;
        if let Some(pos) = bm.windows(4).position(|window| window == needle) {
            let pos2 = pos + 4;
            bm.advance(pos2);

            return true;
        }

        false
    }
}

impl<T> Future for CHandshake<T>
where
    T: AsyncWrite + AsyncRead + Unpin,
{
    type Output = std::result::Result<(T, Option<Vec<u8>>), Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let self_mut = self.get_mut();
        loop {
            match self_mut.state {
                CHState::WritingHeader => {
                    // write out
                    // let io = self.io.as_mut().unwrap();
                    let mut io = self_mut.io.as_mut().unwrap();
                    let pin_io = Pin::new(&mut io);
                    ready!(pin_io.poll_write_buf(cx, &mut self_mut.wmsg))?;

                    if self_mut.wmsg.is_completed() {
                        self_mut.state = CHState::ReadingResponse;
                    }
                }
                CHState::ReadingResponse => {
                    // read in
                    let bm = &mut self_mut.read_buf;
                    if !bm.has_remaining_mut() {
                        // error, head too large
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "header too large",
                        )));
                    }

                    let mut io = self_mut.io.as_mut().unwrap();
                    let pin_io = Pin::new(&mut io);
                    let n = ready!(pin_io.poll_read_buf(cx, bm))?;
                    if n == 0 {
                        return Poll::Ready(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "can't read completed response",
                        )));
                    }

                    if self_mut.parse_response() {
                        // completed
                        self_mut.state = CHState::Done;
                    } else {
                        // continue loop
                    }
                }
                CHState::Done => {
                    let io = self_mut.io.take().unwrap();
                    let vv;
                    if self_mut.read_buf.len() > 0 {
                        vv = Some(self_mut.read_buf.to_vec());
                    } else {
                        vv = None;
                    }
                    return Poll::Ready(Ok((io, vv)));
                }
            }
        }
    }
}
