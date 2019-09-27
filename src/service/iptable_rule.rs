use super::ipset;
use log::{error, info};

pub fn set_iptables_rules(local_tcp_port: u16) {
    unset_iptables_rules();
    ipset::set_ipset();

    let ipt = iptables::new(false).unwrap();

    // sudo iptables -t mangle -N LPROXY_TCP
    // sudo iptables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port 5000 --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01
    // create LPROXY_TCP chain
    match ipt.new_chain("mangle", "LPROXY_TCP") {
        Ok(t) => info!("new chain LPROXY_TCP to mangle result:{}", t),
        Err(e) => {
            error!("new chain LPROXY_TCP to mangle error:{}", e);
        }
    }

    let rule = format!(
        "-p tcp -j TPROXY --on-port {} --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01",
        local_tcp_port
    );
    match ipt.append("mangle", "LPROXY_TCP", &rule) {
        Ok(t) => info!("append rule to mangle.LPROXY_TCP result:{}", t),
        Err(e) => {
            error!("append rule to mangle.LPROXY_TCP error:{}", e);
        }
    }

    // create bypass chain
    // sudo iptables -t mangle -N DIVERT
    // sudo iptables -t mangle -A DIVERT -j MARK --set-mark 1
    // sudo iptables -t mangle -A DIVERT -j ACCEPT
    match ipt.new_chain("mangle", "DIVERT") {
        Ok(t) => info!("new chain DIVERT to mangle result:{}", t),
        Err(e) => {
            error!("new chain DIVERT to mangle error:{}", e);
        }
    }

    match ipt.append("mangle", "DIVERT", "-j MARK --set-mark 1") {
        Ok(t) => info!("append rule to mangle.DIVERT result:{}", t),
        Err(e) => {
            error!("append rule to mangle.DIVERT error:{}", e);
        }
    }

    match ipt.append("mangle", "DIVERT", "-j ACCEPT") {
        Ok(t) => info!("append rule to mangle.DIVERT result:{}", t),
        Err(e) => {
            error!("append rule to mangle.DIVERT error:{}", e);
        }
    }

    // enable prerouting for CHAIN LPROXY_TCP
    match ipt.insert(
        "mangle",
        "PREROUTING",
        "-p tcp -m set --match-set LPROXY dst -j LPROXY_TCP",
        1,
    ) {
        Ok(t) => info!("insert rule to mangle.PREROUTING result:{}", t),
        Err(e) => {
            error!("insert rule LPROXY_TCP to mangle.PREROUTING error:{}", e);
        }
    }

    // enable prerouting for CHAIN DIVERT
    match ipt.insert("mangle", "PREROUTING", "-p tcp -m socket -j DIVERT", 1) {
        Ok(t) => info!("insert rule to mangle.PREROUTING result:{}", t),
        Err(e) => {
            error!("insert rule DIVERT to mangle.PREROUTING error:{}", e);
        }
    }
}

pub fn unset_iptables_rules() {
    let ipt = iptables::new(false).unwrap();
    // delete pretouing for chain DIVERT
    match ipt.delete("mangle", "PREROUTING", "-p tcp -m socket -j DIVERT") {
        Ok(t) => info!("delete rule DIVERT from mangle.PREROUTING result:{}", t),
        Err(e) => {
            error!("insert rule DIVERT to mangle.PREROUTING error:{}", e);
        }
    }

    // delete pretouing for chain LPROXY_TCP
    match ipt.delete(
        "mangle",
        "PREROUTING",
        "-p tcp -m set --match-set LPROXY dst -j LPROXY_TCP",
    ) {
        Ok(t) => info!("delete rule LPROXY_TCP from mangle.PREROUTING result:{}", t),
        Err(e) => {
            error!("insert rule LPROXY_TCP to mangle.PREROUTING error:{}", e);
        }
    }

    // flush chain DIVERT
    match ipt.flush_chain("mangle", "DIVERT") {
        Ok(t) => info!("flush chain DIVERT from mangle result:{}", t),
        Err(e) => {
            error!("flush chain DIVERT from mangle error:{}", e);
        }
    }

    // delete chain DIVERT
    match ipt.delete_chain("mangle", "DIVERT") {
        Ok(t) => info!("delete chain DIVERT from mangle result:{}", t),
        Err(e) => {
            error!("delete chain DIVERT from mangle error:{}", e);
        }
    }

    // flush chain LPROXY_TCP
    match ipt.flush_chain("mangle", "LPROXY_TCP") {
        Ok(t) => info!("flush chain LPROXY_TCP from mangle result:{}", t),
        Err(e) => {
            error!("flush chain LPROXY_TCP from mangle error:{}", e);
        }
    }

    // delete chain LPROXY_TCP
    match ipt.delete_chain("mangle", "LPROXY_TCP") {
        Ok(t) => info!("delete chain LPROXY_TCP from mangle result:{}", t),
        Err(e) => {
            error!("delete chain LPROXY_TCP from mangle error:{}", e);
        }
    }

    ipset::unset_ipset();
}
