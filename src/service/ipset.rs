use super::ip_rules::do_bash_cmd;
use crate::config::{IPSET_TABLE, IPSET_TABLE6};

pub fn set_ipset() {
    // sudo ipset -N LPROXY iphash
    let arg = format!(
        "ipset -N {} iphash;ipset -N {} iphash family inet6",
        IPSET_TABLE, IPSET_TABLE6
    );
    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_ipset() {
    // sudo ipset -X LPROXY
    let arg = format!("ipset -X {};ipset -X {}", IPSET_TABLE, IPSET_TABLE6);
    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}
