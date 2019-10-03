mod tuncfg;
pub use tuncfg::*;
mod logsimple;
pub use logsimple::*;

pub const IPSET_TABLE_NULL: &str = "LPROXY\0";
pub const IPSET_TABLE: &str = "LPROXY";
pub const DEFAULT_DNS_SERVER: &str = "223.5.5.5";
