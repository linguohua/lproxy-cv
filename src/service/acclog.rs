use std::net::IpAddr;
use fnv::FnvHashMap as HashMap;
use fnv::FnvHashSet as HashSet;
use super::{AccReport, AccDomain};

pub struct DNSAddRecord {
    pub dns_records: Vec<(String, IpAddr)>,
}

impl DNSAddRecord {
    pub fn new() -> DNSAddRecord {
        DNSAddRecord {
            dns_records: Vec::with_capacity(4),
        }
    }

    pub fn add(&mut self, ip: IpAddr, name: &String) {
        self.dns_records.push((name.to_string(), ip));
    }
}

// Acc: access domain record
struct AccDomainRecord {
    src_addrs : HashSet<IpAddr>,
}

impl AccDomainRecord {
    pub fn new() -> AccDomainRecord {
        AccDomainRecord {
            src_addrs: HashSet::default(),
        }
    }

    pub fn log(&mut self, dst_ip: IpAddr) {
        self.src_addrs.insert(dst_ip);
    }
}

// Acc: access log
pub struct AccLog {
    domains: HashMap<IpAddr, String>,
    acc_domains: HashMap<String, AccDomainRecord>,
}

impl AccLog {
    pub fn new() -> AccLog {
        AccLog {
            domains: HashMap::default(),
            acc_domains: HashMap::default(),
        }
    }

    pub fn log(&mut self, src_ip: IpAddr, dst_ip : IpAddr) {
        let domain;
        match self.domains.get(&src_ip) {
            Some(domain1) => {
                // use exist domain name
                domain = domain1.to_string();
            }
            None => {
                // convert ip to domain name
                domain = src_ip.to_string();
            }
        }

        match self.acc_domains.get_mut(&domain) {
            Some(acc_domain1) => {
                acc_domain1.log(dst_ip);
            }
            None => {
                let mut acc_domain1 = AccDomainRecord::new();
                acc_domain1.log(dst_ip);
                self.acc_domains.insert(domain, acc_domain1);
            }
        }
    }

    pub fn domain_add(&mut self, domain : DNSAddRecord) {
        for dd in domain.dns_records.iter() {
            self.domains.insert(dd.1, dd.0.to_string());
        }
    }

    pub fn clear_log(&mut self) {
        self.acc_domains.clear();
    }

    pub fn dump_to_pb(&self) -> AccReport {
        let mut report = AccReport::default();
        for (k, ad) in self.acc_domains.iter() {
            let mut pad = AccDomain::default();
            pad.domain = k.to_string();
            for ip in ad.src_addrs.iter() {
                match ip {
                    IpAddr::V4(v4) => {
                        pad.srcIP.push(u32::from_be_bytes(v4.octets()));
                    }
                    _ => {

                    }
                }
            }

            report.domains.push(pad);
        }

        report
    }
}
