use futures::sync::mpsc::UnboundedSender;
use std::fmt;
use tungstenite::protocol::Message;

pub struct TunStub {
    pub tunnel_tx: UnboundedSender<Message>,
    pub tun_idx: u16,
    pub req_idx: u16,
    pub req_tag: u16,
}

impl fmt::Debug for TunStub {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TunStub {{ tun_idx: {}, req_idx: {}, req_tag:{} }}",
            self.tun_idx, self.req_idx, self.req_tag
        )
    }
}
