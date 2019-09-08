#[derive(Debug)]
pub struct TunCfg {
    pub number: usize,
    pub url: String,
}

impl TunCfg {
    pub fn new() -> TunCfg {
        TunCfg {
            number: 2,
            url: "wss://localhost:8000/tun".to_string(),
        }
    }
}
