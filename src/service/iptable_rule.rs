use super::ip_rules::do_bash_cmd;
use super::ipset;

pub fn set_iptables_rules() {
    unset_iptables_rules();
    ipset::set_ipset();

    let args = "iptables -t mangle -N LPROXY_TCP;\
                iptables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port 5000 --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01;\
                iptables -t mangle -N DIVERT;\
                iptables -t mangle -A DIVERT -j MARK --set-mark 1;\
                iptables -t mangle -A DIVERT -j ACCEPT;\
                iptables -t mangle -I PREROUTING -p tcp -m set --match-set LPROXY dst -j LPROXY_TCP;\
                iptables -t mangle -I PREROUTING -p tcp -m socket -j DIVERT";

    // enable prerouting for CHAIN DIVERT
    match do_bash_cmd(args) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_iptables_rules() {
    let args = "iptables -t mangle -D PREROUTING -p tcp -m socket -j DIVERT;\
                iptables -t mangle -D PREROUTING -p tcp -m set --match-set LPROXY dst -j LPROXY_TCP;\
                iptables -t mangle -F DIVERT;\
                iptables -t mangle -X DIVERT;\
                iptables -t mangle -F LPROXY_TCP;\
                iptables -t mangle -X LPROXY_TCP";

    match do_bash_cmd(args) {
        Ok(_) => {}
        Err(_) => {}
    }

    ipset::unset_ipset();
}
