use super::ReqMgr;
use nix::sys::socket::getsockopt;
use nix::sys::socket::sockopt::OriginalDst;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use std::time::Duration;
use tokio;
use tokio::codec::Decoder;
use tokio::prelude::*;
use tokio_codec::BytesCodec;
use tokio_io_timeout::TimeoutStream;
use tokio_tcp::TcpListener;
use tokio_tcp::TcpStream;

pub struct Server {
    port: u16,
}

impl Server {
    pub fn new(port: u16) -> Arc<Server> {
        Arc::new(Server { port: port })
    }

    pub fn start(self: Arc<Server>, mgr: &Arc<ReqMgr>) {
        let mgr = mgr.clone();

        // start tcp server listen at addr
        // Bind the server's socket.
        let port = self.port;
        let addr = format!("127.0.0.1:{}", port);
        let addr_inet = addr.parse().unwrap();
        let listener = TcpListener::bind(&addr_inet).expect("unable to bind TCP listener");

        // Pull out a stream of sockets for incoming connections
        let server = listener
            .incoming()
            .map_err(|e| eprintln!("accept failed = {:?}", e))
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
        let result = getsockopt(rawfd, OriginalDst).unwrap();
        println!("serve_sock dst:{:?}", result);

        // set 2 seconds write-timeout
        let mut socket = TimeoutStream::new(socket);
        let wduration = Duration::new(2, 0);
        socket.set_write_timeout(Some(wduration));

        let framed = BytesCodec::new().framed(socket);
        let (sink, stream) = framed.split();

        let (tx, rx) = futures::sync::mpsc::unbounded();
        let tunstub = mgr.on_request_created(&tx, &result);
        if tunstub.is_none() {
            // invalid tunnel
            println!("failed to alloc tunnel for request!");
            return;
        }

        let send_fut = rx.fold(sink, |mut sink, msg| {
            let s = sink.start_send(msg);
            match s {
                Err(e) => {
                    println!("serve_sock, start_send error:{}", e);
                    Err(())
                }
                _ => Ok(sink),
            }
        });

        let tunstub = Arc::new(tunstub.unwrap());
        let north1 = tunstub.clone();
        let mgr1 = mgr.clone();

        // when peer send finished(FIN), then for-each(recv) future end
        // and wait rx(send) futue end, when the server indicate that
        // no more data, rx's pair tx will be drop, then mpsc end, rx
        // future will end, thus both futures are ended;
        // when peer total closed(RST), then both futures will end
        let receive_fut = stream.for_each(move |message| {
            // post to manager
            if mgr.on_request_msg(message, &tunstub) {
                Ok(())
            } else {
                Err(std::io::Error::from(std::io::ErrorKind::NotConnected))
            }
        });

        // Wait for both futures to complete.
        let receive_fut = receive_fut
            .map(|_| ())
            .map_err(|_| ())
            .join(send_fut.map(|_| ()).map_err(|_| ()))
            .then(move |_| {
                mgr1.on_request_closed(&north1);

                Ok(())
            });

        tokio::spawn(receive_fut);
    }
}
