use byteorder::ByteOrder;
use byteorder::NativeEndian;

use libc;
use std::io::Error;
use std::mem;
use std::os::unix::io::RawFd;

const NLM_F_REQUEST: u16 = 1;
const IPSET_CMD_ADD: u16 = 9;
const NFNL_SUBSYS_IPSET: u16 = 6;
const IPSET_PROTOCOL: u8 = 6;
const IPSET_ATTR_PROTOCOL: u16 = 1;
const IPSET_ATTR_SETNAME: u16 = 2;
const NLA_F_NESTED: u16 = (1 << 15);
const IPSET_ATTR_DATA: u16 = 7;
const IPSET_ATTR_IPADDR_IPV4: u16 = 1;
const IPSET_ATTR_IPADDR_IPV6: u16 = 2;
const IPSET_ATTR_IP: u16 = 1;
const NLA_F_NET_BYTEORDER: u16 = (1 << 14);

struct NlMsgHdr {
    /// Length of the netlink packet, including the header and the payload
    pub length: u32,

    /// NetlinkMessage type. The meaning of this field depends on the netlink protocol family in use.
    pub message_type: u16, // (remove ? IPSET_CMD_DEL : IPSET_CMD_ADD) | (NFNL_SUBSYS_IPSET << 8);

    /// Flags
    pub flags: u16, // NLM_F_REQUEST

    /// Sequence number of the packet
    pub sequence_number: u32,

    /// Port number (usually set to the the process ID)
    pub port_number: u32,
}

fn align4(len: usize) -> u32 {
    ((len + 3) & (!3)) as u32
}

impl NlMsgHdr {
    pub fn new() -> NlMsgHdr {
        NlMsgHdr {
            length: 0,
            message_type: (IPSET_CMD_ADD | (NFNL_SUBSYS_IPSET << 8)),
            flags: NLM_F_REQUEST,
            sequence_number: 0,
            port_number: 0,
        }
    }

    pub fn align_size(&self) -> u32 {
        16
    }

    pub fn write_to(&self, buffer: &mut [u8]) {
        NativeEndian::write_u32(buffer, self.length);
        NativeEndian::write_u16(&mut buffer[4..], self.message_type);
        NativeEndian::write_u16(&mut buffer[6..], self.flags);
        NativeEndian::write_u32(&mut buffer[8..], self.sequence_number);
        NativeEndian::write_u32(&mut buffer[12..], self.port_number);
    }
}

struct NfGenMsg {
    nfgen_family: u8, // libc::AF_INET or libc::AF_INET6
    version: u8,      // NFNETLINK_V0
    res_id: u16,      // be, always zero
}

impl NfGenMsg {
    pub fn new(af: u8) -> NfGenMsg {
        NfGenMsg {
            nfgen_family: af,
            version: 0,
            res_id: 0,
        }
    }

    pub fn align_size(&self) -> u32 {
        align4(4)
    }

    pub fn write_to(&self, buffer: &mut [u8]) {
        buffer[0] = self.nfgen_family;
        buffer[1] = self.version;
        NativeEndian::write_u16(&mut buffer[2..], self.res_id);
    }
}

struct NlAttr {
    nla_len: u16,  // native endian
    nla_type: u16, // native endian
}

impl NlAttr {
    pub fn new_data(nla_type: u16, data_len: u16) -> NlAttr {
        let nla_len = (align4(4) as u16) + data_len;
        NlAttr { nla_len, nla_type }
    }

    pub fn align_size(&self) -> u32 {
        align4(self.nla_len as usize)
    }

    pub fn write_to(&self, buffer: &mut [u8]) {
        NativeEndian::write_u16(buffer, self.nla_len);
        NativeEndian::write_u16(&mut buffer[2..], self.nla_type);
    }
}

pub fn construct_ipset_packet(setname: &str, ipvec: &[u8], out: &mut [u8]) -> u32 {
    let mut hdr = NlMsgHdr::new();
    let nfg = NfGenMsg::new(libc::AF_INET as u8);
    let attr1 = NlAttr::new_data(IPSET_ATTR_PROTOCOL, 1);
    let proto: u8 = IPSET_PROTOCOL;
    let attr2 = NlAttr::new_data(IPSET_ATTR_SETNAME, setname.len() as u16);
    let iplen = ipvec.len();
    let ipaddr_type = if iplen > 4 {
        IPSET_ATTR_IPADDR_IPV6
    } else {
        IPSET_ATTR_IPADDR_IPV4
    };

    let attr5 = NlAttr::new_data(ipaddr_type | NLA_F_NET_BYTEORDER, iplen as u16);
    let attr4 = NlAttr::new_data(NLA_F_NESTED | IPSET_ATTR_IP, attr5.align_size() as u16);
    let attr3 = NlAttr::new_data(NLA_F_NESTED | IPSET_ATTR_DATA, attr4.align_size() as u16);

    hdr.length = hdr.align_size()
        + nfg.align_size()
        + attr1.align_size()
        + attr2.align_size()
        + attr3.align_size();

    let mut writen = 0;
    // write hdr
    hdr.write_to(out);
    writen += hdr.align_size();

    // write nfg
    nfg.write_to(&mut out[writen as usize..]);
    writen += nfg.align_size();

    // write attr1
    attr1.write_to(&mut out[writen as usize..]);
    // write proto
    out[(writen + align4(4)) as usize] = proto;
    writen += attr1.align_size();

    // write attr2
    attr2.write_to(&mut out[writen as usize..]);
    // write setname
    let str = &mut out[(writen + align4(4)) as usize..];
    let strb = setname.as_bytes();
    for i in 0..strb.len() {
        str[i] = strb[i];
    }
    writen += attr2.align_size();

    // write attr3
    attr3.write_to(&mut out[writen as usize..]);
    writen += align4(4);

    // write attr4
    attr4.write_to(&mut out[writen as usize..]);
    writen += align4(4);

    // write attr5
    attr5.write_to(&mut out[writen as usize..]);
    writen += align4(4);
    // write ipvec
    let str = &mut out[writen as usize..];
    for i in 0..ipvec.len() {
        str[i] = ipvec[i];
    }

    hdr.length
}

pub struct NLSocket {
    rawfd: RawFd,
    addr: libc::sockaddr_nl,
}

impl NLSocket {
    pub fn new() -> NLSocket {
        let rawfd = NLSocket::netlink_socket().unwrap();
        let mut addr: libc::sockaddr_nl = unsafe { mem::zeroed() };
        addr.nl_family = libc::PF_NETLINK as libc::sa_family_t;
        addr.nl_pid = 0;
        addr.nl_groups = 0;

        NLSocket { rawfd, addr }
    }

    fn netlink_socket() -> std::io::Result<RawFd> {
        let sock =
            unsafe { libc::socket(libc::PF_NETLINK, libc::SOCK_DGRAM, libc::NETLINK_NETFILTER) };
        if sock < 0 {
            return Err(Error::last_os_error());
        }

        // bind
        let mut addr: libc::sockaddr_nl = unsafe { mem::zeroed() };
        addr.nl_family = libc::PF_NETLINK as libc::sa_family_t;
        addr.nl_pid = 0;
        addr.nl_groups = 0;

        let addr_len = mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t;
        let addr_ptr = &addr as *const libc::sockaddr_nl as *const libc::sockaddr;
        let r = unsafe { libc::bind(sock, addr_ptr, addr_len) };
        if r < 0 {
            return Err(Error::last_os_error());
        }

        // non block
        // let mut non_blocking = true as libc::c_int;
        // let r = unsafe { libc::ioctl(sock, libc::FIONBIO, &mut non_blocking) };
        // if r < 0 {
        //     println!("non block error:{}", Error::last_os_error());
        // }

        Ok(sock)
    }

    pub fn send_to(&self, buf: &[u8], flags: libc::c_int) -> Result<usize, Error> {
        // kernel unicast address
        let addr = &self.addr;
        let addr_len = mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t;
        let addr_ptr = addr as *const libc::sockaddr_nl as *const libc::sockaddr;

        // buffer
        let buf_ptr = buf.as_ptr() as *const libc::c_void;
        let buf_len = buf.len() as libc::size_t;

        let res = unsafe { libc::sendto(self.rawfd, buf_ptr, buf_len, flags, addr_ptr, addr_len) };
        if res < 0 {
            return Err(Error::last_os_error());
        }

        Ok(res as usize)
    }
}

impl Drop for NLSocket {
    fn drop(&mut self) {
        unsafe { libc::close(self.rawfd) };
    }
}
