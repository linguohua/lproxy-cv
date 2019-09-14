use url::Url;
use futures::Future;
use tokio_tcp::TcpStream;
use tokio_tungstenite::client_async_tls;
use tokio_tungstenite::MaybeTlsStream;
use tokio_tungstenite::WebSocketStream;
use tungstenite::handshake::client::Request;
use tungstenite::handshake::client::Response;
use tungstenite::Error;

/// Connect to a given URL.
pub fn ws_connect_async(
    relay_domain: &str,
    relay_port: u16,
    ws_url: Url,
) -> Box<
    dyn Future<Item = (WebSocketStream<MaybeTlsStream<TcpStream>>, Response), Error = Error> + Send,
> {
    // let port = request.url.port_or_known_default().expect("Bug: port unknown");
    let request = Request::from(ws_url);

    Box::new(
        tokio_dns::TcpStream::connect((relay_domain, relay_port))
            .map_err(|e| e.into())
            .and_then(move |socket| client_async_tls(request, socket)),
    )
}
