use std::fmt;

#[derive(Debug)]
pub struct TunCfg {
    pub tunnel_number: usize,
    pub websocket_url: String,

    pub tunnel_req_cap: usize,
    pub relay_domain: String,
    pub relay_port: u16,

    pub dns_tun_url: String,
    pub dns_tunnel_number: usize,

    pub domains_ver: String,
    pub domain_array: Option<Vec<String>>,
    pub local_dns_server: String,
    pub request_quota: usize,

    pub xport_url: String,
    pub token: String,
}

pub fn server_url() -> String {
    "https://127.0.0.1:8000/auth".to_string()
}

pub struct AuthReq {
    pub uuid: String,
    pub current_version: String,
    pub domains_ver: String,
    pub is_cfgmonitor: bool,
}

impl AuthReq {
    pub fn to_json_str(&self) -> String {
        format!(
            "{{\"uuid\":\"{}\",\"is_cfgmonitor\":{},\
             \"current_version\":\"{}\",\"domains_ver\":\"{}\"}}",
            self.uuid, self.is_cfgmonitor, self.current_version, self.domains_ver
        )
    }
}

pub struct AuthResp {
    pub error: i64,
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
        let mut token = "";
        if v["token"].is_string() {
            token = v["token"].as_str().unwrap();
        }

        let error = match v["error"].as_i64() {
            Some(t) => t,
            None => 0,
        };

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

            let xport_url = match v_tuncfg["xport_url"].as_str() {
                Some(t) => t.to_string(),
                None => "https://localhost:8000/xportlws".to_string(),
            };

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

            let domains_ver = match v_tuncfg["domains_ver"].as_str() {
                Some(t) => t.to_string(),
                None => "0.1.0".to_string(),
            };

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
                tunnel_req_cap: tunnel_req_cap,
                relay_domain: relay_domain,
                relay_port: relay_port,
                dns_tun_url: dns_tun_url,
                dns_tunnel_number: dns_tunnel_number,
                domain_array: Some(domain_array),
                local_dns_server,
                request_quota,
                xport_url,
                token: token.to_string(),
                domains_ver,
            };

            tuncfg = Some(tc);
        };

        AuthResp {
            token: token.to_string(),
            restart: restart,
            tuncfg: tuncfg,
            need_upgrade,
            upgrade_url,
            error,
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
