mod config;
mod requests;
mod tunnels;
use futures::future;

fn main() {
    let fut = future::lazy(|| {
        let cfg = config::TunCfg::new();
        let tunmgr = tunnels::TunMgr::new(cfg.number);
        let reqmgr = requests::ReqMgr::new(&tunmgr);

        tunmgr.init(&cfg);
        reqmgr.init();

        Ok(())
    });

    tokio::run(fut);
}
