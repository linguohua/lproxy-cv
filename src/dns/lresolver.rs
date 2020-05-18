use super::Forwarder;
use failure::Error;
use fnv::FnvHashMap as HashMap;
use log::{error, info};
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;

use tokio::net::{UdpSocket};
use futures_03::prelude::*;
use stream_cancel::{Trigger, Tripwire};
use tokio::sync::mpsc;

type TxType = mpsc::UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub type LongLive = Rc<RefCell<LocalResolver>>;
pub struct LocalResolver {
    listen_addr: String,
    tx: (Option<TxType>, Option<Trigger>),
    request_memo_history: HashMap<String, SocketAddr>,
    request_memo: HashMap<String, SocketAddr>,
    dns_server_addr: SocketAddr,
    fw: Option<Rc<RefCell<Forwarder>>>,
}

impl LocalResolver {
    pub fn new(local_dns_server: &str) -> LongLive {
        let dns_server_addr = local_dns_server.parse().unwrap();
        Rc::new(RefCell::new(LocalResolver {
            tx: (None, None),
            listen_addr: "0.0.0.0:0".to_string(),
            request_memo: HashMap::default(),
            request_memo_history: HashMap::default(),
            dns_server_addr,
            fw: None,
        }))
    }

    pub fn start(&mut self, fw: &Forwarder, forward: Rc<RefCell<Forwarder>>) -> Result<(), Error> {
        let listen_addr = &self.listen_addr;
        let addr: SocketAddr = listen_addr.parse().map_err(|e| Error::from(e))?;
        let socket_udp = std::net::UdpSocket::bind(addr)?;
        let a = UdpSocket::from_std(socket_udp)?;

        let udp_framed = tokio_util::udp::UdpFramed::new(a, tokio_util::codec::BytesCodec::new());
        let (a_sink, a_stream) = udp_framed.split();

        let forwarder1 = forward.clone();
        let forwarder2 = forward.clone();
        let (tx, rx) = mpsc::unbounded_channel();
        let (trigger, tripwire) = Tripwire::new();

        self.fw = Some(forward.clone());
        self.set_tx(tx, trigger);
        fw.on_lresolver_udp_created(self, forward);

        let send_fut = rx.map(move |x|{Ok(x)}).forward(a_sink);

        let receive_fut = a_stream
            .take_until(tripwire)
            .for_each(move |rr| {
               match rr {
                    Ok((message, addr)) => {
                        let rf = forwarder1.borrow();
                        // post to manager
                        if rf.on_resolver_udp_msg(message, &addr) {
                        } else {
                            error!("[LocalResolver] on_resolver_udp_msg failed");
                        }
                    },
                    Err(e) => error!("[LocalResolver] for_each failed:{}", e)
                };

                future::ready(())
            });

        // Wait for one future to complete.
        let select_fut = async move {
            future::select(receive_fut, send_fut).await;
            info!("[LocalResolver] udp both future completed");
            let mut rf = forwarder2.borrow_mut();
            rf.on_lresolver_udp_closed();
        };

        tokio::task::spawn_local(select_fut);

        Ok(())
    }

    fn try_rebuild(&mut self, fw: &Forwarder) -> Result<(), Error> {
        let rf = self.fw.take().unwrap();
        let result = self.start(fw, rf)?;
        Ok(result)
    }

    fn set_tx(&mut self, tx2: TxType, trigger: Trigger) {
        self.tx = (Some(tx2), Some(trigger));
    }

    pub fn invalid_tx(&mut self) {
        self.tx = (None, None);
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
        self.fw = None;
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

    pub fn request(
        &mut self,
        fw: &Forwarder,
        qname: &str,
        identify: u16,
        bm: bytes::Bytes,
        src_addr: SocketAddr,
    ) {
        // check tx is valid or not
        if self.get_tx().is_none() {
            if self.fw.is_none() {
                error!("[LocalResolver]request failed, try_rebuild error, fw is none");
                return;
            }

            // need rebuild upd client
            match self.try_rebuild(fw) {
                Err(err) => {
                    error!("[LocalResolver]request failed, try_rebuild error:{}", err);
                    return;
                }
                _ => {}
            }
        }

        match self.get_tx() {
            Some(tx) => {
                let key = format!("{}:{}", qname, identify);
                if self.request_memo.contains_key(&key) {
                    error!("[LocalResolver]request duplicate memo key:{}", key);
                }

                self.request_memo.insert(key, src_addr);
                match tx.send((bm, self.dns_server_addr)) {
                    Err(e) => {
                        error!("[LocalResolver]unbounded_send error:{}", e);
                    }
                    _ => {}
                }
            }
            None => {}
        }
    }
}
