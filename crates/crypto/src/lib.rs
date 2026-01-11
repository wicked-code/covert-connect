pub mod stream;
pub mod kdf;
pub mod cipher;

pub mod config;
pub use config::DataPadding;

pub const MIN_HOST_LEN: usize = 4;
// it's more than 5 for sure -> 3 (domain + '.' + zone) + ":" 1 (port), but 4 is enough to store len in u8

pub const GET_PROTOCOL_MAX_CONNECT_DELAY: usize = 30000;
