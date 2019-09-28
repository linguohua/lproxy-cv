use super::Forwarder;
use failure::Error;
use fnv::FnvHashMap as HashMap;
use futures::sync::mpsc::UnboundedSender;
use log::{error, info};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::net::{UdpFramed, UdpSocket};
use tokio::prelude::*;
use tokio::runtime::current_thread;
use tokio_codec::BytesCodec;

type TxType = UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub type LongLive = Rc<RefCell<LocalResolver>>;
pub struct LocalResolver {
    listen_addr: String,
    tx: (Option<TxType>, Option<Trigger>),
    request_memo_history: HashMap<String, SocketAddr>,
    request_memo: HashMap<String, SocketAddr>,
    dns_server_addr: SocketAddr,
}

impl LocalResolver {
    pub fn new(local_dns_server:&str) -> LongLive {
        let dns_server_addr = local_dns_server.parse().unwrap();
        Rc::new(RefCell::new(LocalResolver {
            tx: (None, None),
            listen_addr: "0.0.0.0:0".to_string(),
            request_memo: HashMap::default(),
            request_memo_history: HashMap::default(),
            dns_server_addr,
        }))
    }

    pub fn start(&mut self, fw: &Forwarder, forward: Rc<RefCell<Forwarder>>) -> Result<(), Error> {
        let listen_addr = &self.listen_addr;
        let addr: SocketAddr = listen_addr.parse().map_err(|e| Error::from(e))?;
        let a = UdpSocket::bind(&addr).map_err(|e| Error::from(e))?;

        let (a_sink, a_stream) = UdpFramed::new(a, BytesCodec::new()).split();

        let forwarder1 = forward.clone();
        let forwarder2 = forward.clone();
        let (tx, rx) = futures::sync::mpsc::unbounded();
        let (trigger, tripwire) = Tripwire::new();

        self.set_tx(tx, trigger);
        fw.on_lresolver_udp_created(self, forward);

        // send future
        let send_fut = a_sink.send_all(rx.map_err(|e| {
            error!("[LocalResolver]sink send_all failed:{:?}", e);
            std::io::Error::from(std::io::ErrorKind::Other)
        }));

        let receive_fut = a_stream
            .take_until(tripwire)
            .for_each(move |(message, addr)| {
                let rf = forwarder1.borrow();
                // post to manager
                if rf.on_resolver_udp_msg(message, &addr) {
                    Ok(())
                } else {
                    Err(std::io::Error::from(std::io::ErrorKind::Other))
                }
            });

        // Wait for both futures to complete.
        let receive_fut = receive_fut
            .map_err(|_| ())
            .select(send_fut.map(|_| ()).map_err(|_| ()))
            .then(move |_| {
                info!("[LocalResolver] udp both future completed");
                let rf = forwarder2.borrow();
                rf.on_lresolver_udp_closed();

                Ok(())
            });

        current_thread::spawn(receive_fut);

        Ok(())
    }

    fn set_tx(&mut self, tx2: TxType, trigger: Trigger) {
        self.tx = (Some(tx2), Some(trigger));
    }

    pub fn get_tx(&self) -> Option<TxType> {
        let tx = &self.tx;
        let tx = &tx.0;
        match tx {
            Some(tx) => Some(tx.clone()),
            None => None,
        }
    }

    pub fn stop(&mut self) {
        self.tx = (None, None);
    }

    pub fn take(&mut self, qname: &str, identify: u16) -> Option<SocketAddr> {
        let key = format!("{}:{}", qname, identify);
        if self.request_memo_history.contains_key(&key) {
            return self.request_memo_history.remove(&key);
        } else {
            return self.request_memo.remove(&key);
        }
    }

    pub fn keepalive(&mut self) {
        self.request_memo_history.clear();
        for (k, v) in self.request_memo.drain() {
            self.request_memo_history.insert(k, v);
        }

        self.request_memo.clear();
    }

    pub fn request(&mut self, qname: &str, identify: u16, bm: bytes::Bytes, src_addr: SocketAddr) {
        match self.get_tx() {
            Some(tx) => {
                let key = format!("{}:{}", qname, identify);
                if self.request_memo.contains_key(&key) {
                    error!("[LocalResolver]request duplicate memo key:{}", key);
                }

                self.request_memo.insert(key, src_addr);
                match tx.unbounded_send((bm, self.dns_server_addr)) {
                    Err(e) => {
                        error!("[Forwarder]unbounded_senderror:{}", e);
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }
}
