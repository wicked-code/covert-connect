use std::{io::ErrorKind, path::Path};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize};

use crypto::config::ProtocolConfig;

use crate::client::ClientState;

/// Main application config
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct ClientConfig {
    pub state: ClientState,
    pub direct_domains: Vec<String>,
    pub direct_apps: Vec<String>,
    pub servers: Vec<ServerConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,

    /// can be ip:port or host:port (port optional, 443 by default)
    pub host: String,

    /// weight of the server, avr if not set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u8>,

    /// the server should be used for these domains
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "sort_opt_vec")]
    pub domains: Option<Vec<String>>,

    /// the server should be used for these apps
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "sort_opt_vec")]
    pub apps: Option<Vec<String>>,

    /// enabled if None
    #[serde(skip_serializing_if = "is_true")]
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// protocol configuration
    pub protocol: ProtocolConfig,
}

fn is_true(val: &bool) -> bool {
    *val
}

fn default_enabled() -> bool {
    true
}

fn sort_opt_vec<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut v = Option::<Vec<String>>::deserialize(deserializer)?;
    if let Some(vec) = v.as_mut() {
        vec.sort();
    }
    Ok(v)
}

impl ClientConfig {
    pub async fn from_file<P>(path: P) -> Result<ClientConfig>
    where
        P: AsRef<Path>,
    {
        let config_str = match tokio::fs::read_to_string(&path).await {
            Ok(str) => str,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                // create default config if not found
                let config = ClientConfig::default();

                // make sure the parent directory exists
                let parent = path.as_ref().parent();
                if let Some(parent) = parent {
                    tokio::fs::create_dir_all(parent).await?;
                }

                // save the default config to file
                config.save_to_file(&path).await?;

                return Ok(config);
            }
            Err(e) => bail!("read config from {:?}: {:?}", path.as_ref(), e),
        };

        toml::from_str::<ClientConfig>(&config_str).with_context(|| "deserialize config".to_string())
    }

    pub async fn save_to_file<P>(&self, path: P) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let content = toml::to_string(self)?;
        tokio::fs::write(&path, content)
            .await
            .with_context(|| format!("write config to {:?}", path.as_ref()))
    }
}
