use anyhow::Result;
use anyhow::anyhow;
use serde::Deserialize;
use std::path::Path;

use client::config::ServerConfig;

/// Main application config
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub servers: Vec<ServerConfig>,
}

impl AppConfig {
    pub fn new<P>(path: P) -> Result<AppConfig>
    where
        P: AsRef<Path>,
    {
        let config = std::fs::read_to_string(&path)?;

        let expanded = shellexpand::full(&config)?;

        config::Config::builder()
            .add_source(config::File::from_str(expanded.as_ref(), config::FileFormat::Yaml))
            .build()?
            .try_deserialize()
            .map_err(|err| anyhow!("deserialize config error: {}", err))
    }
}
