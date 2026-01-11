use std::{
    ops::Range,
    path::Path,
    net::{SocketAddr, IpAddr},
};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use colored::*;
use crypto::config::{ProtocolConfig, range_from_human_readable};

/// Main application config
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// server address
    pub address: SocketAddr,

    /// outbound address
    pub out_address: Option<IpAddr>,

    /// protocol configuration
    #[serde(flatten)]
    pub protocol: ProtocolConfig,

    /// min a max waiting data time (ms) before close the connection
    /// needed to prevent probe for header size
    /// reads random number of bytes, at most u16::MAX (65535)
    /// random timeout for the read is selected in specified range
    #[serde(default = "default_cooldown")]
    #[serde(deserialize_with = "range_from_human_readable")]
    pub unauth_cooldown: Range<u16>
}

fn default_cooldown() -> Range<u16> {
    50..777
}

impl AppConfig {
    pub fn new<P>(path: P) -> Result<Self> 
    where
        P: AsRef<Path>
    {
        let config = std::fs::read_to_string(&path)?;
        let expanded = shellexpand::full(&config)?;

        Self::build(expanded.as_ref())
            .map_err(|err| anyhow!("deserialize config: {}", err))
    }

    fn build(cfg_str: &str) -> Result<Self> {
        config::Config::builder()
            .add_source(config::File::from_str(
                cfg_str,
                config::FileFormat::Yaml,
            ))
            .build()?
            .try_deserialize::<Self>()?
            .check()
    }

    fn check(self) -> Result<AppConfig> {
        if let Some(out_addr) = self.out_address {
            if self.address.ip().is_unspecified() {
                anyhow::bail!("{} listen to any available ip. \
                    Please select specific ip in address option or 127.0.0.1 if https mode \
                    (make sure nginx is not listen to {}) or remove out_address option.",
                    self.address.ip().to_string().bold(),
                    out_addr.to_string().bold())
            }

            if self.address.is_ipv4() != out_addr.is_ipv4() {
                anyhow::bail!("{} listen address version (v4 or v6) should be the same as version of out_address {}",
                    self.address.ip().to_string().bold(),
                    out_addr.to_string().bold())
            }

            if self.address.ip() == out_addr {
                anyhow::bail!("out_address {} should not be equal to address {}",
                    self.address.ip().to_string().bold(),
                    out_addr.to_string().bold())
            }
        }

        Ok(self)
    }
}
