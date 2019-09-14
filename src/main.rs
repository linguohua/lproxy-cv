mod auth;
mod config;
mod requests;
mod tunnels;
use futures::future::{loop_fn, Future, Loop};
use log::{debug, error};
use std::time::{Duration, Instant};
use tokio::timer::Delay;

fn main() {
    config::log_init().unwrap();
    debug!("try to start lproxy-cv server..");

    let httpserver = config::server_url();
    let req = auth::HTTPRequest::new(&httpserver).unwrap();

    let fut_loop = loop_fn(req, |req| {
        req.exec()
            .and_then(move |response| {
                debug!("http response:{:?}", response);

                // TODO: use reponse to init TunCfg
                let cfg = config::TunCfg::new();
                let tunmgr = tunnels::TunMgr::new(&cfg);
                let reqmgr = requests::ReqMgr::new(&tunmgr, &cfg);

                tunmgr.init();
                reqmgr.init();

                Ok(true)
            })
            .or_else(|e| {
                let seconds = 5;
                error!(
                    "http request failed, error:{}, retry {} seconds later",
                    e, seconds
                );

                // delay 5 seconds
                let when = Instant::now() + Duration::from_millis(seconds * 1000);
                let task = Delay::new(when)
                    .and_then(|_| {
                        debug!("retry...");
                        Ok(false)
                    })
                    .map_err(|e| {
                        error!("delay retry errored, err={:?}", e);
                        ()
                    });
                task
            })
            .and_then(|done| {
                if done {
                    Ok(Loop::Break(()))
                } else {
                    Ok(Loop::Continue(req))
                }
            })
    });

    tokio::run(fut_loop);
}
