use flutter_rust_bridge::frb;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::Range;

pub use client::config::ServerConfig as ClientServerConfig;
pub use crypto::config::{DataPadding, ProtocolConfig as CryptoProtocolConfig};
pub use crypto::{cipher::CipherType, kdf::Kdf};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub caption: Option<String>,
    pub host: String,
    pub weight: Option<u8>,
    pub domains: Option<Vec<String>>,
    pub enabled: bool,
    pub protocol: ProtocolConfig,
}

impl Into<ClientServerConfig> for ServerConfig {
    fn into(self) -> ClientServerConfig {
        return ClientServerConfig {
            caption: self.caption.clone(),
            host: self.host.clone(),
            weight: self.weight.clone(),
            domains: self.domains.clone(),
            enabled: self.enabled,
            protocol: self.protocol.into(),
            url_path: None,
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0u16),
        };
    }
}

impl From<ClientServerConfig> for ServerConfig {
    fn from(cfg: ClientServerConfig) -> Self {
        return ServerConfig {
            caption: cfg.caption.clone(),
            host: cfg.host.clone(),
            weight: cfg.weight.clone(),
            domains: cfg.domains.clone(),
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

impl Into<CryptoProtocolConfig> for ProtocolConfig {
    fn into(self) -> CryptoProtocolConfig {
        return CryptoProtocolConfig {
            key: self.key,
            kdf: self.kdf,
            cipher: self.cipher,
            max_connect_delay: self.max_connect_delay,
            header_padding: Range {
                start: self.header_padding.start,
                end: self.header_padding.end,
            },
            data_padding: self.data_padding,
            encryption_limit: self.encryption_limit,
        };
    }
}

impl From<CryptoProtocolConfig> for ProtocolConfig {
    fn from(cfg: CryptoProtocolConfig) -> Self {
        return ProtocolConfig {
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
