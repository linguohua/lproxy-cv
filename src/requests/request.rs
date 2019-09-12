use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use stream_cancel::Trigger;

pub struct Request {
    pub tag: u16,

    pub request_tx: Option<UnboundedSender<Bytes>>,
    pub trigger: Option<Trigger>,
}

impl Request {
    pub fn new() -> Request {
        Request {
            tag: 0,
            request_tx: None,
            trigger: None,
        }
    }

    pub fn with(tx: UnboundedSender<Bytes>, trigger: Trigger) -> Request {
        Request {
            tag: 0,
            request_tx: Some(tx),
            trigger: Some(trigger),
        }
    }
}
