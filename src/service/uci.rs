use super::ip_rules::do_bash_cmd;
use crate::config::{LOCAL_DNS_SERVER_PORT, LOCAL_SERVER};

pub fn set_uci_dnsmasq_to_default(dns_server:String) {
    let arg = format!(
        "uci -q delete dhcp.@dnsmasq[0].server;\
         uci -q add_list dhcp.@dnsmasq[0].server=\"{}\";\
         uci -q set dhcp.@dnsmasq[0].noresolv='1';\
         uci commit dhcp;\
         /etc/init.d/dnsmasq restart",
         dns_server
    );

    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}

pub fn set_uci_dnsmasq_to_me() {
    let arg = format!(
        "uci -q delete dhcp.@dnsmasq[0].server;\
         uci -q add_list dhcp.@dnsmasq[0].server=\"{}#{}\";\
         uci commit dhcp;\
         /etc/init.d/dnsmasq restart",
        LOCAL_SERVER, LOCAL_DNS_SERVER_PORT
    );

    match do_bash_cmd(&arg) {
        Ok(_) => {}
        Err(_) => {}
    }
}
