use futures::sync::mpsc::UnboundedSender;
use tungstenite::protocol::Message;

pub struct North {
    pub tx: UnboundedSender<Message>,
    pub tun_idx: u16,
    pub req_idx: u16,
    pub req_tag: u16,
}
