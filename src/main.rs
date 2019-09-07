mod auth;
mod config;
mod requests;
mod tunnels;

use futures::future::{loop_fn, Future, Loop};
use std::time::{Duration, Instant};
use tokio::timer::Delay;

use log::{error, trace};

use log::{Level, Metadata, Record};

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

use log::{LevelFilter, SetLoggerError};

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init() -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Debug))
}

fn main() {
    init().unwrap();
    trace!("try to start lproxy-cv server..");

    let req = auth::HTTPRequest::new("https://localhost:5000/auth").unwrap();

    let fut_loop = loop_fn(req, |req| {
        req.exec()
            .and_then(|response| {
                trace!("http response:{:?}", response);

                let cfg = config::TunCfg::new();
                let tunmgr = tunnels::TunMgr::new(cfg.number);
                let reqmgr = requests::ReqMgr::new(&tunmgr);

                tunmgr.init(&cfg);
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
                        trace!("retry...");
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
