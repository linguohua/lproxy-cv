use super::{BytePacketBuffer, DnsPacket, DnsQuestion, DnsRecord, QueryType};
use log::{error, info};
use std::net::IpAddr;
use std::net::UdpSocket;

pub fn query(qname: &str, dns_server: &str) -> Vec<IpAddr> {
    let qtype = QueryType::A;
    let server = (dns_server, 53);
    let mut result: Vec<IpAddr> = Vec::new();

    let socket = UdpSocket::bind(("0.0.0.0", 0));
    if socket.is_err() {
        error!("[DnsQuery]UdpSocket::bind failed, name:{}", qname);
        return result;
    }

    let socket = socket.unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap(); // should not failed

    let mut packet = DnsPacket::new();

    packet.header.id = 6666;
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet
        .questions
        .push(DnsQuestion::new(qname.to_string(), qtype));

    let mut reqbuff = vec![0 as u8; 512];
    let mut req_buffer = BytePacketBuffer::new(&mut reqbuff);
    packet.write(&mut req_buffer).unwrap(); // should not failed

    if let Err(e) = socket.send_to(&req_buffer.buf[0..req_buffer.pos], server) {
        error!("[DnsQuery]send to failed, name:{}, error:{}", qname, e);
        return result;
    }

    let mut rspbuff = vec![0 as u8; 512];
    let mut res_buffer = BytePacketBuffer::new(&mut rspbuff);

    if let Ok(_) = socket.recv_from(&mut res_buffer.buf) {
        let res_packet = DnsPacket::from_buffer(&mut res_buffer).unwrap();

        for a in res_packet.answers.iter() {
            match a {
                DnsRecord::A {
                    domain: _,
                    addr,
                    ttl: _,
                } => {
                    result.push(IpAddr::V4(*addr));
                }
                _ => {}
            }
        }
    }

    info!(
        "[DnsQuery]name:{}, server:{}, response:{:?}",
        qname, dns_server, result
    );

    result
}
