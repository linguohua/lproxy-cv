mod auth;
mod config;
mod dns;
mod requests;
mod service;
mod tunnels;

use futures::future::lazy;
use futures::stream::Stream;
use futures::Future;
use log::{error, info};
use service::Service;
use signal_hook::iterator::Signals;
use std::env;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

fn main() {
    config::log_init().unwrap();
    let args: Vec<String> = env::args().collect();
    // println!("{:?}", args);
    if args.len() > 1 {
        let s = args.get(1).unwrap();
        if s == "-v" {
            println!("{}", VERSION);
            std::process::exit(0);
        }
    }

    info!("try to start lproxy-cv server..");

    let l = lazy(|| {
        // let (c, p) = oneshot::<bool>();
        let s = Service::new();
        let clone = s.clone();
        s.start();
        // listen signal

        // p.wait()
        //     .map(|v| {
        //         info!("oneshot wait return:{}", v);
        //         ()
        //     })
        //     .map_err(|e| {
        //         error!("wait failed:{}", e);
        //         ()
        //     })

        let wait_signal = Signals::new(&[signal_hook::SIGUSR1])
            .unwrap()
            .into_async()
            .unwrap()
            .into_future()
            .map(move |sig| {
                info!("got sigal:{:?}", sig.0);
                clone.stop();
                ()
            })
            .map_err(|e| error!("{}", e.0));

        wait_signal
    });

    tokio::run(l);
}
