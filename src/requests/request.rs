use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;

pub struct Request {
    pub tag: u16,

    pub tx: Option<UnboundedSender<Bytes>>,
}

impl Request {
    pub fn new() -> Request {
        Request { tag: 0, tx: None }
    }
}
