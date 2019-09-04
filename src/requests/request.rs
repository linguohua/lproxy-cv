use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;

pub struct Request {
    pub tag: u16,

    pub request_tx: Option<UnboundedSender<Bytes>>,
}

impl Request {
    pub fn new() -> Request {
        Request { tag: 0, request_tx: None }
    }
}
