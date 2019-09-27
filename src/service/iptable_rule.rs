use super::ip_rules::do_bash_cmd;
use super::ipset;

pub fn set_iptables_rules(local_tcp_port: u16) {
    unset_iptables_rules();
    ipset::set_ipset();

    // sudo iptables -t mangle -N LPROXY_TCP
    // sudo iptables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port 5000 --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01
    // create LPROXY_TCP chain
    match do_bash_cmd("iptables -t mangle -N LPROXY_TCP") {
        Ok(_) => {}
        Err(_) => {}
    }

    let rule = format!(
        "iptables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port {} --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01",
        local_tcp_port
    );
    match do_bash_cmd(&rule) {
        Ok(_) => {}
        Err(_) => {}
    }

    // create bypass chain
    // sudo iptables -t mangle -N DIVERT
    // sudo iptables -t mangle -A DIVERT -j MARK --set-mark 1
    // sudo iptables -t mangle -A DIVERT -j ACCEPT
    match do_bash_cmd("iptables -t mangle -N DIVERT") {
        Ok(_) => {}
        Err(_) => {}
    }

    match do_bash_cmd("iptables -t mangle -A DIVERT -j MARK --set-mark 1") {
        Ok(_) => {}
        Err(_) => {}
    }

    match do_bash_cmd("iptables -t mangle -A DIVERT -j ACCEPT") {
        Ok(_) => {}
        Err(_) => {}
    }

    // enable prerouting for CHAIN LPROXY_TCP
    match do_bash_cmd(
        "iptables -t mangle -I PREROUTING -p tcp -m set --match-set LPROXY dst -j LPROXY_TCP",
    ) {
        Ok(_) => {}
        Err(_) => {}
    }

    // enable prerouting for CHAIN DIVERT
    match do_bash_cmd("iptables -t mangle -I PREROUTING -p tcp -m socket -j DIVERT") {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_iptables_rules() {
    // delete pretouing for chain DIVERT
    match do_bash_cmd("iptables -t mangle -D PREROUTING -p tcp -m socket -j DIVERT") {
        Ok(_) => {}
        Err(_) => {}
    }

    // delete pretouing for chain LPROXY_TCP
    match do_bash_cmd(
        "iptables -t mangle -D PREROUTING -p tcp -m set --match-set LPROXY dst -j LPROXY_TCP",
    ) {
        Ok(_) => {}
        Err(_) => {}
    }

    // flush chain DIVERT
    match do_bash_cmd("iptables -t mangle -F DIVERT") {
        Ok(_) => {}
        Err(_) => {}
    }

    // delete chain DIVERT
    match do_bash_cmd("iptables -t mangle -X DIVERT") {
        Ok(_) => {}
        Err(_) => {}
    }

    // flush chain LPROXY_TCP
    match do_bash_cmd("iptables -t mangle -F LPROXY_TCP") {
        Ok(_) => {}
        Err(_) => {}
    }

    match do_bash_cmd("iptables -t mangle -X LPROXY_TCP") {
        Ok(_) => {}
        Err(_) => {}
    }

    ipset::unset_ipset();
}
