use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, atomic::AtomicU64};
use tokio::sync::RwLock;

use crate::{config::ServerConfig, egress::Egress, server_connection_info::ServerConnectInfo};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ServerState {
    pub rx_total: AtomicU64,
    pub tx_total: AtomicU64,
    pub err_count: AtomicU64, // tunnels with errors i.e. zero data returned from server, used for check healthy connection
    pub success_count: AtomicU64, // tunnels with no zero data returned from server, used for check healthy connection
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    pub config: ServerConfig,
    pub state: Arc<ServerState>,
    pub connect_info: Option<ServerConnectInfo>,
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
        self.remove_domain(&domain).await.ok();

        if !server_host.is_empty() {
            let mut servers = self.servers.write().await;
            if let Some(pos) = servers.iter().position(|s| s.config.host == server_host) {
                let config = &mut ((*servers)[pos].config);
                if let Some(domains) = &mut config.domains {
                    domains.push(domain);
                    domains.sort();
                } else {
                    config.domains = Some(vec![domain]);
                }
            } else {
                bail!("host not found");
            }
        } else {
            self.direct_domains.write().await.push(domain.clone());
        }

        Ok(())
    }

    async fn remove_domain_from_servers(&self, domain: &str) -> bool {
        let mut deleted = false;
        let mut servers = self.servers.write().await;
        for srv in servers.iter_mut() {
            if let Some(domains) = &mut srv.config.domains
                && let Some(idx) = domains.iter().position(|d| d == domain)
            {
                domains.remove(idx);
                deleted = true;
            }
        }

        deleted
    }

    async fn remove_direct_domain(&self, domain: &str) -> bool {
        let mut wr_domains = self.direct_domains.write().await;
        if let Some(idx) = wr_domains.iter().position(|d| d == domain) {
            wr_domains.remove(idx);
            true
        } else {
            false
        }
    }

    pub async fn remove_domain(&self, domain: &str) -> Result<()> {
        let mut deleted = self.remove_direct_domain(domain).await;
        deleted |= self.remove_domain_from_servers(domain).await;
        if deleted {
            Ok(())
        } else {
            Err(anyhow!("Domain not found"))
        }
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        self.remove_app(&app).await.ok();

        if !server_host.is_empty() {
            let mut servers = self.servers.write().await;
            if let Some(pos) = servers.iter().position(|s| s.config.host == server_host) {
                let config = &mut ((*servers)[pos].config);
                if let Some(apps) = &mut config.apps {
                    apps.push(app);
                    apps.sort();
                } else {
                    config.apps = Some(vec![app]);
                }
            } else {
                bail!("host not found");
            }
        } else {
            self.direct_apps.write().await.push(app.clone());
        }

        Ok(())
    }

    async fn remove_app_from_servers(&self, app: &str) -> bool {
        let mut deleted = false;
        let mut servers = self.servers.write().await;
        for srv in servers.iter_mut() {
            if let Some(apps) = &mut srv.config.apps
                && let Some(idx) = apps.iter().position(|d| d == app)
            {
                apps.remove(idx);
                deleted = true;
            }
        }

        deleted
    }

    async fn remove_app_internal(&self, app: &str) -> bool {
        let mut wr_apps = self.direct_apps.write().await;
        if let Some(idx) = wr_apps.iter().position(|d| d == app) {
            wr_apps.remove(idx);
            true
        } else {
            false
        }
    }

    pub async fn remove_app(&self, app: &str) -> Result<()> {
        let mut deleted = self.remove_app_internal(app).await;
        deleted |= self.remove_app_from_servers(app).await;
        if deleted { Ok(()) } else { Err(anyhow!("App not found")) }
    }

    pub async fn add_server(&self, config: ServerConfig, egress: &Arc<Egress>) {
        let connect_info = self.new_connection_info(&config, egress).await;
        self.servers.write().await.push(ServerInfo {
            config,
            state: Default::default(),
            connect_info,
        });
    }

    pub async fn update_connection_info(&self, egress: &Arc<Egress>) {
        let mut servers = self.servers.write().await;
        for srv in servers.iter_mut() {
            if srv.connect_info.is_none() {
                srv.connect_info = self.new_connection_info(&srv.config, egress).await;
            }
        }
    }

    async fn new_connection_info(&self, config: &ServerConfig, egress: &Arc<Egress>) -> Option<ServerConnectInfo> {
        match ServerConnectInfo::new(&config.host, &config.protocol.key, egress).await {
            Ok(info) => Some(info),
            Err(err) => {
                tracing::error!("{:?}", err);
                None
            }
        }
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

    pub async fn update_server(&self, orig_host: &str, config: ServerConfig, egress: &Arc<Egress>) -> Result<()> {
        let connection_info = self.new_connection_info(&config, egress).await;
        let mut wr_servers = self.servers.write().await;
        if let Some(ref mut item) = wr_servers.iter_mut().find(|s| s.config.host == orig_host) {
            item.config = config;
            item.connect_info = connection_info;
            Ok(())
        } else {
            Err(anyhow!("server not found"))
        }
    }

    pub async fn get_servers(&self) -> Vec<ServerInfo> {
        self.servers.read().await.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use crypto::{cipher::CipherType, config::ProtocolConfig, kdf::Kdf};

    use super::*;

    #[tokio::test]
    async fn test_direct_apps() {
        let info = ClientInfo::new();
        info.add_direct_apps(&vec!["app1".to_string(), "app2".to_string()])
            .await;
        assert_eq!(
            info.get_direct_apps().await,
            vec!["app1".to_string(), "app2".to_string()]
        );
        info.set_app("app3".to_string(), "".to_string()).await.unwrap();
        assert_eq!(
            info.get_direct_apps().await,
            vec!["app1".to_string(), "app2".to_string(), "app3".to_string()]
        );
        info.remove_app("app2").await.unwrap();
        assert_eq!(
            info.get_direct_apps().await,
            vec!["app1".to_string(), "app3".to_string()]
        );
    }

    #[tokio::test]
    async fn test_server_selected_apps() {
        let info = ClientInfo::new();
        info.add_direct_apps(&vec!["app1".to_string(), "app2".to_string()])
            .await;
        assert_eq!(
            info.get_direct_apps().await,
            vec!["app1".to_string(), "app2".to_string()]
        );
        add_server(&info, "server1").await;
        add_server(&info, "server2").await;
        info.set_app("app3".to_string(), "server1".to_string()).await.unwrap();
        assert_eq!(
            get_server_apps(&info, "server1").await.unwrap(),
            vec!["app3".to_string()]
        );
        info.set_app("app4".to_string(), "server1".to_string()).await.unwrap();
        assert_eq!(
            get_server_apps(&info, "server1").await.unwrap(),
            vec!["app3".to_string(), "app4".to_string()]
        );
        info.set_app("app5".to_string(), "server2".to_string()).await.unwrap();
        assert_eq!(
            get_server_apps(&info, "server2").await.unwrap(),
            vec!["app5".to_string()]
        );
        info.remove_app("app4").await.unwrap();
        assert_eq!(
            get_server_apps(&info, "server1").await.unwrap(),
            vec!["app3".to_string()]
        );
        info.remove_app("app5").await.unwrap();
        assert_eq!(
            get_server_apps(&info, "server2").await.unwrap_or_default(),
            Vec::<String>::new()
        );
        info.remove_app("app2").await.unwrap();
        assert_eq!(info.get_direct_apps().await, vec!["app1".to_string()]);
    }

    #[tokio::test]
    async fn test_direct_domains() {
        let info = ClientInfo::new();
        info.add_direct_domains(&vec!["test1.com".to_string(), "test2.com".to_string()])
            .await;
        assert_eq!(
            info.get_direct_domains().await,
            vec!["test1.com".to_string(), "test2.com".to_string()]
        );
        info.set_domain("test3.com".to_string(), "".to_string()).await.unwrap();
        assert_eq!(
            info.get_direct_domains().await,
            vec![
                "test1.com".to_string(),
                "test2.com".to_string(),
                "test3.com".to_string()
            ]
        );
        info.remove_domain("test2.com").await.unwrap();
        assert_eq!(
            info.get_direct_domains().await,
            vec!["test1.com".to_string(), "test3.com".to_string()]
        );
    }

    #[tokio::test]
    async fn test_server_selected_domains() {
        let info = ClientInfo::new();
        info.add_direct_domains(&vec!["test1.com".to_string(), "test2.com".to_string()])
            .await;
        assert_eq!(
            info.get_direct_domains().await,
            vec!["test1.com".to_string(), "test2.com".to_string()]
        );
        add_server(&info, "server1").await;
        add_server(&info, "server2").await;
        info.set_domain("test3.com".to_string(), "server1".to_string())
            .await
            .unwrap();
        assert_eq!(
            get_server_domains(&info, "server1").await.unwrap(),
            vec!["test3.com".to_string()]
        );
        info.set_domain("test4.com".to_string(), "server1".to_string())
            .await
            .unwrap();
        assert_eq!(
            get_server_domains(&info, "server1").await.unwrap(),
            vec!["test3.com".to_string(), "test4.com".to_string()]
        );
        info.set_domain("test5.com".to_string(), "server2".to_string())
            .await
            .unwrap();
        assert_eq!(
            get_server_domains(&info, "server2").await.unwrap(),
            vec!["test5.com".to_string()]
        );
        info.remove_domain("test4.com").await.unwrap();
        assert_eq!(
            get_server_domains(&info, "server1").await.unwrap(),
            vec!["test3.com".to_string()]
        );
        info.remove_domain("test5.com").await.unwrap();
        assert_eq!(
            get_server_domains(&info, "server2").await.unwrap_or_default(),
            Vec::<String>::new()
        );
        info.remove_domain("test2.com").await.unwrap();
        assert_eq!(info.get_direct_domains().await, vec!["test1.com".to_string()]);
    }

    async fn get_server_apps(info: &ClientInfo, server: &str) -> Result<Vec<String>> {
        info.get_servers()
            .await
            .iter()
            .find(|s| s.config.host == server)
            .ok_or_else(|| anyhow!("server not found"))?
            .config
            .apps
            .clone()
            .ok_or_else(|| anyhow!("apps not found"))
    }

    async fn get_server_domains(info: &ClientInfo, server: &str) -> Result<Vec<String>> {
        info.get_servers()
            .await
            .iter()
            .find(|s| s.config.host == server)
            .ok_or_else(|| anyhow!("server not found"))?
            .config
            .domains
            .clone()
            .ok_or_else(|| anyhow!("domains not found"))
    }

    async fn add_server(info: &ClientInfo, host: &str) {
        info.add_server(
            ServerConfig {
                caption: None,
                host: host.to_string(),
                domains: None,
                apps: None,
                weight: None,
                enabled: true,
                protocol: new_protocol_config(),
            },
            &Egress::new(),
        )
        .await
    }

    fn new_protocol_config() -> ProtocolConfig {
        ProtocolConfig {
            key: "testkey".to_string(),
            kdf: Kdf::Blake3,
            cipher: CipherType::Aes256Gcm,
            data_padding: Default::default(),
            max_connect_delay: 10000,
            header_padding: 50..777,
            encryption_limit: usize::MAX,
        }
    }

    #[tokio::test]
    async fn test_del_server() {
        let info = ClientInfo::new();
        add_server(&info, "server1").await;
        add_server(&info, "server2").await;
        assert_eq!(info.get_servers().await.len(), 2);

        let remaining = info.del_server("server1").await.unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(info.get_servers().await[0].config.host, "server2");

        let remaining = info.del_server("server2").await.unwrap();
        assert_eq!(remaining, 0);

        assert!(info.del_server("nonexistent").await.is_err());
    }

    #[tokio::test]
    async fn test_set_enabled() {
        let info = ClientInfo::new();
        add_server(&info, "server1").await;
        assert!(info.get_servers().await[0].config.enabled);

        info.set_enabled("server1", false).await.unwrap();
        assert!(!info.get_servers().await[0].config.enabled);

        info.set_enabled("server1", true).await.unwrap();
        assert!(info.get_servers().await[0].config.enabled);

        assert!(info.set_enabled("nonexistent", false).await.is_err());
    }

    #[tokio::test]
    async fn test_update_server() {
        let info = ClientInfo::new();
        add_server(&info, "server1").await;

        let new_config = ServerConfig {
            caption: Some("Updated".to_string()),
            host: "server1-new".to_string(),
            domains: Some(vec!["example.com".to_string()]),
            apps: None,
            weight: Some(5),
            enabled: false,
            protocol: new_protocol_config(),
        };
        info.update_server("server1", new_config, &Egress::new()).await.unwrap();

        let servers = info.get_servers().await;
        assert_eq!(servers[0].config.host, "server1-new");
        assert_eq!(servers[0].config.caption, Some("Updated".to_string()));
        assert!(!servers[0].config.enabled);

        assert!(
            info.update_server(
                "nonexistent",
                ServerConfig {
                    caption: None,
                    host: "x".to_string(),
                    domains: None,
                    apps: None,
                    weight: None,
                    enabled: true,
                    protocol: new_protocol_config(),
                },
                &Egress::new(),
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn test_error_set_app_nonexistent_host() {
        let info = ClientInfo::new();
        assert!(
            info.set_app("app1".to_string(), "nonexistent".to_string())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_error_set_domain_nonexistent_host() {
        let info = ClientInfo::new();
        assert!(
            info.set_domain("test.com".to_string(), "nonexistent".to_string())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_remove_nonexistent_app_and_domain() {
        let info = ClientInfo::new();
        assert!(info.remove_app("nope").await.is_err());
        assert!(info.remove_domain("nope.com").await.is_err());
    }

    #[tokio::test]
    async fn test_move_domain_between_servers() {
        let info = ClientInfo::new();
        add_server(&info, "server1").await;
        add_server(&info, "server2").await;

        info.set_domain("test.com".to_string(), "server1".to_string())
            .await
            .unwrap();
        assert_eq!(
            get_server_domains(&info, "server1").await.unwrap(),
            vec!["test.com".to_string()]
        );

        // Move domain from server1 to server2
        info.set_domain("test.com".to_string(), "server2".to_string())
            .await
            .unwrap();
        assert_eq!(
            get_server_domains(&info, "server1").await.unwrap(),
            Vec::<String>::new()
        ); // removed from server1
        assert_eq!(
            get_server_domains(&info, "server2").await.unwrap(),
            vec!["test.com".to_string()]
        );

        // Move domain from server2 to direct
        info.set_domain("test.com".to_string(), "".to_string()).await.unwrap();
        assert_eq!(
            get_server_domains(&info, "server2").await.unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(info.get_direct_domains().await, vec!["test.com".to_string()]);
    }

    #[tokio::test]
    async fn test_move_app_between_servers() {
        let info = ClientInfo::new();
        add_server(&info, "server1").await;
        add_server(&info, "server2").await;

        info.set_app("myapp".to_string(), "server1".to_string()).await.unwrap();
        assert_eq!(
            get_server_apps(&info, "server1").await.unwrap(),
            vec!["myapp".to_string()]
        );

        // Move app from server1 to server2
        info.set_app("myapp".to_string(), "server2".to_string()).await.unwrap();
        assert_eq!(get_server_apps(&info, "server1").await.unwrap(), Vec::<String>::new());
        assert_eq!(
            get_server_apps(&info, "server2").await.unwrap(),
            vec!["myapp".to_string()]
        );

        // Move app from server2 to direct
        info.set_app("myapp".to_string(), "".to_string()).await.unwrap();
        assert_eq!(get_server_apps(&info, "server2").await.unwrap(), Vec::<String>::new());
        assert_eq!(info.get_direct_apps().await, vec!["myapp".to_string()]);
    }
}
