use super::ip_rules::do_bash_cmd;

pub fn set_ipset() {
    // sudo ipset -N LPROXY iphash
    let arg = "ipset -N LPROXY iphash";
    match do_bash_cmd(arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_ipset() {
    // sudo ipset -X LPROXY
    let arg = "ipset -X LPROXY";
    match do_bash_cmd(arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}
