use bytes::{BufMut, BytesMut};
use futures::prelude::*;
use futures::try_ready;
use std::io::Error;
use tokio::prelude::*;

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

impl<T> CHandshake<T> {
    pub fn parse_response(&mut self) -> bool {
        let needle = b"\r\n\r\n";
        let bm = &mut self.read_buf;
        if let Some(pos) = bm.windows(4).position(|window| window == needle) {
            let pos2 = pos + 4;
            bm.split_to(pos2);

            return true;
        }

        false
    }
}

impl<T> Future for CHandshake<T>
where
    T: AsyncWrite + AsyncRead,
{
    type Item = (T, Option<Vec<u8>>);
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                CHState::WritingHeader => {
                    // write out
                    let io = self.io.as_mut().unwrap();
                    try_ready!(io.write_buf(&mut self.wmsg));

                    if self.wmsg.is_completed() {
                        self.state = CHState::ReadingResponse;
                    }
                }
                CHState::ReadingResponse => {
                    // read in
                    let io = self.io.as_mut().unwrap();
                    let bm = &mut self.read_buf;
                    if !bm.has_remaining_mut() {
                        // error, head too large
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "header too large",
                        ));
                    }

                    try_ready!(io.read_buf(bm));

                    if self.parse_response() {
                        // completed
                        self.state = CHState::Done;
                    } else {
                        // continue loop
                    }
                }
                CHState::Done => {
                    let io = self.io.take().unwrap();
                    let vv;
                    if self.read_buf.len() > 0 {
                        vv = Some(self.read_buf.to_vec());
                    } else {
                        vv = None;
                    }
                    return Ok(Async::Ready((io, vv)));
                }
            }
        }
    }
}
