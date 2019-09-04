use super::ReqMgr;
use std::sync::Arc;
use tokio;
use tokio::codec::Decoder;
use tokio::prelude::*;
use tokio_codec::BytesCodec;
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
        let framed = BytesCodec::new().framed(socket);
        let (sink, stream) = framed.split();

        // let amt = io::copy(reader, writer);
        let (tx, rx) = futures::sync::mpsc::unbounded();

        let send_fut = rx.fold(sink, |mut sink, msg| {
            sink.start_send(msg).unwrap();
            Ok(sink)
        });

        let north = mgr.on_request_created(&tx);
        let north1 = north.clone();
        let mgr1 = mgr.clone();

        let receive_fut = stream.for_each(move |message| {
            // post to manager
            mgr.on_request_msg(message, &north);

            Ok(())
        });

        // Wait for either of futures to complete.
        let receive_fut = receive_fut
            .map(|_| ())
            .map_err(|_| ())
            .select(send_fut.map(|_| ()).map_err(|_| ()))
            .then(move |_| {
                mgr1.on_request_closed(&north1);

                Ok(())
            });

        tokio::spawn(receive_fut);
    }
}
