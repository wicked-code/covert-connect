use anyhow::{Result, anyhow, bail};
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use std::env;
use std::sync::{Arc, OnceLock, atomic::Ordering};
use tokio::net::lookup_host;

use client::client::Client;

pub use client::client::ClientState;

use flutter_rust_bridge::frb;

use crate::api::log::{LogLine, get_trace_log, init_trace_log};
use crate::api::wrappers::{ProtocolConfig, ServerConfig};

#[derive(Clone)]
pub struct ClientConfig {
    pub state: ClientState,
    pub direct_domains: Vec<String>,
    pub direct_apps: Vec<String>,
    pub servers: Vec<ServerConfig>,
}

#[derive(Clone)]
pub struct ClientStatus {
    pub initialized: bool,
    pub working: bool,
    pub servers: Vec<ServerInfo>,
}

#[derive(Clone)]
pub struct ServerInfo {
    pub state: ServerState,
    pub config: ServerConfig,
    pub ip: Option<String>,
    pub port: Option<u16>,
}

#[derive(Clone)]
pub struct ServerState {
    pub rx_total: u64,
    pub tx_total: u64,
    pub err_count: u64,
    pub success_count: u64,
}

pub struct ClientService {
    /// flutter_rust_bridge:ignore
    client: OnceLock<Arc<Client>>,
}

impl ClientService {
    #[frb(sync)]
    pub fn new() -> ClientService {
        return {
            ClientService {
                client: Default::default(),
            }
        };
    }

    pub async fn start(&self, cfg: ClientConfig) -> Result<()> {
        if let Err(err) = init_trace_log() {
            println!("Failed to initialize trace log: {:?}", err);
            bail!("Failed to initialize trace log: {:?}", err);
        }

        let mut client_state = cfg.state;
        if cfg.servers.is_empty() {
            // turn off proxy if no servers
            client_state = ClientState::Off;
        }

        let client_instance = Client::new(client_state);
        client_instance.initialize().await?;
        self.client
            .set(client_instance.clone())
            .map_err(|_| anyhow!("client already initialized"))?;

        flutter_rust_bridge::spawn(async move {
            client_instance.add_direct_apps(&cfg.direct_apps).await;
            client_instance.add_direct_domains(&cfg.direct_domains).await;

            for srv in cfg.servers {
                client_instance.add_server(srv.into()).await;
            }

            if let Err(err) = client_instance.serve().await {
                tracing::error!("serve: {:?}", err);
            }
        });

        Ok(())
    }

    pub async fn get_config(&self) -> Result<ClientConfig> {
        let client = self.get_client()?;

        return Ok(ClientConfig {
            state: client.get_state().await,
            direct_domains: client.get_direct_domains().await,
            direct_apps: client.get_direct_apps().await,
            servers: client
                .get_servers()
                .await
                .into_iter()
                .map(|srv| srv.config.into())
                .collect(),
        });
    }

    pub async fn get_status(&self) -> Result<ClientStatus> {
        let client = self.get_client()?;

        let servers = client
            .get_servers()
            .await
            .iter()
            .map(|s| ServerInfo {
                state: ServerState {
                    rx_total: s.state.rx_total.load(Ordering::Relaxed),
                    tx_total: s.state.tx_total.load(Ordering::Relaxed),
                    err_count: s.state.err_count.load(Ordering::Relaxed),
                    success_count: s.state.success_count.load(Ordering::Relaxed),
                },
                config: s.config.clone().into(),
                ip: s.connect_info.as_ref().map(|info| info.address.ip().to_string()),
                port: s.connect_info.as_ref().map(|info| info.address.port()),
            })
            .collect();

        Ok(ClientStatus {
            initialized: client.is_initialized(),
            working: client.is_working(),
            servers,
        })
    }

    pub async fn get_state(&self) -> Result<ClientState> {
        Ok(self.get_client()?.get_state().await)
    }

    pub async fn set_state(&self, state: ClientState) -> Result<()> {
        self.get_client()?.set_state(state).await
    }

    pub async fn set_server_enabled(&self, host: String, value: bool) -> Result<()> {
        self.get_client()?.set_enabled(&host, value).await
    }

    pub async fn get_server_protocol(&self, server: String, key: String) -> Result<ProtocolConfig> {
        let protocol = self.get_client()?.get_server_protocol(&server, &key).await?;
        Ok(protocol.into())
    }

    pub async fn stop(&self) -> Result<()> {
        self.get_client()?.set_state(ClientState::Off).await
    }

    pub async fn get_direct_apps(&self) -> Result<Vec<String>> {
        Ok(self.get_client()?.get_direct_apps().await)
    }

    pub async fn get_direct_domains(&self) -> Result<Vec<String>> {
        Ok(self.get_client()?.get_direct_domains().await)
    }

    pub async fn add_server(&self, config: ServerConfig) -> Result<()> {
        self.get_client()?.add_server(config.into()).await;
        Ok(())
    }

    pub async fn update_server(&self, orig_host: String, new_config: ServerConfig) -> Result<()> {
        self.get_client()?.update_server(&orig_host, new_config.into()).await
    }

    pub async fn delete_server(&self, host: String) -> Result<()> {
        self.get_client()?.del_server(&host).await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        let client = self.get_client()?;
        client.set_domain(domain, server_host).await
    }

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        let client = self.get_client()?;
        client.remove_domain(domain).await
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        let client = self.get_client()?;
        client.set_app(app, server_host).await
    }

    pub async fn remove_app(&self, app: String) -> Result<()> {
        let client = self.get_client()?;
        client.remove_app(app).await
    }

    pub async fn get_log(start: Option<u64>, end: Option<u64>, limit: usize) -> Result<Vec<LogLine>> {
        get_trace_log(start, end, limit).await
    }

    pub async fn check_domain(domain: String) -> Result<bool> {
        if let Ok(mut res) = lookup_host(domain).await {
            Ok(res.next().is_some())
        } else {
            Ok(false)
        }
    }

    pub async fn get_ttfb(&self, server: String, domain: String) -> Result<u32> {
        let client = self.get_client()?;
        Ok(client.get_ttfb(&server, &domain).await? as u32)
    }

    pub async fn get_autostart() -> Result<bool> {
        let auto = ClientService::init_autostart()?;
        Ok(auto.is_enabled()?)
    }

    pub async fn set_autostart(enabled: bool) -> Result<()> {
        let auto = ClientService::init_autostart()?;
        if enabled {
            auto.enable()?;
        } else {
            auto.disable()?;
        }
        if auto.is_enabled()? == enabled {
            Ok(())
        } else {
            Err(anyhow!("failed to set autostart"))
        }
    }

    fn init_autostart() -> Result<AutoLaunch> {
        let path = env::current_exe().map_err(|e| anyhow!("failed to get current exe path: {e}"))?;
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow!("failed to convert path to string"))?;
        let auto = AutoLaunchBuilder::new()
            .set_app_name(&format!("covert-connect-{}", env!("CARGO_PKG_VERSION")))
            .set_app_path(path_str)
            .set_use_launch_agent(true)
            .build()?;
        Ok(auto)
    }

    fn get_client(&self) -> Result<&Arc<Client>> {
        self.client.get().ok_or_else(|| anyhow!("client not initialized"))
    }
}

#[frb(mirror(ClientState))]
pub enum _ClientState {
    Smart,
    All,
    Off,
}
