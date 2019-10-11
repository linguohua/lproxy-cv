use std::fmt;

#[derive(Debug)]
pub struct TunCfg {
    pub tunnel_number: usize,
    pub websocket_url: String,

    // pub local_tcp_port: u16,
    pub tunnel_req_cap: usize,
    pub relay_domain: String,
    pub relay_port: u16,

    pub dns_tun_url: String,
    pub dns_tunnel_number: usize,

    pub domain_array: Option<Vec<String>>,
    pub local_dns_server: String,
    pub request_quota: usize,
}

// impl TunCfg {
// pub fn new() -> TunCfg {
//     TunCfg {
//         tunnel_number: 2,
//         websocket_url: "wss://127.0.0.1/tun".to_string(),
//         local_server: "127.0.0.1:5000".to_string(),
//         tunnel_req_cap: 100,
//         relay_domain: "127.0.0.1".to_string(),
//         relay_port: 12345,

//         dns_udp_addr: "127.0.0.1:5000".to_string(),
//         dns_tun_url: "wss://127.0.0.1/dns".to_string(),
//         dns_tunnel_number: 1,
//     }
// }
// }

pub fn server_url() -> String {
    "https://127.0.0.1:8000/auth".to_string()
}

pub struct AuthReq {
    pub uuid: String,
    pub current_version: String,
}

impl AuthReq {
    pub fn to_json_str(&self) -> String {
        format!(
            "{{\"uuid\":\"{}\",\"current_version\":\"{}\"}}",
            self.uuid, self.current_version
        )
    }
}

pub struct AuthResp {
    pub token: String,
    pub restart: bool,
    pub need_upgrade: bool,
    pub upgrade_url: String,
    pub tuncfg: Option<TunCfg>,
}

impl AuthResp {
    pub fn from_json_str(s: &str) -> AuthResp {
        use serde_json::Value;
        let v: Value = serde_json::from_str(s).unwrap();

        let restart = match v["restart"].as_bool() {
            Some(x) => x,
            None => false,
        };

        let need_upgrade = match v["need_upgrade"].as_bool() {
            Some(x) => x,
            None => false,
        };

        let upgrade_url = match v["upgrade_url"].as_str() {
            Some(x) => x.to_string(),
            None => String::default(),
        };

        let v_tuncfg = &v["tuncfg"];
        let mut tuncfg = None;
        if v_tuncfg.is_object() {
            let tunnel_number = match v_tuncfg["tunnel_number"].as_u64() {
                Some(t) => t as usize,
                None => 2,
            };

            let request_quota = match v_tuncfg["request_quota"].as_u64() {
                Some(t) => t as usize,
                None => 10,
            };

            let websocket_url = match v_tuncfg["websocket_url"].as_str() {
                Some(t) => t.to_string(),
                None => "wss://127.0.0.1/tun".to_string(),
            };

            // let local_server = match v_tuncfg["local_server"].as_str() {
            //     Some(t) => t.to_string(),
            //     None => "127.0.0.1:5000".to_string(),
            // };

            let tunnel_req_cap = match v_tuncfg["tunnel_req_cap"].as_u64() {
                Some(t) => t as usize,
                None => 100,
            };

            let relay_domain = match v_tuncfg["relay_domain"].as_str() {
                Some(t) => t.to_string(),
                None => "127.0.0.1".to_string(),
            };

            let relay_port = match v_tuncfg["relay_port"].as_u64() {
                Some(t) => t as u16,
                None => 12345,
            };

            // let dns_udp_addr = match v_tuncfg["dns_udp_addr"].as_str() {
            //     Some(t) => t.to_string(),
            //     None => "127.0.0.1:5000".to_string(),
            // };

            let dns_tun_url = match v_tuncfg["dns_tun_url"].as_str() {
                Some(t) => t.to_string(),
                None => "wss://127.0.0.1/dns".to_string(),
            };

            let local_dns_server = match v_tuncfg["local_dns_server"].as_str() {
                Some(t) => t.to_string(),
                None => "223.5.5.5:53".to_string(),
            };

            let dns_tunnel_number = match v_tuncfg["dns_tunnel_number"].as_u64() {
                Some(t) => t as usize,
                None => 1,
            };

            // let local_tcp_port = match v_tuncfg["local_tcp_port"].as_u64() {
            //     Some(t) => t as u16,
            //     None => 5000,
            // };

            let mut domain_array: Vec<String> = Vec::new();
            match v_tuncfg["domain_array"].as_array() {
                Some(t_array) => {
                    for vv in t_array.iter() {
                        match vv.as_str() {
                            Some(t) => domain_array.push(t.to_string()),
                            None => {}
                        };
                    }
                }
                None => {}
            };

            let tc = TunCfg {
                tunnel_number: tunnel_number,
                websocket_url: websocket_url,
                // local_server: local_server,
                tunnel_req_cap: tunnel_req_cap,
                relay_domain: relay_domain,
                relay_port: relay_port,
                // local_tcp_port,
                // dns_udp_addr: dns_udp_addr,
                dns_tun_url: dns_tun_url,
                dns_tunnel_number: dns_tunnel_number,
                domain_array: Some(domain_array),
                local_dns_server,
                request_quota,
            };

            tuncfg = Some(tc);
        };

        AuthResp {
            token: v["token"].to_string(),
            restart: restart,
            tuncfg: tuncfg,
            need_upgrade,
            upgrade_url,
        }
    }
}

impl fmt::Debug for AuthResp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{ AuthResp token:{}, restart:{}, tuncfg:{:?} }}",
            self.token, self.restart, self.tuncfg
        )
    }
}
