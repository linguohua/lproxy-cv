mod config;
mod dns;
mod htp;
mod lws;
mod requests;
mod service;
mod tunnels;
mod xport;
mod udpx;

use fs2::FileExt;
//use futures::stream::Stream;
//se futures::Future;
use log::{error, info};
use service::Service;
use tokio::signal::unix::{signal, SignalKind};
use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::process;
use tokio::runtime;

pub const PIDFILE: &'static str = "/var/run/lproxy-cv.pid";
pub const LOCKFILE: &'static str = "/var/run/lproxy-cv.lock";
pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const ARCH: &'static str = std::env::consts::ARCH;

fn to_version_number(ver: &str) -> usize {
    // split
    let splits = ver.split(".");
    let vec: Vec<&str> = splits.collect();
    let mut u: usize = 0;

    for s in vec.iter() {
        let uu: usize = s.parse().unwrap();
        u = (u << 8) | uu;
    }

    u
}

fn main() {
    config::log_init().unwrap();
    let args: Vec<String> = env::args().collect();
    // println!("{:?}", args);
    let mut uuid: String = String::default();
    if args.len() > 1 {
        let s = args.get(1).unwrap();
        if s == "-v" {
            println!("{}", VERSION);
            std::process::exit(0);
        } else if s == "-vn" {
            println!("{}", to_version_number(VERSION));
            std::process::exit(0);
        } else if s == "-u" {
            if args.len() > 2 {
                let uuidstr = args.get(2).unwrap();
                uuid = uuidstr.to_string();
            }
        }
    }

    if uuid == "" {
        println!("please specify the uuid with -u");
        std::process::exit(1);
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

    info!(
        "try to start lproxy-cv server, ver:{}, uuid: {}, arch:{}",
        VERSION, uuid, ARCH
    );

    let mut basic_rt = runtime::Builder::new()
    .basic_scheduler()
    .enable_all()
    .build().unwrap();
    // let handle = rt.handle();
    let local = tokio::task::LocalSet::new();

    let l = async move {
        let s = Service::new(uuid);
        s.borrow_mut().start(s.clone());

        let mut ss = signal(SignalKind::user_defined1()).unwrap();
        let sig = ss.recv().await;
        println!("got signal {:?}", sig);
            // Service::stop
        s.borrow_mut().stop();
    };

    local.spawn_local(l);
    basic_rt.block_on(local);

    if pidfile_holder.is_some() {
        match pidfile_holder.unwrap().set_len(0) {
            _ => {}
        }
    }
}
