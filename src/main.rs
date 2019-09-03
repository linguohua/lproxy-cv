mod config;
mod tunnels;
mod requests;

fn main() {
    let cfg = config::TunCfg::new();
    let tunmgr = tunnels::TunMgr::new(cfg.number);
    tunmgr.init(&cfg);

    let reqmgr = requests::ReqMgr::new();
    reqmgr.init();

    println!("Hello, world!");
}
