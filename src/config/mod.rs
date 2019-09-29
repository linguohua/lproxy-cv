mod tuncfg;
pub use tuncfg::*;
mod logsimple;
pub use logsimple::*;

pub const IPSET_TABLE_NULL: &str = "LPROXY\0";
pub const IPSET_TABLE: &str = "LPROXY";
