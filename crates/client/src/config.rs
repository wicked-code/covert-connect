use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, str::FromStr};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::net::lookup_host;

use crypto::config::ProtocolConfig;
use crypto::kdf::Kdf;

const HTTPS_PORT: &str = ":443";

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
    pub domains: Option<Vec<String>>,

    /// the server should be used for these apps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps: Option<Vec<String>>,

    /// enabled if None
    #[serde(skip_serializing_if = "is_true")]
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// protocol configuration
    pub protocol: ProtocolConfig,

    #[serde(skip_deserializing, skip_serializing)] 
    #[serde(default = "default_server_address")]
    pub address: SocketAddr,

    #[serde(skip_deserializing, skip_serializing)] 
    pub url_path: Option<String>,
}

pub struct ServerConnectConfig {
    pub host: String,
    pub address: SocketAddr,
    pub url_path: Option<String>,
}

impl ServerConnectConfig {
    pub async fn new(host: &str, key: &str) -> Result<ServerConnectConfig> {
        let mut host = host.to_owned();
        Ok(if let Ok(address) = SocketAddr::from_str(&host) {
            ServerConnectConfig {
                host,
                address,
                url_path: None,
            }
        } else {
            let mut server_host = host.clone();
            let add_path = match host.rfind(':') {
                Some(pos) => {
                    if &host[pos..] == HTTPS_PORT {
                        host.truncate(pos);
                        true
                    } else {
                        false
                    }
                }, 
                None => {
                    server_host += HTTPS_PORT;
                    true
                }
            };
    
            let address = lookup_host(&server_host)
                .await?
                .next()
                .ok_or_else(|| anyhow!("ip:port or host:port required, found : {}", &server_host))?;
    
            let url_path = if add_path {
                Some(Kdf::derive_url_path(key)?)
            } else {
                None
            };

            ServerConnectConfig {
                host,
                address,
                url_path,
            }
        })
    }
}

impl ServerConfig {
    pub async fn init(&mut self) -> Result<()> {
        let conn_cfg = ServerConnectConfig::new(&self.host, &self.protocol.key).await?;
        if self.protocol.encryption_limit == 0 && conn_cfg.url_path.is_none() {
            anyhow::bail!(
                "Zero encryption_limit is not allowed without https tunnel. \
                Remove encyption_limit or use https to connect to the server."
            );
        }

        self.address = conn_cfg.address;
        self.host = conn_cfg.host;
        self.url_path = conn_cfg.url_path;

        if let Some(domains) = &mut self.domains {
            domains.sort();
        }

        if let Some(apps) = &mut self.apps {
            apps.sort();
        }

        Ok(())
    }
}

fn is_true(val: &bool) -> bool {
    *val
}

fn default_enabled() -> bool {
    true
}

pub fn default_server_address() -> SocketAddr {
    // defined in init
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0u16)
}
