use super::ReqMgr;
use failure::Error;
use log::{error, info};
use nix::sys::socket::setsockopt;
use nix::sys::socket::sockopt::IpTransparent;
use std::cell::RefCell;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::result::Result;
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio;
use tokio::net::TcpListener;

type LongLive = Rc<RefCell<Server>>;

pub struct Server {
    listen_addr: String,
    listen_port: u16,
    // TODO: log request count
    listener_trigger: Option<Trigger>,
}

impl Server {
    pub fn new(addr: &str, port: u16) -> LongLive {
        info!("[Server]new server, addr:{}", addr);
        Rc::new(RefCell::new(Server {
            listen_addr: addr.to_string(),
            listener_trigger: None,
            listen_port: port,
        }))
    }

    pub fn start(&mut self, mgr: Rc<RefCell<ReqMgr>>) -> Result<(), Error> {
        info!("[Server]start local server at:{}", self.listen_addr);

        // start tcp server listen at addr
        // Bind the server's socket.
        let addr = &self.listen_addr;
        let addr_inet = addr.parse().map_err(|e| Error::from(e))?;
        let socket_addr = std::net::SocketAddr::new(addr_inet, self.listen_port);
        let listener = TcpListener::bind(&socket_addr).map_err(|e| Error::from(e))?;

        let rawfd = listener.as_raw_fd();
        info!("[Server]listener rawfd:{}", rawfd);
        // enable linux TPROXY
        let enabled = true;
        setsockopt(rawfd, IpTransparent, &enabled).map_err(|e| Error::from(e))?;

        let (trigger, tripwire) = Tripwire::new();
        self.save_listener_trigger(trigger);

        let server = listener
            .incoming()
            .map_err(|e| error!("[Server] accept failed = {:?}", e))
            .take_until(tripwire)
            .for_each(move |sock| {
                // service new socket
                // let mgr = mgr.clone();
                mgr.borrow_mut().on_accept_tcpstream(sock);
                Ok(())
            })
            .then(|_| {
                info!("[Server]listening future completed");
                Ok(())
            });

        // Start the Tokio runtime
        tokio::task::spawn_local(server);

        Ok(())
    }

    fn save_listener_trigger(&mut self, trigger: Trigger) {
        self.listener_trigger = Some(trigger);
    }

    pub fn stop(&mut self) {
        self.listener_trigger = None;
    }
}
