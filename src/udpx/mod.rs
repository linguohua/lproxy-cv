mod udpserv;
pub use udpserv::*;
mod udpsock;
pub use udpsock::*;
mod cache;
pub use cache::*;
mod udpXmgr;
pub use udpXmgr::*;
mod ustub;
pub use ustub::*;

use std::io;
use std::os::unix::io::RawFd;
use std::net::SocketAddr;
use os_socketaddr::OsSocketAddr;

#[link(name="recvmsg", kind="static")]
extern {
    static udp_sas_IPV6_RECVORIGDSTADDR: libc::c_int;
    static udp_sas_IP_RECVORIGDSTADDR: libc::c_int;
    // https://docs.rs/crate/udp_sas/0.1.3/source/src/lib.rs
    fn udp_sas_recv(sock: libc::c_int, 
                 buf: *mut u8, buf_len: libc::size_t, flags: libc::c_int,
                 src: *mut libc::sockaddr, src_len: libc::socklen_t,
                 dst: *mut libc::sockaddr, dst_len: libc::socklen_t,
                 ) -> libc::ssize_t;
}

use self::udp_sas_IP_RECVORIGDSTADDR as IP_RECVORIGDSTADDR;
use self::udp_sas_IPV6_RECVORIGDSTADDR as IPV6_RECVORIGDSTADDR;

macro_rules! try_io {
    ($x:expr) => {
        match $x {
            -1 => {return Err(io::Error::last_os_error());},
            x  => x
            }}
}

fn getsockopt<T>(socket: RawFd, level: libc::c_int, name: libc::c_int, value: &mut T)
    -> io::Result<libc::socklen_t>
{
    unsafe {
        let mut len = std::mem::size_of::<T>() as libc::socklen_t;
        try_io!(libc::getsockopt(socket, level, name,
                                 value as *mut T as *mut libc::c_void,
                                 &mut len));
        Ok(len)
    }
}
fn setsockopt<T>(socket: RawFd, level: libc::c_int, name: libc::c_int, value: &T)
    -> io::Result<()>
{
    unsafe {
        try_io!(libc::setsockopt(socket, level, name,
                                 value as *const T as *const libc::c_void,
                                 std::mem::size_of::<T>() as libc::socklen_t));
        Ok(())
    }
}

pub fn set_ip_recv_origdstaddr (socket: RawFd) -> io::Result<()>
{
    unsafe {
        let mut domain = libc::c_int::default();
        getsockopt(socket, libc::SOL_SOCKET, libc::SO_DOMAIN, &mut domain)?;

        let (level, option) = match domain {
            libc::AF_INET  => (libc::IPPROTO_IP,   IP_RECVORIGDSTADDR),
            libc::AF_INET6 => (libc::IPPROTO_IPV6, IPV6_RECVORIGDSTADDR),
            _ => { return Err(io::Error::new(io::ErrorKind::Other, "not an inet socket")); }
        };

        setsockopt(socket, level, option, &(1 as libc::c_int))
    }
}

pub fn recv_sas(socket: RawFd, buf: &mut [u8])
    -> io::Result<(usize, Option<SocketAddr>, Option<SocketAddr>)>
{
    let mut src = OsSocketAddr::new();
    let mut dst = OsSocketAddr::new();
    
    let nb = {
        unsafe {udp_sas_recv(socket,
                             buf.as_mut_ptr(), buf.len(), 0,
                             src.as_mut_ptr(), src.capacity() as libc::socklen_t,
                             dst.as_mut_ptr(), dst.capacity() as libc::socklen_t,
                             )}
    };

    if nb < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok((nb as usize, src.into(), dst.into()))
    }
}
