use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;

pub struct Request {
    idx: usize,
    pub tx: UnboundedSender<Bytes>,
}

impl Request {
    pub fn new(tx: UnboundedSender<Bytes>) -> Request {
        Request {
            idx: 0,
            tx: tx,
        }
    }

    pub fn bind(&mut self, idx: usize) {
        self.idx = idx;
    }
}
