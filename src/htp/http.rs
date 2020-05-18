use crate::dns;
use failure::Error;
use futures_03::future::Either;
use futures_03::Future;
use native_tls::TlsConnector;
use std::io;
use std::time::Duration;
use tokio::prelude::*;
use url::Url;

#[derive(Debug)]
pub struct HTTPRequest {
    url_parsed: Url,
    timeout: Duration,
}

#[derive(Debug)]
pub struct HTTPResponse {
    pub status: i32,
    pub header: Option<String>,
    pub body: Option<Vec<u8>>,
}

impl HTTPRequest {
    pub fn new(url: &str, timeout2: Option<Duration>) -> Result<HTTPRequest, Error> {
        let urlparsed = Url::parse(url)?;
        let timeout;
        if timeout2.is_some() {
            timeout = timeout2.unwrap();
        } else {
            timeout = Duration::from_secs(10000000000);
        }

        Ok(HTTPRequest {
            url_parsed: urlparsed,
            timeout,
        })
    }

    pub async fn exec(&self, body: Option<Vec<u8>>) -> Result<HTTPResponse, Error> {
        if self.is_secure() {
            self.https_exec(body).await
        } else {
            self.http_exec(body).await
        }
    }

    async  fn http_exec(&self, body: Option<Vec<u8>>) -> Result<HTTPResponse, Error> {
        let urlparsed = &self.url_parsed;
        let host = urlparsed.host_str().unwrap();
        let port = urlparsed.port_or_known_default().unwrap();

        let content_size;
        if let Some(ref s) = body {
            content_size = s.len();
        } else {
            content_size = 0;
        }

        let head_str = self.to_http_header(content_size);
        let fut = dns::MyDns::new(host.to_string());
        let fut = fut.and_then(move |ipaddr| {
            let addr = std::net::SocketAddr::new(ipaddr, port);
            tokio::net::TcpStream::connect(&addr)
        });

        let fut = fut.map_err(|e| e.into()).and_then(move |socket| {
            let vec;
            if let Some(s) = body {
                vec = Vec::from([head_str.as_bytes(), &s[..]].concat());
            } else {
                vec = Vec::from(head_str.as_bytes());
            }

            let request = tokio::io::AsyncWriteExt::write_all(socket, &vec);
            let response =
                request.and_then(|(socket, _)| tokio::io::AsyncReadExt::read_to_end(socket, Vec::new()));

            let response = response
                .and_then(|arg| {
                    let resp = HTTPResponse::parse(&arg.1);
                    Ok(resp)
                })
                .map_err(|e| Error::from(e));

            response
        });

        fut.timeout(self.timeout).map_err(|err| {
            if err.is_elapsed() {
                failure::err_msg("request timeout")
            } else if let Some(inner) = err.into_inner() {
                inner
            } else {
                failure::err_msg("timer error")
            }
        })
    }

    async fn https_exec(&self, body: Option<Vec<u8>>) -> Result<HTTPResponse, Error> {
        let urlparsed = &self.url_parsed;
        let host = urlparsed.host_str().unwrap();
        let port = urlparsed.port_or_known_default().unwrap();

        let content_size;
        if let Some(ref s) = body {
            content_size = s.len();
        } else {
            content_size = 0;
        }

        let head_str = self.to_http_header(content_size);
        let cx = TlsConnector::builder().build().unwrap();
        let cx = tokio_tls::TlsConnector::from(cx);
        let host_str = host.to_string();

        let fut = dns::MyDns::new(host.to_string());
        let fut = fut.and_then(move |ipaddr| {
            let addr = std::net::SocketAddr::new(ipaddr, port);
            tokio::net::TcpStream::connect(&addr)
        });

        let fut = fut.map_err(|e| e.into()).and_then(move |socket| {
            let tls_handshake = cx
                .connect(&host_str, socket)
                .map_err(|e| io::Error::new(io::ErrorKind::Other, e));

            let request = tls_handshake.and_then(move |socket| {
                let vec;
                if let Some(s) = body {
                    vec = Vec::from([head_str.as_bytes(), &s[..]].concat());
                } else {
                    vec = Vec::from(head_str.as_bytes());
                }
                tokio::io::AsyncWriteExt::write_all(socket, &vec)
            });

            let response =
                request.and_then(|(socket, _)| tokio::io::AsyncReadExt::read_to_end(socket, Vec::new()));

            let response = response
                .and_then(|arg| {
                    let resp = HTTPResponse::parse(&arg.1);
                    Ok(resp)
                })
                .map_err(|e| Error::from(e));

            response
        });

        fut.timeout(self.timeout).map_err(|err| {
            if err.is_elapsed() {
                failure::err_msg("request timeout")
            } else if let Some(inner) = err.into_inner() {
                inner
            } else {
                failure::err_msg("timer error")
            }
        })
    }

    fn is_secure(&self) -> bool {
        self.url_parsed.scheme() == "https"
    }

    fn to_http_header(&self, content_size: usize) -> String {
        let urlparsed = &self.url_parsed;
        let method;
        if content_size > 0 {
            method = "POST";
        } else {
            method = "GET";
        }

        let path = urlparsed.path().to_string();
        let query = match urlparsed.query() {
            Some(q) => format!("?{}", q),
            None => "".to_string(),
        };

        let path = format!("{}{}", path, query);

        let h = format!(
            "\
             {} {} HTTP/1.0\r\n\
             Host: {}\r\n\
             Accept-Encoding: gzip\r\n\
             Content-Length: {} \r\n\
             Content-Type: application/json; charset=utf-8 \r\n\
             \r\n\
             ",
            method,
            path,
            urlparsed.host().unwrap(),
            content_size,
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
            let gzip = s.contains("gzip");

            header = Some(s);

            let pos = pos + 4;
            let bb = &bytes[pos..];
            let s2;

            if gzip {
                use flate2::read::GzDecoder;
                use std::io::prelude::*;
                let mut gz = GzDecoder::new(bb);
                let mut s3 = Vec::<u8>::new();
                // info!("http response is gzip");
                gz.read_to_end(&mut s3).unwrap();
                s2 = s3;
            } else {
                // s2 = String::from_utf8_lossy(bb).to_string();
                s2 = bb.to_vec();
            }

            body = Some(s2);
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
        match header_str.find("HTTP/1.") {
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
