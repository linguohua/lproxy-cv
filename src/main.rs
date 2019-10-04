mod config;
mod dns;
mod htp;
mod requests;
mod service;
mod tunnels;

use fs2::FileExt;
use futures::future::lazy;
use futures::stream::Stream;
use futures::Future;
use log::{error, info};
use service::Service;
use signal_hook::iterator::Signals;
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process;
use tokio::runtime::current_thread::Runtime;

pub const PIDFILE: &'static str = "/var/run/lproxy-cv.pid";
pub const LOCKFILE: &'static str = "/var/run/lproxy-cv.lock";
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

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

    let lockfile = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(LOCKFILE);

    match lockfile {
        Ok(ref lfile) => {
            if let Err(e) = lfile.try_lock_exclusive() {
                error!("lock pid file failed!:{}", e);
                return;
            }
        }
        Err(e) => {
            error!("create lock file failed!:{}", e);
            return;
        }
    }

    let pidfile = OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(PIDFILE);

    let pidfile_holder;

    match pidfile {
        Ok(mut pidf) => {
            let pid = process::id();
            let pid = &format!("{}", pid);
            match pidf.write_all(pid.as_bytes()) {
                Err(e) => {
                    error!("write pid file failed!:{}", e);
                    return;
                }
                _ => {
                    pidfile_holder = Some(pidf);
                }
            }
        }

        Err(e) => {
            error!("create pid file failed:{}", e);
            return;
        }
    }

    info!("try to start lproxy-cv server, ver:{}", VERSION);
    let mut rt = Runtime::new().unwrap();
    // let handle = rt.handle();

    let l = lazy(|| {
        let s = Service::new();
        s.borrow_mut().start(s.clone());

        let wait_signal = Signals::new(&[signal_hook::SIGUSR1])
            .unwrap()
            .into_async()
            .unwrap()
            .into_future()
            .map(move |sig| {
                info!("got sigal:{:?}", sig.0);
                // Service::stop
                s.borrow_mut().stop();
                ()
            })
            .map_err(|e| error!("{}", e.0));

        wait_signal
    });

    rt.spawn(l);
    rt.run().unwrap();

    if pidfile_holder.is_some() {
        match pidfile_holder.unwrap().set_len(0) {
            _ => {}
        }
    }
}
