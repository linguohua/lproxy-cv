use super::DnsTunnel;
use super::Forwarder;
use crate::tunnels::ws_connect_async;
use futures_03::prelude::*;
use log::{debug, error, info};
use nix::sys::socket::{shutdown, Shutdown};
use std::cell::RefCell;
use std::rc::Rc;
use tokio;
use tokio::sync::mpsc::UnboundedSender;
use url;

pub type TxType = UnboundedSender<(bytes::Bytes, std::net::SocketAddr)>;

pub fn connect(fw: &Forwarder, mgr2: Rc<RefCell<Forwarder>>, index: usize, udp_tx: TxType, dns_server:String) {
    let relay_domain = fw.relay_domain.to_string();
    let relay_port = fw.relay_port;
    let ws_url = &fw.dns_tun_url;
    let token = &fw.token;
    let ws_url = format!("{}?dns=1&tok={}", ws_url, token);
    let url = url::Url::parse(&ws_url).unwrap();

    let mgr1 = mgr2.clone();
    let mgr3 = mgr2.clone();
    let mgr4 = mgr2.clone();
    let mgr5 = mgr2.clone();

    // TODO: need to specify address and port
    let f_fut = async move {
        let (ws_stream, rawfd) = match ws_connect_async(&relay_domain, relay_port, url, dns_server).await {
            Ok((f, r)) => (f, r),
            Err(e) => {
                error!("[dnstunbuilder]ws_connect_async failed:{}", e);
                let mut rf = mgr4.borrow_mut();
                rf.on_tunnel_build_error(index);
                return;
            }
        };

        debug!("[dnstunbuilder]WebSocket handshake has been successfully completed");
        // let inner = ws_stream.get_inner().get_ref();

        // Create a channel for our stream, which other sockets will use to
        // send us messages. Then register our address with the stream to send
        // data to us.
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let t = Rc::new(RefCell::new(DnsTunnel::new(tx, rawfd, udp_tx, index)));
        {
            let mut rf = mgr1.borrow_mut();
            if let Err(_) = rf.on_tunnel_created(t.clone()) {
                // TODO: should return directly
                if let Err(e) = shutdown(rawfd, Shutdown::Both) {
                    error!("[dnstunbuilder]shutdown rawfd failed:{}", e);
                }
                return;
            }
        }

        // `sink` is the stream of messages going out.
        // `stream` is the stream of incoming messages.
        let (mut sink, mut stream) = ws_stream.split();
        let receive_fut = async move {
            while let Some(message) = stream.next().await {
                debug!("[dnstunbuilder]tunnel read a message");
                match message {
                    Ok(m) => {
                        // post to manager
                        let mut clone = t.borrow_mut();
                        clone.on_tunnel_msg(m, &mgr5.borrow());
                    }
                    Err(e) => {
                        error!("[dnstunbuilder] rx-shit:{}", e);
                        break;
                    }
                }
            }
        };

        let mut rxx = rx.map(move |x| Ok(x));
        let send_fut = sink.send_all(&mut rxx);

        // Wait for either of futures to complete.
        future::select(receive_fut.boxed_local(), send_fut).await;
        info!("[dnstunbuilder] both websocket futures completed");
        {
            let mut rf = mgr3.borrow_mut();
            rf.on_tunnel_closed(index);
        }

        // error!(
        //     "[dnstunbuilder]Error during the websocket handshake occurred: {}",
        //     e
        // );
        // let mut rf = mgr4.borrow_mut();
        // rf.on_tunnel_build_error(index);
    };
    tokio::task::spawn_local(f_fut);
}
