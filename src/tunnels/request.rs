use bytes::Bytes;
use futures::sync::mpsc::UnboundedSender;
use stream_cancel::Trigger;
use std::fmt;

pub struct Request {
    pub index: u16,
    pub tag: u16,

    pub request_tx: Option<UnboundedSender<Bytes>>,
    pub trigger: Option<Trigger>,

    pub ipv4_be: u32,
    pub port_be: u16,
}

impl Request {
    pub fn new(idx:u16) -> Request {
        Request {
            index:idx,
            tag: 0,
            request_tx: None,
            trigger: None,
            ipv4_be: 0,
            port_be: 0,
        }
    }

    pub fn with(tx: UnboundedSender<Bytes>, trigger: Trigger, ip:u32, port:u16) -> Request {
        Request {
            index:0,
            tag: 0,
            request_tx: Some(tx),
            trigger: Some(trigger),
            ipv4_be:ip,
            port_be:port,
        }
    }
}

impl fmt::Debug for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Req {{ indx: {}, tag: {}, ip:{}, port:{} }}",
            self.index, self.tag, self.ipv4_be, self.port_be
        )
    }
}
