mod tuncfg;
pub use tuncfg::*;
mod logsimple;
pub use logsimple::*;

pub const IPSET_TABLE_NULL: &str = "LPROXY\0";
pub const IPSET_TABLE: &str = "LPROXY";
pub const DEFAULT_DNS_SERVER: &str = "223.5.5.5";
pub const LOCAL_SERVER: &str = "127.0.0.1";
pub const LOCAL_SERVER_PORT: u16 = 5000;
pub const KEEP_ALIVE_INTERVAL: u64 = 15 * 1000;
//  pub const CFG_MONITOR_INTERVAL: u64 = 60 * 1000;
pub const CFG_MONITOR_INTERVAL: u64 = 30 * 60 * 1000;
pub const LPROXY_SCRIPT: &str = "lps.sh";

pub const DEFAULT_REQ_QUOTA: u16 = 1000;
