use nix::sys::socket::setsockopt;
use nix::sys::socket::sockopt::IpTransparent; // OriginalDst
use nix::sys::socket::InetAddr;
use nix::sys::socket::{bind, listen, SockAddr};
use nix::sys::socket::{socket, AddressFamily, SockFlag, SockType};
use std::net::SocketAddr;
use std::os::unix::io::FromRawFd;

#[allow(dead_code)]
pub fn build_listener_sock(addr: &SocketAddr) -> std::net::TcpListener {
    let sock = socket(
        AddressFamily::Inet,
        SockType::Stream,
        SockFlag::SOCK_NONBLOCK,
        None,
    )
    .expect("unable to create sock");

    // set sock option
    let enabled = true;
    setsockopt(sock, IpTransparent, &enabled).expect("setsockopt IpTransparent failed");

    let addr = SockAddr::new_inet(InetAddr::from_std(addr));
    bind(sock, &addr).expect("unable to bind TCP listener");
    listen(sock, 5).expect("unable to call listen");

    unsafe { std::net::TcpListener::from_raw_fd(sock) }

    // let std_listener = build_listener_sock(&addr_inet);
    // let listener = TcpListener::from_std(std_listener, &Handle::default())
    //     .expect("tokio.TcpListener from_std failed");
    // Pull out a stream of sockets for incoming connections
}
