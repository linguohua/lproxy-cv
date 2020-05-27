use super::{BytePacketBuffer, DnsPacket, DnsQuestion, DnsRecord, QueryType};
use crate::config::DEFAULT_DNS_SERVER;
use futures_03::prelude::*;
use futures_03::task::{Context, Poll};
use log::{error, info};
use std::io::{Error, ErrorKind};
use std::net::IpAddr;
use std::pin::Pin;
use tokio::net::UdpSocket;

pub enum MyDnsState {
    Init,
    Send,
    Recv,
    Done,
}

// The query timeout should consider by use tokio timeout future
pub struct MyDns {
    domain: String,
    state: MyDnsState,
    result: Option<IpAddr>,
    udp: Option<UdpSocket>,
    send_buf: Option<Vec<u8>>,
    send_len: usize,
}

impl MyDns {
    pub fn new(domain: String) -> MyDns {
        MyDns {
            domain,
            state: MyDnsState::Init,
            result: None,
            udp: None,
            send_buf: None,
            send_len: 0,
        }
    }

    fn proc_rawip(&mut self) -> bool {
        let domain = &self.domain;
        let ipv4_result = domain.parse::<std::net::Ipv4Addr>();
        let is_raw_ip;
        match ipv4_result {
            Ok(ip) => {
                is_raw_ip = true;
                let ipaddr = IpAddr::V4(ip);
                self.result = Some(ipaddr);
            }
            _ => {
                is_raw_ip = false;
            }
        };

        is_raw_ip
    }

    fn prepare_query(&mut self) -> std::io::Result<()> {
        let mut packet = DnsPacket::new();

        let qtype = QueryType::A;
        packet.header.id = 6666;
        packet.header.questions = 1;
        packet.header.recursion_desired = true;
        packet
            .questions
            .push(DnsQuestion::new(self.domain.to_string(), qtype));

        let mut reqbuff = vec![0 as u8; 512];
        let mut req_buffer = BytePacketBuffer::new(&mut reqbuff);
        packet.write(&mut req_buffer).unwrap(); // should not failed

        let addr: std::net::SocketAddr = "0.0.0.0:0"
            .parse()
            .map_err(|_| Error::new(ErrorKind::Other, "bind addr parse error"))?;
        let u = std::net::UdpSocket::bind(addr)?;
        let a = UdpSocket::from_std(u)?;
        self.udp = Some(a);
        self.send_len = req_buffer.pos;
        self.send_buf = Some(reqbuff);

        Ok(())
    }

    fn parse_result(&mut self, rspbuf: &mut [u8]) -> std::io::Result<()> {
        let mut res_buffer = BytePacketBuffer::new(rspbuf);
        let res_packet = DnsPacket::from_buffer(&mut res_buffer)?; // should not failed
        for a in res_packet.answers.iter() {
            match a {
                DnsRecord::A {
                    domain: _,
                    addr,
                    ttl: _,
                } => {
                    self.result = Some(IpAddr::V4(*addr));
                    return Ok(());
                }
                _ => {}
            }
        }

        Err(Error::new(ErrorKind::Other, "DnsPacket has no A record"))
    }
}

impl Future for MyDns {
    type Output = std::result::Result<IpAddr, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let self_mut = self.get_mut();
        loop {
            match self_mut.state {
                MyDnsState::Init => {
                    if self_mut.proc_rawip() {
                        self_mut.state = MyDnsState::Done;
                        info!("[mydns] domain:{} is raw ip", self_mut.domain);
                    } else {
                        // send to udp socket
                        self_mut.prepare_query()?;
                        self_mut.state = MyDnsState::Send;
                    }
                }
                MyDnsState::Send => {
                    let target = format!("{}:53", DEFAULT_DNS_SERVER);
                    let target_addr: std::net::SocketAddr = target
                        .parse()
                        .map_err(|_| Error::new(ErrorKind::Other, "target addr parse error"))?;

                    let a = self_mut.udp.as_mut().unwrap(); // should not failed
                    let len = self_mut.send_len;
                    let buf = self_mut.send_buf.as_mut().unwrap(); // should not failed
                    let nr = a.poll_send_to(cx, &buf[0..len], &target_addr)?;
                    match nr {
                        Poll::Ready(n) => {
                            if n != len {
                                error!("[mydns] poll_send_to leak data");
                            }
                        }
                        Poll::Pending => {
                            return Poll::Pending;
                        }
                    }

                    self_mut.state = MyDnsState::Recv;
                }
                MyDnsState::Recv => {
                    // recv from udpsocket
                    let a = self_mut.udp.as_mut().unwrap(); // should not failed
                    let mut rspbuff = vec![0 as u8; 512];
                    let result = a.poll_recv_from(cx, &mut rspbuff[..])?;
                    match result {
                        Poll::Ready((n, _)) => {
                            // parse result
                            self_mut.parse_result(&mut rspbuff[..n])?;
                            info!(
                                "[mydns] domain:{} resolved to:{:?}",
                                self_mut.domain, self_mut.result
                            );
                            self_mut.state = MyDnsState::Done;
                        }
                        Poll::Pending => {
                            return Poll::Pending;
                        }
                    }
                }
                MyDnsState::Done => {
                    // discard
                    self_mut.send_buf = None;
                    self_mut.udp = None;

                    return Poll::Ready(Ok(self_mut.result.take().unwrap()));
                }
            }
        }
    }
}
