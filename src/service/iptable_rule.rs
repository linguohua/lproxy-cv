use super::ip_rules::do_bash_cmd;
// use super::ipset;
use crate::config::{LOCAL_TPROXY_SERVER_PORT};

pub fn set_iptables_rules() {
    unset_iptables_rules();

    let args = format!(
        "iptables -t mangle -N LPROXY_TCP;\
            iptables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port {ppport} --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01;\
            iptables -t mangle -A LPROXY_TCP -p udp -j TPROXY --on-port {ppport} --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01;\
            iptables -t mangle -I PREROUTING -m set --match-set LPROXYN dst -j LPROXY_TCP;\
            iptables -t mangle -I PREROUTING -m set --match-set LPROXY dst -j LPROXY_TCP;\
            iptables -t mangle -N DIVERT;\
            iptables -t mangle -A DIVERT -j MARK --set-mark 1;\
            iptables -t mangle -A DIVERT -j ACCEPT;\
            iptables -t mangle -I PREROUTING -m socket -j DIVERT;\
            ip6tables -t mangle -N LPROXY_TCP;\
            ip6tables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port {ppport} --on-ip ::1 --tproxy-mark 0x01/0x01;\
            ip6tables -t mangle -A LPROXY_TCP -p udp -j TPROXY --on-port {ppport} --on-ip ::1 --tproxy-mark 0x01/0x01;\
            ip6tables -t mangle -I PREROUTING -m set --match-set LPROXY6 dst -j LPROXY_TCP;\
            ip6tables -t mangle -N DIVERT;\
            ip6tables -t mangle -A DIVERT -j MARK --set-mark 1;\
            ip6tables -t mangle -A DIVERT -j ACCEPT;\
            ip6tables -t mangle -I PREROUTING -m socket -j DIVERT;", ppport = LOCAL_TPROXY_SERVER_PORT);

    // iptables -t mangle -N DIVERT;\
    // iptables -t mangle -A DIVERT -j MARK --set-mark 1;\
    // iptables -t mangle -A DIVERT -j ACCEPT;\
    // iptables -t mangle -I PREROUTING -p tcp -m socket -j DIVERT
    // enable prerouting for CHAIN DIVERT
    match do_bash_cmd(&args) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_iptables_rules() {
    let args =
        "iptables -t mangle -D PREROUTING -m set --match-set LPROXY dst -j LPROXY_TCP;\
         iptables -t mangle -D PREROUTING -m set --match-set LPROXYN dst -j LPROXY_TCP;\
         iptables -t mangle -D PREROUTING -m socket -j DIVERT;\
         iptables -t mangle -F DIVERT;\
         iptables -t mangle -X DIVERT;\
         iptables -t mangle -F LPROXY_TCP;\
         iptables -t mangle -X LPROXY_TCP;\
         ip6tables -t mangle -D PREROUTING -m set --match-set LPROXY6 dst -j LPROXY_TCP;\
         ip6tables -t mangle -D PREROUTING -m socket -j DIVERT;\
         ip6tables -t mangle -F LPROXY_TCP;\
         ip6tables -t mangle -X LPROXY_TCP;\
         ip6tables -t mangle -F DIVERT;\
         ip6tables -t mangle -X DIVERT;";

    // iptables -t mangle -F DIVERT;\
    // iptables -t mangle -X DIVERT;\

    match do_bash_cmd(args) {
        Ok(_) => {}
        Err(_) => {}
    }

    // ipset::unset_ipset();
}

pub fn set_iptables_rules_for_global() {
    unset_iptables_rules_for_global();

    // TODO: fix ipv6
    let args = format!(
        "iptables -t mangle -N LPROXY_TCP;\
            iptables -t mangle -A LPROXY_TCP -d 0.0.0.0/8 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 10.0.0.0/8 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 127.0.0.0/8 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 169.254.0.0/16 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 172.16.0.0/12 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 192.168.0.0/16 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 224.0.0.0/4 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -d 240.0.0.0/4 -j RETURN;\
            iptables -t mangle -A LPROXY_TCP -p tcp -j TPROXY --on-port {ppport} --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01;\
            iptables -t mangle -A LPROXY_TCP -p udp -j TPROXY --on-port {ppport} --on-ip 127.0.0.1 --tproxy-mark 0x01/0x01;\
            iptables -t mangle -I PREROUTING -j LPROXY_TCP;\
            iptables -t mangle -N DIVERT;\
            iptables -t mangle -A DIVERT -j MARK --set-mark 1;\
            iptables -t mangle -A DIVERT -j ACCEPT;\
            iptables -t mangle -I PREROUTING -m socket -j DIVERT;", ppport = LOCAL_TPROXY_SERVER_PORT);

    // iptables -t mangle -N DIVERT;\
    // iptables -t mangle -A DIVERT -j MARK --set-mark 1;\
    // iptables -t mangle -A DIVERT -j ACCEPT;\
    // iptables -t mangle -I PREROUTING -p tcp -m socket -j DIVERT
    // enable prerouting for CHAIN DIVERT
    match do_bash_cmd(&args) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn unset_iptables_rules_for_global() {
    // TODO: fix ipv6
    let args =
        "iptables -t mangle -D PREROUTING -j LPROXY_TCP;\
            iptables -t mangle -D PREROUTING -m socket -j DIVERT;\
            iptables -t mangle -F DIVERT;\
            iptables -t mangle -X DIVERT;\
            iptables -t mangle -F LPROXY_TCP;\
            iptables -t mangle -X LPROXY_TCP;";

    // iptables -t mangle -N DIVERT;\
    // iptables -t mangle -A DIVERT -j MARK --set-mark 1;\
    // iptables -t mangle -A DIVERT -j ACCEPT;\
    // iptables -t mangle -I PREROUTING -p tcp -m socket -j DIVERT
    // enable prerouting for CHAIN DIVERT
    match do_bash_cmd(args) {
        Ok(_) => {}
        Err(_) => {}
    }
}
