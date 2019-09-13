use super::{ReqMgr, Request};
use log::{error, info};
use nix::sys::socket::getsockname; // getsockopt
use nix::sys::socket::setsockopt;
use nix::sys::socket::sockopt::IpTransparent;
use nix::sys::socket::InetAddr::V4;
use nix::sys::socket::SockAddr::Inet;
use nix::sys::socket::{shutdown, Shutdown};
use std::io::{Error, ErrorKind};
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::Duration;
use stream_cancel::{StreamExt, Tripwire};
use tokio;
use tokio::codec::Decoder;
use tokio::prelude::*;
use tokio_codec::BytesCodec;
use tokio_io_timeout::TimeoutStream;
use tokio_tcp::TcpListener;
use tokio_tcp::TcpStream;

pub struct Server {
    listen_addr: String,
}

impl Server {
    pub fn new(addr: &str) -> Arc<Server> {
        info!("[Server]new server, add:{}", addr);
        Arc::new(Server {
            listen_addr: addr.to_string(),
        })
    }

    pub fn start(self: Arc<Server>, mgr: &Arc<ReqMgr>) {
        let mgr = mgr.clone();
        info!("[Server]start local server at:{}", self.listen_addr);

        // start tcp server listen at addr
        // Bind the server's socket.
        let addr = &self.listen_addr;
        let addr_inet = addr.parse().unwrap();
        let listener = TcpListener::bind(&addr_inet).expect("unable to bind TCP listener");
        let rawfd = listener.as_raw_fd();
        info!("[Server]listener rawfd:{}", rawfd);
        // enable linux TPROXY
        let enabled = true;
        setsockopt(rawfd, IpTransparent, &enabled).unwrap();

        let server = listener
            .incoming()
            .map_err(|e| error!("[Server] accept failed = {:?}", e))
            .for_each(move |sock| {
                // service new socket
                let mgr = mgr.clone();
                Server::serve_sock(sock, mgr);

                Ok(())
            });

        // Start the Tokio runtime
        tokio::spawn(server);
    }

    pub fn serve_sock(socket: TcpStream, mgr: Arc<ReqMgr>) {
        // config tcp stream
        socket.set_linger(None).unwrap();
        let kduration = Duration::new(3, 0);
        socket.set_keepalive(Some(kduration)).unwrap();
        socket.set_nodelay(true).unwrap();

        // get real dst address
        let rawfd = socket.as_raw_fd();
        //let result = getsockopt(rawfd, OriginalDst).unwrap();
        let result = getsockname(rawfd).unwrap();

        let mut ip_le = 0; // result.sin_addr.s_addr;
        let mut port_le = 0; // result.sin_port.to_be();
        match result {
            Inet(iaddr) => match iaddr {
                V4(v) => {
                    ip_le = v.sin_addr.s_addr;
                    port_le = v.sin_port.to_be();
                }
                _ => {}
            },
            _ => {}
        }

        let ipaddr = std::net::Ipv4Addr::from(ip_le.to_be()); // ip_le.to_be()
        info!("[Server]serve_sock, ip:{}, port:{}", ipaddr, port_le);

        // set 2 seconds write-timeout
        let mut socket = TimeoutStream::new(socket);
        let wduration = Duration::new(2, 0);
        socket.set_write_timeout(Some(wduration));

        let framed = BytesCodec::new().framed(socket);
        let (sink, stream) = framed.split();
        let (trigger, tripwire) = Tripwire::new();
        let (tx, rx) = futures::sync::mpsc::unbounded();

        let req = Request::with(tx, trigger, ip_le, port_le);
        let tunstub = mgr.on_request_created(req);

        if tunstub.is_none() {
            // invalid tunnel
            error!("[Server]failed to alloc tunnel for request!");
            return;
        }

        info!("[Server]allocated tun:{:?}", tunstub);
        let tunstub = Arc::new(tunstub.unwrap());
        let req_idx = tunstub.req_idx;

        // send future
        let send_fut = sink.send_all(rx.map_err(|e| {
            error!("[Server]sink send_all failed:{:?}", e);
            Error::new(ErrorKind::Other, "")
        }));

        let send_fut = send_fut.and_then(move |_| {
            info!("[Server]send_fut end, index:{}", req_idx);
            // shutdown read direction
            if let Err(e) = shutdown(rawfd, Shutdown::Read) {
                error!("[Server]shutdown rawfd error:{}", e);
            }

            Ok(())
        });

        let tunstub1 = tunstub.clone();

        // when peer send finished(FIN), then for-each(recv) future end
        // and wait rx(send) futue end, when the server indicate that
        // no more data, rx's pair tx will be drop, then mpsc end, rx
        // future will end, thus both futures are ended;
        // when peer total closed(RST), then both futures will end
        let receive_fut = stream.take_until(tripwire).for_each(move |message| {
            // post to manager
            if ReqMgr::on_request_msg(message, &tunstub) {
                Ok(())
            } else {
                Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
            }
        });

        let tunstub2 = tunstub1.clone();
        let receive_fut = receive_fut.and_then(move |_| {
            // client(of request) send finished(FIN), indicate that
            // no more data to send
            ReqMgr::on_request_recv_finished(&tunstub1);

            Ok(())
        });

        // Wait for both futures to complete.
        let receive_fut = receive_fut
            .map_err(|_| ())
            .join(send_fut.map_err(|_| ()))
            .then(move |_| {
                mgr.on_request_closed(&tunstub2);

                Ok(())
            });

        tokio::spawn(receive_fut);
    }
}
