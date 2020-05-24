use std::fmt;

#[derive(Debug)]
pub struct TunCfg {
    pub tunnel_number: usize,
    pub tunnel_url: String,
    pub tunnel_req_cap: usize,
    pub relay_domain: String,
    pub relay_port: u16,
    pub dns_tunnel_number: usize,

    pub domains_ver: String,
    pub domain_array: Option<Vec<String>>,
    pub local_dns_server: String,
    pub request_quota: usize,

    pub xport_url: String,
    pub token: String,

    pub cfg_monitor_url: String,

    pub cfg_access_report_url: String,
    pub work_as_global : bool,
}

pub fn server_url() -> String {
    "http://103.39.232.231:8002/auth".to_string()
}

pub struct AuthReq {
    pub uuid: String,
    pub current_version: String,
    pub arch: String,
    pub macs: Vec<String>,
}

impl AuthReq {
    pub fn to_json_str(&self) -> String {
        let macs = self.format_macs();
        format!(
            "{{\"uuid\":\"{}\",\
             \"current_version\":\"{}\",\"arch\":\"{}\", \"macs\":[{}]}}",
            self.uuid, self.current_version, self.arch, macs
        )
    }

    fn format_macs(&self) -> String {
        let mut macs = String::with_capacity(1024);
        let len = self.macs.len();
        for i in 0..len {
            let mac = &self.macs[i];
            macs.push_str(&format!("\"{}\"", mac));
            if i < len -1 {
                macs.push_str(",");
            }
        }

        macs
    }
}

pub struct CfgMonitorReq {
    pub current_version: String,
    pub domains_ver: String,
    pub arch: String,
}

impl CfgMonitorReq {
    pub fn to_json_str(&self) -> String {
        format!(
            "{{\"current_version\":\"{}\",\"domains_ver\":\"{}\",\"arch\":\"{}\"}}",
            self.current_version, self.domains_ver, self.arch
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
    pub fn from_json_str(s: &str) -> Option<AuthResp> {
        use serde_json::Value;
        let vv = serde_json::from_str(s);
        if vv.is_err() {
            return None;
        }

        let v: Value = vv.unwrap();
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

            let tunnel_url = match v_tuncfg["tunnel_url"].as_str() {
                Some(t) => t.to_string(),
                None => "wss://127.0.0.1/tun".to_string(),
            };

            let xport_url = match v_tuncfg["xport_url"].as_str() {
                Some(t) => t.to_string(),
                None => "https://localhost:8000/xportlws".to_string(),
            };

            let cfg_monitor_url = match v_tuncfg["cfg_monitor_url"].as_str() {
                Some(t) => t.to_string(),
                None => "https://localhost:8000/cfgmonitor".to_string(),
            };

            let cfg_access_report_url = match v_tuncfg["cfg_access_report_url"].as_str() {
                Some(t) => t.to_string(),
                None => "https://localhost:8000/accreport".to_string(),
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

            let local_dns_server = match v_tuncfg["local_dns_server"].as_str() {
                Some(t) => t.to_string(),
                None => "223.5.5.5:53".to_string(),
            };

            let dns_tunnel_number = match v_tuncfg["dns_tunnel_number"].as_u64() {
                Some(t) => t as usize,
                None => 1,
            };

            let work_as_global = match v_tuncfg["work_as_global"].as_bool() {
                Some(t) => t,
                None => false,
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
                tunnel_url,
                tunnel_req_cap: tunnel_req_cap,
                relay_domain: relay_domain,
                relay_port: relay_port,
                dns_tunnel_number: dns_tunnel_number,
                domain_array: Some(domain_array),
                local_dns_server,
                request_quota,
                xport_url,
                token: token.to_string(),
                domains_ver,
                cfg_monitor_url,
                cfg_access_report_url,
                work_as_global,
            };

            tuncfg = Some(tc);
        };

        Some(AuthResp {
            token: token.to_string(),
            restart: restart,
            tuncfg: tuncfg,
            need_upgrade,
            upgrade_url,
            error,
        })
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
