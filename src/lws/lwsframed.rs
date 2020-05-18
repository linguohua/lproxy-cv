use super::{RMessage, WMessage};
use bytes::BufMut;
use futures_03::prelude::*;
use futures_03::ready;
use std::io::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::prelude::*;
use std::pin::Pin;
use futures_03::task::{Context, Poll};

pub struct LwsFramed<T> {
    io: T,
    reading: Option<RMessage>,
    writing: Option<WMessage>,
    tail: Option<Vec<u8>>,
}

impl<T> LwsFramed<T> {
    pub fn new(io: T, tail: Option<Vec<u8>>) -> Self {
        LwsFramed {
            io,
            reading: None,
            writing: None,
            tail,
        }
    }
}

fn read_from_tail<B: BufMut>(vec: &mut Vec<u8>, bf: &mut B) -> Vec<u8> {
    let remain_in_bf = bf.remaining_mut();
    let len_in_vec = vec.len();
    let len_to_copy = if len_in_vec < remain_in_bf {
        len_in_vec
    } else {
        remain_in_bf
    };

    bf.put_slice(&vec[..len_to_copy]);
    vec.split_off(len_to_copy)
}

impl<T> Stream for LwsFramed<T>
where
    T: AsyncRead+Unpin,
{
    type Item = std::result::Result<RMessage,Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            //self.inner.poll()
            if self.reading.is_none() {
                self.reading = Some(RMessage::new());
            }

            let reading = &mut self.reading;
            let msg = reading.as_mut().unwrap();

            if self.tail.is_some() {
                // has tail, handle tail first
                // self.read_from_tail(msg);
                let tail = &mut self.tail;
                let tail = tail.as_mut().unwrap();

                let tail = read_from_tail(tail, msg);
                if tail.len() < 1 {
                    self.tail = None;
                } else {
                    self.tail = Some(tail);
                }
            } else {
                // read from io
                let pin_io = Pin::new(&mut self.io);
                let n = ready!(pin_io.poll_read_buf(cx,msg))?;
                if n == 0 {
                    return Poll::Ready(None);
                }
            }

            if msg.is_completed() {
                // if message is completed
                // return ready
                return Poll::Ready(Some(Ok(self.reading.take().unwrap())));
            }
        }
    }
}

impl<T> Sink<WMessage> for LwsFramed<T>
where
    T: AsyncWrite+Unpin,
{
    type Error = Error;
    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.writing.is_some() {
            match self.poll_flush(cx)? {
                Poll::Ready(()) => {}
                Poll::Pending => return Poll::Pending,
            }
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: WMessage) -> Result<(), Self::Error> {
        self.writing = Some(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let pin_io = Pin::new(&mut self.io);
        if self.writing.is_some() {
            let writing = self.writing.as_mut().unwrap();
            loop {
                ready!(pin_io.poll_write_buf(cx,writing))?;

                if writing.is_completed() {
                    self.writing = None;
                    break;
                }
            }
        }

        // Try flushing the underlying IO
        ready!(pin_io.poll_flush(cx))?;

        return Poll::Ready(Ok(()));
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }
}
