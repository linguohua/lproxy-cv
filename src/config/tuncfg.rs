#[derive(Debug)]
pub struct TunCfg {
    pub number: usize,
    pub url: String,
}

impl TunCfg{
    pub fn new() ->TunCfg {
        TunCfg {
            number:1,
            url: "wss://localhost:443/tun".to_string(),
        }
    }
}
