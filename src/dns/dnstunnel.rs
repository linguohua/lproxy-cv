use crate::config::KEEP_ALIVE_INTERVAL;
use byte::*;
use crossbeam::queue::ArrayQueue;
use futures::sync::mpsc::UnboundedSender;
use log::{error, info};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::time::Instant;
use tungstenite::protocol::Message;

type TxType = UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub struct DnsTunnel {
    pub tx: UnboundedSender<Message>,
    pub index: usize,

    rtt_queue: ArrayQueue<i64>,
    rtt_sum: AtomicI64,

    ping_count: AtomicU8,
    time: Instant,

    udp_tx: Option<TxType>,
}

impl DnsTunnel {
    pub fn new(tx: UnboundedSender<Message>, udp_tx: Option<TxType>, idx: usize) -> DnsTunnel {
        info!("[DnsTunnel]new Tunnel, idx:{}", idx);
        let size = 5;
        let rtt_queue = ArrayQueue::new(size);
        for _ in 0..size {
            if let Err(e) = rtt_queue.push(0) {
                panic!("init rtt_queue failed:{}", e);
            }
        }

        DnsTunnel {
            tx: tx,
            index: idx,
            rtt_queue: rtt_queue,
            rtt_sum: AtomicI64::new(0),
            ping_count: AtomicU8::new(0),
            time: Instant::now(),
            udp_tx: udp_tx,
        }
    }

    pub fn on_tunnel_msg(&self, msg: Message) {
        let bs = msg.into_data();
        let len = bs.len();
        if len < 6 {
            error!("[Tunnel]pong data length({}) != 8", len);
            return;
        }

        match &self.udp_tx {
            Some(tx) => {
                let offset = &mut 0;
                let port = bs.read_with::<u16>(offset, LE).unwrap();
                let ip32 = bs.read_with::<u32>(offset, LE).unwrap();
                let content = &bs[6..];
                let sockaddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip32)), port);
                tx.unbounded_send((bytes::Bytes::from(content), sockaddr))
                    .unwrap();
            }
            None => {}
        }
    }

    pub fn on_closed(&self) {
        info!(
            "[DnsTunnel]tunnel live duration {} minutes",
            self.time.elapsed().as_secs() / 60
        );
    }

    pub fn send_ping(&self) -> bool {
        let ping_count = self.ping_count.load(Ordering::SeqCst) as i64;
        if ping_count > 10 {
            return true;
        }

        let timestamp = self.get_elapsed_milliseconds();
        let mut bs1 = vec![0 as u8; 8];
        let bs = &mut bs1[..];
        let offset = &mut 0;
        bs.write_with::<u64>(offset, timestamp, LE).unwrap();

        let msg = Message::Ping(bs1);
        let r = self.tx.unbounded_send(msg);
        match r {
            Err(e) => {
                error!("[DnsTunnel]tunnel send_ping error:{}", e);
            }
            _ => {
                self.ping_count.fetch_add(1, Ordering::SeqCst);
                if ping_count > 0 {
                    // TODO: fix accurate RTT?
                    self.append_rtt(ping_count * (KEEP_ALIVE_INTERVAL as i64));
                }
            }
        }

        true
    }

    fn get_elapsed_milliseconds(&self) -> u64 {
        let in_ms = self.time.elapsed().as_millis();
        in_ms as u64
    }

    fn append_rtt(&self, rtt: i64) {
        if let Ok(rtt_remove) = self.rtt_queue.pop() {
            if let Err(e) = self.rtt_queue.push(rtt) {
                panic!("[DnsTunnel]rtt_quque push failed:{}", e);
            }

            self.rtt_sum.fetch_add(rtt - rtt_remove, Ordering::SeqCst);
        }
    }

    pub fn get_rtt(&self) -> i64 {
        let rtt_sum = self.rtt_sum.load(Ordering::SeqCst);
        rtt_sum / (self.rtt_queue.len() as i64)
    }
}
