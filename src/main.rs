mod auth;
mod config;
mod dns;
mod requests;
mod service;
mod tunnels;

use futures::future::lazy;
use log::info;
use service::Service;
use std::env;

fn main() {
    config::log_init().unwrap();
    let args: Vec<String> = env::args().collect();
    // println!("{:?}", args);
    if args.len() > 1 {
        let s = args.get(1).unwrap();
        if s == "-v" {
            println!("0.1.0");
            std::process::exit(0);
        }
    }

    info!("try to start lproxy-cv server..");

    let l = lazy(|| {
        let s = Service::new();
        s.start();

        Ok(())
    });

    tokio::run(l);
}
