use super::{BytePacketBuffer, DnsPacket, DnsQuestion, DnsRecord, QueryType};
use std::net::IpAddr;
use std::net::UdpSocket;
use log::info;

pub fn query(qname: &str, dns_server: &str) -> Vec<IpAddr> {
    let qtype = QueryType::A;
    let server = (dns_server, 53);

    let socket = UdpSocket::bind(("0.0.0.0", 43210)).unwrap();

    let mut packet = DnsPacket::new();

    packet.header.id = 6666;
    packet.header.questions = 1;
    packet.header.recursion_desired = true;
    packet
        .questions
        .push(DnsQuestion::new(qname.to_string(), qtype));

    let mut reqbuff = vec![0 as u8; 512];
    let mut req_buffer = BytePacketBuffer::new(&mut reqbuff);
    packet.write(&mut req_buffer).unwrap();
    socket
        .send_to(&req_buffer.buf[0..req_buffer.pos], server)
        .unwrap();

    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();

    let mut rspbuff = vec![0 as u8; 512];
    let mut res_buffer = BytePacketBuffer::new(&mut rspbuff);

    let mut result: Vec<IpAddr> = Vec::new();
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

    info!("[DnsQuery]name:{}, server:{}, response:{:?}", qname, dns_server, result);

    result
}
