use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize};

use crypto::config::ProtocolConfig;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,

    /// can be ip:port or host:port (port optional, 443 by default)
    pub host: String,

    /// weight of the server, avr if not set
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<u8>,

    /// the server should be used for these domains
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(deserialize_with = "sort_opt_vec")]
    pub domains: Option<Vec<String>>,

    /// the server should be used for these apps
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
