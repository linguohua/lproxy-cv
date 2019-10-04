use log::{error, info};
use std::process::Command;

pub fn set_ip_rules() {
    unset_ip_rules();
    // sudo ip route add local 0.0.0.0/0 dev lo table 100
    // sudo ip rule add fwmark 1 table 100
    let arg = "ip route add local 0.0.0.0/0 dev lo table 100;\
               ip rule add fwmark 1 table 100";

    match do_bash_cmd(arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_ip_rules() {
    // sudo ip route del local 0.0.0.0/0 dev lo table 100
    // sudo ip rule del fwmark 1 table 100
    let arg = "ip route del local 0.0.0.0/0 dev lo table 100;\
               ip rule del fwmark 1 table 100";

    match do_bash_cmd(arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn do_bash_cmd(arg: &str) -> std::io::Result<bool> {
    match Command::new("sh").args(&["-c", arg]).output() {
        Ok(o) => {
            if o.status.success() {
                info!("bash [{}] ok", arg);
                return Ok(true);
            } else {
                info!(
                    "bash [{}] failed: {}",
                    arg,
                    String::from_utf8_lossy(&o.stderr)
                );

                return Ok(false);
            }
        }
        Err(e) => {
            error!("bash [{}] failed: {}", arg, e);
            return Err(e);
        }
    }
}

pub fn fileto_excecutable(filepath: &str) {
    let arg = format!("chmod a+x {}", filepath);

    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}
