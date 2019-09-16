use crate::config::KEEP_ALIVE_INTERVAL;
use byte::*;
use crossbeam::queue::ArrayQueue;
use futures::sync::mpsc::UnboundedSender;
use log::{error, info};
use std::net::IpAddr::V4;
use std::net::IpAddr::V6;
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
                panic!("[DnsTunnel]init rtt_queue failed:{}", e);
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
        if msg.is_pong() {
            self.on_pong(msg);

            return;
        }

        let bs = msg.into_data();
        let len = bs.len();
        if len < 6 {
            error!("[DnsTunnel]on_tunnel_msg data length({}) != 8", len);
            return;
        }

        match &self.udp_tx {
            Some(tx) => {
                let offset = &mut 0;
                let port = bs.read_with::<u16>(offset, LE).unwrap();
                let ip32 = bs.read_with::<u32>(offset, LE).unwrap();
                info!("[DnsTunnel]on_tunnel_msg, port:{}, ip:{}", port, ip32);
                let content = &bs[6..];
                let sockaddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip32)), port);
                tx.unbounded_send((bytes::Bytes::from(content), sockaddr))
                    .unwrap(); // udp send should not failed!
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

    fn on_pong(&self, msg: Message) {
        let bs = msg.into_data();
        let len = bs.len();
        if len != 8 {
            error!("[DnsTunnel]pong data length({}) != 8", len);
            return;
        }

        // reset ping count
        self.ping_count.store(0, Ordering::SeqCst);

        let offset = &mut 0;
        let timestamp = bs.read_with::<u64>(offset, LE).unwrap();

        let in_ms = self.get_elapsed_milliseconds();
        assert!(in_ms >= timestamp, "[DnsTunnel]pong timestamp > now!");

        let rtt = in_ms - timestamp;
        let rtt = rtt as i64;
        self.append_rtt(rtt);
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

    pub fn on_dns_udp_msg(&self, message: &bytes::BytesMut, addr: &std::net::SocketAddr) {
        let port = addr.port();
        let mut ip: u32 = 0;
        match addr.ip() {
            V4(v4) => {
                ip = v4.into();
            }
            V6(_) => {
                // TODO: support v6
                error!("[DnsTunnel]ip6 address not supported yet");
            }
        }

        let size = message.len();
        let hsize = 6; // port + ipv4
        let buf = &mut vec![0; hsize + size];
        let header = &mut buf[0..hsize];
        let offset = &mut 0;
        info!("[DnsTunnel]on_dns_udp_msg, port:{}, ip:{}", port, ip);
        header.write_with::<u16>(offset, port, LE).unwrap();
        header.write_with::<u32>(offset, ip, LE).unwrap();

        let msg_body = &mut buf[hsize..];
        msg_body.copy_from_slice(message.as_ref());

        let wmsg = Message::from(&buf[..]);
        let tx = &self.tx;
        let result = tx.unbounded_send(wmsg);
        match result {
            Err(e) => {
                error!(
                    "[DnsTunnel]request tun send error:{}, tun_tx maybe closed",
                    e
                );
            }
            _ => info!("[DnsTunnel]unbounded_send request msg",),
        }
    }
}
