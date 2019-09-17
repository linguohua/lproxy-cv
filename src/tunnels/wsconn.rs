use futures::Future;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use tokio_tcp::TcpStream;
use tokio_tungstenite::client_async_tls;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tungstenite::handshake::client::Request;
use tungstenite::Error;
use url::Url;

/// Connect to a given URL.
pub fn ws_connect_async(
    relay_domain: &str,
    relay_port: u16,
    ws_url: Url,
) -> Box<dyn Future<Item = (WebSocketStream<MaybeTlsStream<TcpStream>>, RawFd), Error = Error> + Send>
{
    // let port = request.url.port_or_known_default().expect("Bug: port unknown");
    let request = Request::from(ws_url);

    Box::new(
        tokio_dns::TcpStream::connect((relay_domain, relay_port))
            .map_err(|e| e.into())
            .and_then(move |socket| {
                let rawfd = socket.as_raw_fd();
                client_async_tls(request, socket).and_then(move |(ws, _)| Ok((ws, rawfd)))
            }),
    )
}
