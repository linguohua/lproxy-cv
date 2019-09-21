pub const KEEP_ALIVE_INTERVAL: u64 = 15 * 1000;
pub const CFG_MONITOR_INTERVAL: u64 = 30 * 60 * 1000;

use std::fmt;

#[derive(Debug)]
pub struct TunCfg {
    pub tunnel_number: usize,
    pub websocket_url: String,
    pub local_server: String,
    pub tunnel_req_cap: usize,
    pub relay_domain: String,
    pub relay_port: u16,

    pub dns_udp_addr: String,
    pub dns_tun_url: String,
    pub dns_tunnel_number: usize,
}

impl TunCfg {
    pub fn new() -> TunCfg {
        TunCfg {
            tunnel_number: 2,
            websocket_url: "wss://127.0.0.1/tun".to_string(),
            local_server: "127.0.0.1:5000".to_string(),
            tunnel_req_cap: 100,
            relay_domain: "127.0.0.1".to_string(),
            relay_port: 12345,

            dns_udp_addr: "127.0.0.1:5000".to_string(),
            dns_tun_url: "wss://127.0.0.1/dns".to_string(),
            dns_tunnel_number: 1,
        }
    }
}

pub fn server_url() -> String {
    "https://127.0.0.1:8000/auth".to_string()
}

pub struct AuthReq {
    pub uuid: String,
}

impl AuthReq {
    pub fn to_json_str(&self) -> String {
        format!("{{\"uuid\":\"{}\"}}", self.uuid)
    }
}

pub struct AuthResp {
    pub token: String,
}

impl AuthResp {
    pub fn from_json_str(s: &str) -> AuthResp {
        use serde_json::Value;
        let v: Value = serde_json::from_str(s).unwrap();
        AuthResp {
            token: v["token"].to_string(),
        }
    }
}

impl fmt::Display for AuthResp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ AuthResp token:{} }}", self.token)
    }
}
