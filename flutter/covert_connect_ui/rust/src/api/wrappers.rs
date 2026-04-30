use flutter_rust_bridge::frb;
use std::ops::Range;

pub use client::config::ServerConfig as ClientServerConfig;
pub use client::log::LogLine;
pub use crypto::config::{DataPadding, ProtocolConfig as CryptoProtocolConfig};
pub use crypto::{cipher::CipherType, kdf::Kdf};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub caption: Option<String>,
    pub host: String,
    pub weight: Option<u8>,
    pub domains: Option<Vec<String>>,
    pub apps: Option<Vec<String>>,
    pub enabled: bool,
    pub protocol: ProtocolConfig,
}

impl From<ServerConfig> for ClientServerConfig {
    fn from(cfg: ServerConfig) -> ClientServerConfig {
        ClientServerConfig {
            caption: cfg.caption.clone(),
            host: cfg.host.clone(),
            weight: cfg.weight,
            domains: cfg.domains.clone(),
            apps: cfg.apps.clone(),
            enabled: cfg.enabled,
            protocol: cfg.protocol.into(),
        }
    }
}

impl From<ClientServerConfig> for ServerConfig {
    fn from(cfg: ClientServerConfig) -> Self {
        ServerConfig {
            caption: cfg.caption.clone(),
            host: cfg.host.clone(),
            weight: cfg.weight,
            domains: cfg.domains.clone(),
            apps: cfg.apps.clone(),
            enabled: cfg.enabled,
            protocol: cfg.protocol.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProtocolConfig {
    pub key: String,
    pub kdf: Kdf,
    pub cipher: CipherType,
    pub max_connect_delay: u16,
    pub header_padding: HeaderPadding,
    pub data_padding: DataPadding,
    pub encryption_limit: usize,
}

impl From<ProtocolConfig> for CryptoProtocolConfig {
    fn from(cfg: ProtocolConfig) -> CryptoProtocolConfig {
        CryptoProtocolConfig {
            key: cfg.key,
            kdf: cfg.kdf,
            cipher: cfg.cipher,
            max_connect_delay: cfg.max_connect_delay,
            header_padding: Range {
                start: cfg.header_padding.start,
                end: cfg.header_padding.end,
            },
            data_padding: cfg.data_padding,
            encryption_limit: cfg.encryption_limit,
        }
    }
}

impl From<CryptoProtocolConfig> for ProtocolConfig {
    fn from(cfg: CryptoProtocolConfig) -> Self {
        ProtocolConfig {
            key: cfg.key,
            kdf: cfg.kdf,
            cipher: cfg.cipher,
            max_connect_delay: cfg.max_connect_delay,
            header_padding: HeaderPadding {
                start: cfg.header_padding.start,
                end: cfg.header_padding.end,
            },
            data_padding: cfg.data_padding,
            encryption_limit: cfg.encryption_limit,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HeaderPadding {
    pub start: u16,
    pub end: u16,
}

#[frb(mirror(DataPadding))]
pub struct _DataPadding {
    pub max: u16,
    pub rate: u8,
}

#[frb(mirror(Kdf))]
pub enum _Kdf {
    Argon2,
    Blake3,
}

#[frb(mirror(CipherType))]
pub enum _CipherType {
    Aes256Gcm,
    ChaCha20Poly1305,
}

#[frb(mirror(LogLine))]
pub struct _LogLine {
    pub line: String,
    pub position: u64,
}
