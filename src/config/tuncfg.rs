#[derive(Debug)]
pub struct TunCfg {
    pub tunnel_number: usize,
    pub websocket_url: String,
    pub local_server: String,
}

impl TunCfg {
    pub fn new() -> TunCfg {
        TunCfg {
            tunnel_number: 2,
            websocket_url: "wss://localhost:8000/tun".to_string(),
            local_server: "127.0.0.1:5000".to_string(),
        }
    }
}
