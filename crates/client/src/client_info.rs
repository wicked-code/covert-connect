use anyhow::{Result, anyhow, bail};
use std::sync::{Arc, atomic::AtomicU64};
use tokio::sync::RwLock;

use crate::config::ServerConfig;

#[derive(Default)]
pub struct ServerState {
    pub rx_total: AtomicU64,
    pub tx_total: AtomicU64,
    pub err_count: AtomicU64, // tunnels with errors i.e. zero data returned from server, used for check healthy connection
    pub succes_count: AtomicU64, // tunnels with no zero data returned from server, used for check healthy connection
}

#[derive(Clone)]
pub struct ServerInfo {
    pub config: ServerConfig,
    pub state: Arc<ServerState>,
}

pub struct ClientInfo {
    servers: RwLock<Vec<ServerInfo>>,
    direct_apps: RwLock<Vec<String>>,
    direct_domains: RwLock<Vec<String>>,
}

impl ClientInfo {
    pub fn new() -> Arc<Self> {
        Arc::new(ClientInfo {
            servers: Default::default(),
            direct_apps: Default::default(),
            direct_domains: Default::default(),
        })
    }

    pub async fn get_direct_apps(&self) -> Vec<String> {
        self.direct_apps.read().await.clone()
    }

    pub async fn add_direct_apps(&self, apps: &Vec<String>) {
        self.direct_apps.write().await.extend_from_slice(apps.as_slice());
    }

    pub async fn add_direct_domains(&self, hosts: &Vec<String>) {
        self.direct_domains.write().await.extend_from_slice(hosts.as_slice());
    }

    pub async fn get_direct_domains(&self) -> Vec<String> {
        self.direct_domains.read().await.clone()
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        if !server_host.is_empty() {
            self.remove_direct_domain(&domain).await.ok();

            let mut servers = self.servers.write().await;
            if let Some(pos) = servers.iter().position(|s| s.config.host == server_host) {
                let config = &mut ((*servers)[pos].config);
                if let Some(domains) = &mut config.domains {
                    if !domains.iter().any(|d| d == &domain) {
                        domains.push(domain);
                        domains.sort();
                    }
                } else {
                    config.domains = Some(vec![domain]);
                }
            } else {
                bail!("host not found");
            }
        } else {
            if !self.direct_domains.read().await.iter().any(|d| d == &domain) {
                self.direct_domains.write().await.push(domain.clone());
            }

            self.remove_domain_from_servers(&domain).await?;
        }
        Ok(())
    }

    async fn remove_domain_from_servers(&self, domain: &str) -> Result<()> {
        let mut servers = self.servers.write().await;
        for srv in servers.iter_mut() {
            if let Some(domains) = &mut srv.config.domains {
                if let Some(idx) = domains.iter().position(|d| d == domain) {
                    domains.remove(idx);
                }
            }
        }

        Ok(())
    }

    async fn remove_direct_domain(&self, domain: &str) -> Result<()> {
        let mut wr_domains = self.direct_domains.write().await;
        if let Some(idx) = wr_domains.iter().position(|d| d == domain) {
            wr_domains.remove(idx);
            Ok(())
        } else {
            Err(anyhow!("domain not found"))
        }
    }

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        self.remove_direct_domain(&domain).await?;
        self.remove_domain_from_servers(&domain).await
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        if !server_host.is_empty() {
            self.remove_app_internal(&app).await.ok();

            let mut servers = self.servers.write().await;
            if let Some(pos) = servers.iter().position(|s| s.config.host == server_host) {
                let config = &mut ((*servers)[pos].config);
                if let Some(apps) = &mut config.apps {
                    if !apps.iter().any(|d| d == &app) {
                        apps.push(app);
                        apps.sort();
                    }
                } else {
                    config.apps = Some(vec![app]);
                }

                Ok(())
            } else {
                Err(anyhow!("host not found"))
            }
        } else {
            if !self.direct_apps.read().await.iter().any(|d| d == &app) {
                self.direct_apps.write().await.push(app.clone());
            }

            self.remove_app_from_servers(&app).await
        }
    }

    async fn remove_app_from_servers(&self, app: &str) -> Result<()> {
        let mut servers = self.servers.write().await;
        for srv in servers.iter_mut() {
            if let Some(apps) = &mut srv.config.apps {
                if let Some(idx) = apps.iter().position(|d| d == app) {
                    apps.remove(idx);
                }
            }
        }

        Ok(())
    }

    async fn remove_app_internal(&self, app: &str) -> Result<()> {
        let mut wr_apps = self.direct_apps.write().await;
        if let Some(idx) = wr_apps.iter().position(|d| d == app) {
            wr_apps.remove(idx);

            Ok(())
        } else {
            Err(anyhow!("app not found"))
        }
    }

    pub async fn remove_app(&self, app: String) -> Result<()> {
        self.remove_app_internal(&app).await?;
        self.remove_app_from_servers(&app).await
    }

    pub async fn add_server(&self, config: ServerConfig) {
        self.servers.write().await.push(ServerInfo {
            config,
            state: Default::default(),
        });
    }

    pub async fn del_server(&self, host: &str) -> Result<usize> {
        let mut wr_servers = self.servers.write().await;
        if let Some(idx) = wr_servers.iter().position(|s| s.config.host == host) {
            wr_servers.remove(idx);
            Ok(wr_servers.len())
        } else {
            Err(anyhow!("server not found"))
        }
    }

    pub async fn set_enabled(&self, host: &str, value: bool) -> Result<()> {
        let mut wr_servers = self.servers.write().await;
        if let Some(idx) = wr_servers.iter().position(|s| s.config.host == host) {
            (*wr_servers)[idx].config.enabled = value;

            Ok(())
        } else {
            Err(anyhow!("server not found"))
        }
    }

    pub async fn update_server(&self, orig_host: &str, config: ServerConfig) -> Result<()> {
        let mut wr_servers = self.servers.write().await;
        if let Some(idx) = wr_servers.iter().position(|s| s.config.host == orig_host) {
            (*wr_servers)[idx].config = config;
            Ok(())
        } else {
            Err(anyhow!("server not found"))
        }
    }

    pub async fn get_servers(&self) -> Vec<ServerInfo> {
        self.servers.read().await.iter().cloned().collect()
    }
}
