use futures::{future, Future};
use log::error;
use native_tls::TlsConnector;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::RawFd;
use std::result::Result;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_tcp::TcpStream;
use tokio_tls::TlsConnector as TokioTlsConnector;
use tokio_tungstenite::stream::Stream as StreamSwitcher;
use tokio_tungstenite::{client_async_with_config, MaybeTlsStream, WebSocketStream};
use tungstenite::client::url_mode;
use tungstenite::handshake::client::{Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::stream::Mode;
use tungstenite::Error;
use url::Url;

fn wrap_stream<S>(
    socket: S,
    domain: String,
    mode: Mode,
) -> Box<dyn Future<Item = MaybeTlsStream<S>, Error = Error> + Send>
where
    S: 'static + AsyncRead + AsyncWrite + Send,
{
    match mode {
        Mode::Plain => Box::new(future::ok(StreamSwitcher::Plain(socket))),
        Mode::Tls => Box::new(
            future::result(TlsConnector::new())
                .map(TokioTlsConnector::from)
                .and_then(move |connector| connector.connect(&domain, socket))
                .map(|s| StreamSwitcher::Tls(s))
                .map_err(|e| Error::Tls(e)),
        ),
    }
}

fn domain(request: &Request) -> Result<String, Error> {
    match request.url.host_str() {
        Some(d) => Ok(d.to_string()),
        None => Err(Error::Url("no host name in the url".into())),
    }
}

/// Creates a WebSocket handshake from a request and a stream,
/// upgrading the stream to TLS if required.
fn my_client_async_tls<R, S>(
    request: R,
    ss: S,
    config: Option<WebSocketConfig>,
) -> Box<dyn Future<Item = (WebSocketStream<MaybeTlsStream<S>>, Response), Error = Error> + Send>
where
    R: Into<Request<'static>>,
    S: 'static + AsyncRead + AsyncWrite + Send,
{
    let request: Request = request.into();

    let domain = match domain(&request) {
        Ok(domain) => domain,
        Err(err) => return Box::new(future::err(err)),
    };

    // Make sure we check domain and mode first. URL must be valid.
    let mode = match url_mode(&request.url) {
        Ok(m) => m,
        Err(e) => return Box::new(future::err(e.into())),
    };

    let fut = wrap_stream(ss, domain, mode)
        .and_then(move |sss| client_async_with_config(request, sss, config));

    Box::new(fut)
}

/// Connect to a given URL.
pub fn ws_connect_async(
    relay_domain: &str,
    relay_port: u16,
    ws_url: Url,
    config: Option<WebSocketConfig>,
) -> Box<dyn Future<Item = (WebSocketStream<MaybeTlsStream<TcpStream>>, RawFd), Error = Error> + Send>
{
    // let port = request.url.port_or_known_default().expect("Bug: port unknown");
    let request = Request::from(ws_url);

    Box::new(
        tokio_dns::TcpStream::connect((relay_domain, relay_port))
            .map_err(|e| e.into())
            .and_then(move |socket| {
                if let Err(e) = socket.set_nodelay(true) {
                    error!("ws_connect_async, set_nodelay failed:{}", e);
                }

                let rawfd = socket.as_raw_fd();
                my_client_async_tls(request, socket, config)
                    .and_then(move |(ws, _)| Ok((ws, rawfd)))
            }),
    )
}
