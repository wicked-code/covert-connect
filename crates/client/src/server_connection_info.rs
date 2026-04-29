use std::{net::SocketAddr, str::FromStr, sync::Arc};
use serde::{Serialize, Deserialize};

use anyhow::{Result, anyhow};
use crypto::kdf::Kdf;

use crate::egress::Egress;

const HTTPS_PORT_STR: &str = ":443";
const HTTPS_PORT: u16 = 443;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConnectInfo {
    pub address: SocketAddr,
    pub url_path: Option<String>,
}

impl ServerConnectInfo {
    pub async fn new(host: &str, key: &str, egress: &Arc<Egress>) -> Result<ServerConnectInfo> {
        let mut host = host.to_owned();
        Ok(if let Ok(address) = SocketAddr::from_str(&host) {
            ServerConnectInfo {
                address,
                url_path: None,
            }
        } else {
            let mut port = HTTPS_PORT;
            if let Some(pos) = host.rfind(':') {
                if &host[pos..] != HTTPS_PORT_STR {
                    match host[pos + 1..].parse::<u16>() {
                        Ok(p) => port = p,
                        Err(err) => {
                            return Err(anyhow!("invalid port in host {}, {}", host, err));
                        }
                    }
                }
                host.truncate(pos);
            };

            let address = egress
                .lookup_host(&host)
                .await?
                .ok_or_else(|| anyhow!("server host not found: {}", &host))?;

            let address = SocketAddr::new(address, port);

            let url_path = if port == HTTPS_PORT {
                Some(Kdf::derive_url_path(key)?)
            } else {
                None
            };

            ServerConnectInfo { address, url_path }
        })
    }
}
