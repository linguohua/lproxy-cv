use super::{RMessage, WMessage};
use bytes::BufMut;
use futures_03::prelude::*;
use futures_03::ready;
use std::io::Error;
use tokio::io::{AsyncRead, AsyncWrite};
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
        let self_mut = self.get_mut();
        loop {
            //self.inner.poll()
            if self_mut.reading.is_none() {
                self_mut.reading = Some(RMessage::new());
            }

            let reading = &mut self_mut.reading;
            let msg = reading.as_mut().unwrap();

            if self_mut.tail.is_some() {
                // has tail, handle tail first
                // self.read_from_tail(msg);
                let tail = &mut self_mut.tail;
                let tail = tail.as_mut().unwrap();

                let tail = read_from_tail(tail, msg);
                if tail.len() < 1 {
                    self_mut.tail = None;
                } else {
                    self_mut.tail = Some(tail);
                }
            } else {
                // read from io
                let mut io = &mut self_mut.io;
                let pin_io = Pin::new(&mut io);
                let n = ready!(pin_io.poll_read_buf(cx,msg))?;
                if n == 0 {
                    return Poll::Ready(None);
                }
            }

            if msg.is_completed() {
                // if message is completed
                // return ready
                return Poll::Ready(Some(Ok(self_mut.reading.take().unwrap())));
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
            return self.poll_flush(cx)
        }

        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: WMessage) -> Result<(), Self::Error> {
        let self_mut = self.get_mut();
        self_mut.writing = Some(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let self_mut = self.get_mut();
        
        if self_mut.writing.is_some() {
            let writing = self_mut.writing.as_mut().unwrap();
            loop {
                let pin_io = Pin::new(&mut self_mut.io);
                ready!(pin_io.poll_write_buf(cx,writing))?;

                if writing.is_completed() {
                    self_mut.writing = None;
                    break;
                }
            }
        }

        // Try flushing the underlying IO
        let pin_io = Pin::new(&mut self_mut.io);
        ready!(pin_io.poll_flush(cx))?;

        return Poll::Ready(Ok(()));
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.poll_flush(cx))?;
        Poll::Ready(Ok(()))
    }
}
