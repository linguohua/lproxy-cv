use futures::sync::mpsc::UnboundedSender;
use std::fmt;
use crate::lws::WMessage;

pub struct TunStub {
    pub tunnel_tx: UnboundedSender<WMessage>,
    pub tun_idx: u16,
    pub req_idx: u16,
    pub req_tag: u16,
    pub request_quota: u16,
}

impl fmt::Display for TunStub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TunStub {{ tun_idx: {}, req_idx: {}, req_tag:{} }}",
            self.tun_idx, self.req_idx, self.req_tag
        )
    }
}

impl fmt::Debug for TunStub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
