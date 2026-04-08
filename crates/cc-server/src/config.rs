use anyhow::{Result, anyhow};
use colored::*;
use crypto::config::{ProtocolConfig, range_from_human_readable};
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    ops::Range,
    path::Path,
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Egress {
    /// outbound IPv4 address
    #[serde(default)]
    pub ipv4: Option<Ipv4Addr>,

    /// outbound IPv6 address
    #[serde(default)]
    pub ipv6: Option<Ipv6Addr>,

    /// outbound address
    #[serde(default = "default_true")]
    pub prefer_v4: bool,
}

/// Main application config
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// server address
    pub address: SocketAddr,

    /// outbound ip's config
    #[serde(default = "default_egress")]
    pub egress: Egress,

    /// protocol configuration
    #[serde(flatten)]
    pub protocol: ProtocolConfig,

    /// min a max waiting data time (ms) before close the connection
    /// needed to prevent probe for header size
    /// reads random number of bytes, at most u16::MAX (65535)
    /// random timeout for the read is selected in specified range
    #[serde(default = "default_cooldown")]
    #[serde(deserialize_with = "range_from_human_readable")]
    pub unauth_cooldown: Range<u16>,
}

fn default_cooldown() -> Range<u16> {
    50..777
}

impl AppConfig {
    pub fn new<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let config = std::fs::read_to_string(&path)?;
        let expanded = shellexpand::full(&config)?;

        Self::build(expanded.as_ref()).map_err(|err| anyhow!("deserialize config: {}", err))
    }

    fn build(cfg_str: &str) -> Result<Self> {
        config::Config::builder()
            .add_source(config::File::from_str(cfg_str, config::FileFormat::Yaml))
            .build()?
            .try_deserialize::<Self>()?
            .check()
    }

    fn check(self) -> Result<AppConfig> {
        if let Some(out_v4) = self.egress.ipv4 && let SocketAddr::V4(addr_v4) = self.address && out_v4 == *addr_v4.ip() {
            anyhow::bail!(
                "{} egress address should not be the same as listen address {}, set different egress address or remove egress",
                out_v4.to_string().bold(),
                self.address.ip().to_string().bold(),
            )
        }

        if let Some(out_v6) = self.egress.ipv6 && let SocketAddr::V6(addr_v6) = self.address && out_v6 == *addr_v6.ip() {
            anyhow::bail!(
                "{} egress address should not be the same as listen address {}, set different egress address or remove egress",
                out_v6.to_string().bold(),
                self.address.ip().to_string().bold(),
            )
        }
        Ok(self)
    }
}

fn default_egress() -> Egress {
    Egress {
        ipv4: None,
        ipv6: None,
        prefer_v4: true,
    }
}

fn default_true() -> bool {
    true
}