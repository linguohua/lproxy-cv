use super::{TMessage, WMessage};
use futures_03::prelude::*;
use std::io::Error;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::prelude::*;
use std::pin::Pin;
use futures_03::task::{Context, Poll};
use futures_03::ready;

pub struct TcpFramed<T> {
    io: T,
    reading: Option<TMessage>,
    writing: Option<WMessage>,
}

impl<T> TcpFramed<T> {
    pub fn new(io: T) -> Self {
        TcpFramed {
            io,
            reading: None,
            writing: None,
        }
    }
}

impl<T> Stream for TcpFramed<T>
where
    T: AsyncRead+Unpin,
{
    type Item = std::result::Result<TMessage,Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            //self.inner.poll()
            if self.reading.is_none() {
                self.reading = Some(TMessage::new());
            }

            let reading = &mut self.reading;
            let msg = reading.as_mut().unwrap();

            // read from io
            let pin_io = Pin::new(&mut self.io);
            let n = ready!(pin_io.poll_read_buf(cx, msg))?;
            if n == 0 {
                return Poll::Ready(None);
            }

            return Poll::Ready(Some(Ok(self.reading.take().unwrap())));
        }
    }
}

impl<T> Sink<WMessage> for TcpFramed<T>
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
