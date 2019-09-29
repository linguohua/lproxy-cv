use super::ip_rules::do_bash_cmd;
use crate::config::IPSET_TABLE;

pub fn set_ipset() {
    // sudo ipset -N LPROXY iphash
    let arg = format!("ipset -N {} iphash", IPSET_TABLE);
    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_ipset() {
    // sudo ipset -X LPROXY
    let arg = format!("ipset -X {}", IPSET_TABLE);
    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}
