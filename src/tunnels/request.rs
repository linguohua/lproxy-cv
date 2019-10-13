use crate::lws::WMessage;
use futures::sync::mpsc::UnboundedSender;
use std::fmt;
use stream_cancel::Trigger;

pub struct Request {
    pub index: u16,
    pub tag: u16,

    pub request_tx: Option<UnboundedSender<WMessage>>,
    pub trigger: Option<Trigger>,

    pub write_out: u16,
}

impl Request {
    pub fn new(idx: u16) -> Request {
        Request {
            index: idx,
            tag: 0,
            request_tx: None,
            trigger: None,
            write_out: 0,
        }
    }

    pub fn with(tx: UnboundedSender<WMessage>, trigger: Trigger) -> Request {
        Request {
            index: 0,
            tag: 0,
            request_tx: Some(tx),
            trigger: Some(trigger),
            write_out: 0,
        }
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Req {{ indx: {}, tag: {}}}", self.index, self.tag,)
    }
}
