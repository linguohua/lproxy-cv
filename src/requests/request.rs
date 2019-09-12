use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use stream_cancel::Trigger;
use std::fmt;

pub struct Request {
    pub index: u16,
    pub tag: u16,

    pub request_tx: Option<UnboundedSender<Bytes>>,
    pub trigger: Option<Trigger>,
}

impl Request {
    pub fn new(idx:u16) -> Request {
        Request {
            index:idx,
            tag: 0,
            request_tx: None,
            trigger: None,
        }
    }

    pub fn with(tx: UnboundedSender<Bytes>, trigger: Trigger) -> Request {
        Request {
            index:0,
            tag: 0,
            request_tx: Some(tx),
            trigger: Some(trigger),
        }
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Req {{ indx: {}, tag: {} }}",
            self.index, self.tag
        )
    }
}
