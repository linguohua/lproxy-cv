use fnv::FnvHashMap as HashMap;
use std::fmt;

struct SubDomain {
    next_subdomains: HashMap<String, SubDomain>,
    leaf: bool,
}

impl fmt::Display for SubDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "(SubDomain, next_subdomains count:{})\n",
            self.next_subdomains.len()
        )?;

        for (_, n) in self.next_subdomains.iter() {
            fmt::Display::fmt(&n, f)?;
        }

        Ok(())
    }
}

impl SubDomain {
    pub fn new() -> SubDomain {
        SubDomain {
            leaf: false,
            next_subdomains: HashMap::default(),
        }
    }

    pub fn insert(&mut self, parts: &Vec<&str>, idx: usize) {
        let domain = parts[idx];
        let subdomain = self.get_or_insert(domain);

        let next = idx + 1;
        if parts.len() > next {
            subdomain.insert(parts, next);
        } else {
            subdomain.leaf = true;
        }
    }

    fn get_or_insert(&mut self, domain: &str) -> &mut SubDomain {
        let domains = &mut self.next_subdomains;
        if !domains.contains_key(domain) {
            domains.insert(domain.to_string(), SubDomain::new());
        }

        let s = domains.get_mut(domain);
        s.unwrap()
    }

    pub fn has(&self, parts: &Vec<&str>, idx: usize) -> bool {
        // if we can reach the root, then ok
        let domain = parts[idx];
        let subdomain = self.next_subdomains.get(domain);
        match subdomain {
            Some(s) => {
                if s.leaf || parts.len() <= idx + 1 {
                    return true;
                } else {
                    return s.has(parts, idx + 1);
                }
            }
            None => return false,
        }
    }
}

pub struct DomainMap {
    root: SubDomain,
}

impl fmt::Display for DomainMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(DnMap:)\n")?;

        fmt::Display::fmt(&self.root, f)
    }
}

impl DomainMap {
    pub fn new() -> DomainMap {
        DomainMap {
            root: SubDomain::new(),
        }
    }

    pub fn insert(&mut self, domain: &str) {
        // split
        let splits = domain.split(".");
        let mut vec: Vec<&str> = splits.collect();

        // revert
        vec.reverse();
        if vec.len() < 2 {
            println!("domain split < 2");
            return;
        }

        // top level
        let top = vec[0];
        if top.len() < 1 {
            println!("domain name is empty");
            return;
        }

        self.root.insert(&vec, 0);
    }

    pub fn has(&self, domain: &str) -> bool {
        // split
        let splits = domain.split(".");
        let mut vec: Vec<&str> = splits.collect();

        // revert
        vec.reverse();
        if vec.len() < 2 {
            println!("domain split < 2");
            return false;
        }

        self.root.has(&vec, 0)
    }
}
