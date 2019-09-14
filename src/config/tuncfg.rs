#[derive(Debug)]
pub struct TunCfg {
    pub tunnel_number: usize,
    pub websocket_url: String,
    pub local_server: String,
    pub tunnel_req_cap: usize,
    pub relay_domain: String,
    pub relay_port: u16,
}

impl TunCfg {
    pub fn new() -> TunCfg {
        TunCfg {
            tunnel_number: 2,
            websocket_url: "wss://localhost/tun".to_string(),
            local_server: "127.0.0.1:5000".to_string(),
            tunnel_req_cap: 100,
            relay_domain:"127.0.0.1".to_string(),
            relay_port:8000,
        }
    }
}

pub fn server_url() -> String {
    "https://localhost:8000/auth".to_string()
}
