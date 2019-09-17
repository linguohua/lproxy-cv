mod auth;
mod config;
mod dns;
mod requests;
mod tunnels;
mod service;

use futures::future::lazy;
use log::info;
use service::Service;

fn main() {
    config::log_init().unwrap();
    info!("try to start lproxy-cv server..");

    let l = lazy(|| {
        let s = Service::new();
        s.start();

        Ok(())
    });

    tokio::run(l);
}
