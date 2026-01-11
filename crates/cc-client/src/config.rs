use anyhow::anyhow;
use std::path::Path;
use serde::Deserialize;
use anyhow::Result;

use client::config::ServerConfig;

/// Main application config
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    /// proxy local address
    #[serde(default = "default_proxy_port")]
    pub proxy_port: u16,

    pub servers: Vec<ServerConfig>,
}

fn default_proxy_port() -> u16 {
    25443
}

impl AppConfig {
    pub fn new<P>(path: P) -> Result<AppConfig> 
    where
        P: AsRef<Path>
    {
        let config = std::fs::read_to_string(&path)?;
        
        let expanded = shellexpand::full(&config)?;

        config::Config::builder()
            .add_source(config::File::from_str(
                expanded.as_ref(),
                config::FileFormat::Yaml,
            ))
            .build()?
            .try_deserialize()
            .map_err(|err| anyhow!("deserialize config error: {}", err))
    }

    pub async fn init(mut self) -> Result<AppConfig> {

        for srv in &mut self.servers {
            srv.init().await?;
        }

        Ok(self)
    }
}
