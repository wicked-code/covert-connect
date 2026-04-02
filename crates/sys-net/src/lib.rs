pub mod outbound;
pub mod dns;

pub use outbound::find_outbound_ip;
pub use dns::{flush_system_dns_cache, get_dns_by_if_addr};
