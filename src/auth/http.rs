use failure::Error;
use futures::future::Either;
use futures::Future;
use native_tls::TlsConnector;
use std::io;
use url::Url;

#[derive(Debug)]
pub struct HTTPRequest {
    url_parsed: Url,
}

#[derive(Debug)]
pub struct HTTPResponse {
    status: i32,
    header: Option<String>,
    body: Option<String>,
}

impl HTTPRequest {
    pub fn new(url: &str) -> Result<HTTPRequest, Error> {
        let urlparsed = Url::parse(url)?;
        Ok(HTTPRequest {
            url_parsed: urlparsed,
        })
    }

    pub fn exec(&self) -> impl Future<Item = HTTPResponse, Error = Error> {
        if self.is_secure() {
            Either::A(self.https_exec())
        } else {
            Either::B(self.http_exec())
        }
    }

    fn http_exec(&self) -> impl Future<Item = HTTPResponse, Error = Error> {
        let urlparsed = &self.url_parsed;
        let host = urlparsed.host_str().unwrap();

        let port = urlparsed.port_or_known_default().unwrap();

        let head_str = self.to_http_header();

        let fut = tokio_dns::TcpStream::connect((host, port))
            .map_err(|e| e.into())
            .and_then(move |socket| {
                let vec = Vec::from(head_str.as_bytes());
                let request = tokio_io::io::write_all(socket, vec);
                let response =
                    request.and_then(|(socket, _)| tokio_io::io::read_to_end(socket, Vec::new()));

                let response = response
                    .and_then(|arg| {
                        let resp = HTTPResponse::parse(&arg.1);
                        Ok(resp)
                    })
                    .map_err(|e| Error::from(e));

                response
            });

        fut
    }

    fn https_exec(&self) -> impl Future<Item = HTTPResponse, Error = Error> {
        let urlparsed = &self.url_parsed;
        let host = urlparsed.host_str().unwrap();

        let port = urlparsed.port_or_known_default().unwrap();

        let head_str = self.to_http_header();
        let cx = TlsConnector::builder().build().unwrap();
        let cx = tokio_tls::TlsConnector::from(cx);
        let host_str = host.to_string();

        let fut = tokio_dns::TcpStream::connect((host, port))
            .map_err(|e| e.into())
            .and_then(move |socket| {
                let tls_handshake = cx
                    .connect(&host_str, socket)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e));

                let request = tls_handshake.and_then(move |socket| {
                    let vec = Vec::from(head_str.as_bytes());
                    tokio_io::io::write_all(socket, vec)
                });

                let response =
                    request.and_then(|(socket, _)| tokio_io::io::read_to_end(socket, Vec::new()));

                let response = response
                    .and_then(|arg| {
                        let resp = HTTPResponse::parse(&arg.1);
                        Ok(resp)
                    })
                    .map_err(|e| Error::from(e));

                response
            });

        fut
    }

    fn is_secure(&self) -> bool {
        self.url_parsed.scheme() == "https"
    }

    fn to_http_header(&self) -> String {
        let urlparsed = &self.url_parsed;

        let h = format!(
            "\
             GET {} HTTP/1.0\r\n\
             Host: {}\r\n\
             \r\n\
             ",
            urlparsed.path(),
            urlparsed.host().unwrap()
        );

        h.to_string()
    }
}

impl HTTPResponse {
    fn parse(bytes: &Vec<u8>) -> HTTPResponse {
        // println!("{}", String::from_utf8_lossy(bytes));
        let status;
        let body;
        let header;

        let needle = "\r\n\r\n".as_bytes();
        if let Some(pos) = bytes.windows(4).position(|window| window == needle) {
            //let head_bytes = &bytes[..pos];
            let s = String::from_utf8_lossy(&bytes[..pos]).to_string();
            status = HTTPResponse::extract_status(&s);
            header = Some(s);

            let pos = pos + 4;
            let s = String::from_utf8_lossy(&bytes[pos..]).to_string();
            body = Some(s);
        } else {
            status = 0;
            body = None;
            header = None;
        }

        HTTPResponse {
            status: status,
            header: header,
            body: body,
        }
    }

    fn extract_status(header_str: &str) -> i32 {
        match header_str.find("HTTP/1.0 ") {
            Some(pos1) => {
                let pos1 = pos1 + 9;
                let next = &header_str[pos1..];
                match next.find(" OK") {
                    Some(pos2) => {
                        let pos2 = pos1 + pos2;
                        let status_str = &header_str[pos1..pos2];
                        if let Ok(i) = status_str.parse() {
                            return i;
                        }
                    }
                    None => {}
                }
            }
            None => {}
        }
        0
    }
}

// fn url_to_sockaddr(urlparsed: &Url) -> Result<std::net::SocketAddr, Error> {
//     let host = urlparsed
//         .host_str()
//         .ok_or(err_msg("no host found in url"))?;
//     let port = urlparsed
//         .port_or_known_default()
//         .ok_or(err_msg("no port found in url"))?;

//     let hostport_str = format!("{}:{}", host, port);
//     let addr = hostport_str
//         .to_socket_addrs()?
//         .next()
//         .ok_or(err_msg(format!("failed to do dns for host:{}", host)))?;

//     Ok(addr)
// }
