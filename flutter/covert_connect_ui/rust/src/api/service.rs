use anyhow::{Result, anyhow, bail};
use parking_lot::Mutex;
use std::sync::{Arc, OnceLock, atomic::Ordering};
use tokio::net::lookup_host;
use tokio_util::sync::CancellationToken;

use client::client::ClientState;

use flutter_rust_bridge::frb;

use crate::api::backend::ClientBackend;
use crate::api::wrappers::{ProtocolConfig, ServerConfig};
use client::log::{LogLine, get_trace_log, init_trace_log};

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
    client: OnceLock<Arc<dyn ClientBackend>>,
    cancel_token: Mutex<CancellationToken>,
}

impl ClientService {
    #[frb(sync)]
    pub fn new() -> ClientService {
        ClientService {
            client: Default::default(),
            cancel_token: Mutex::new(CancellationToken::new()),
        }
    }

    /// Mobile (Android/iOS): run the client in-process.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub async fn start(&self) -> Result<()> {
        use crate::api::backend::LocalBackend;
        use client::client::Client;
        use directories::ProjectDirs;

        if let Err(err) = init_trace_log() {
            println!("Failed to initialize trace log: {:?}", err);
            bail!("Failed to initialize trace log: {:?}", err);
        }

        let dirs = ProjectDirs::from("com", "wicked-code", "covert-connect")
            .ok_or_else(|| anyhow!("Failed to get config directory"))?;

        let client_instance = Client::new(dirs.config_dir().to_path_buf().join("config.toml"));
        client_instance.initialize().await?;

        self.client
            .set(Arc::new(LocalBackend(client_instance.clone())))
            .map_err(|_| anyhow!("client already initialized"))?;

        let cancel_token = self.cancel_token.lock().clone();
        flutter_rust_bridge::spawn(async move {
            if let Err(err) = client_instance.serve(cancel_token).await {
                tracing::error!("serve: {:?}", err);
            }
        });

        Ok(())
    }

    /// Desktop: spawn cc-tray (unless `/show` was passed) and connect to the
    /// running cc-client over its IPC channel.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    pub async fn start(&self) -> Result<()> {
        use crate::api::backend::RemoteBackend;
        use client::api::connect_client_api;

        if let Err(err) = init_trace_log() {
            println!("Failed to initialize trace log: {:?}", err);
            bail!("Failed to initialize trace log: {:?}", err);
        }

        let show_only = std::env::args().skip(1).any(|a| a == "/show");
        if !show_only {
            if let Err(err) = spawn_tray() {
                tracing::warn!("failed to spawn cc-tray: {:?}", err);
            }
        }

        let api = connect_client_api().await?;
        self.client
            .set(Arc::new(RemoteBackend(api)))
            .map_err(|_| anyhow!("client already initialized"))?;

        Ok(())
    }

    pub async fn get_config(&self) -> Result<ClientConfig> {
        let client = self.get_client()?;

        Ok(ClientConfig {
            state: client.get_state().await?,
            direct_domains: client.get_direct_domains().await?,
            direct_apps: client.get_direct_apps().await?,
            servers: client
                .get_servers()
                .await?
                .into_iter()
                .map(|srv| srv.config.into())
                .collect(),
        })
    }

    pub async fn get_status(&self) -> Result<ClientStatus> {
        let client = self.get_client()?;

        let servers = client
            .get_servers()
            .await?
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
            initialized: client.is_initialized().await?,
            working: client.is_working().await?,
            servers,
        })
    }

    pub async fn get_state(&self) -> Result<ClientState> {
        self.get_client()?.get_state().await
    }

    pub async fn set_state(&self, state: ClientState) -> Result<()> {
        self.get_client()?.set_state(state).await
    }

    pub async fn set_server_enabled(&self, host: String, value: bool) -> Result<()> {
        self.get_client()?.set_enabled(host, value).await
    }

    pub async fn get_server_protocol(&self, server: String, key: String) -> Result<ProtocolConfig> {
        let protocol = self.get_client()?.get_server_protocol(server, key).await?;
        Ok(protocol.into())
    }

    pub async fn stop(&self) -> Result<()> {
        self.cancel_token.lock().cancel();
        *self.cancel_token.lock() = CancellationToken::new();
        self.get_client()?.set_state(ClientState::Off).await
    }

    pub async fn get_direct_apps(&self) -> Result<Vec<String>> {
        self.get_client()?.get_direct_apps().await
    }

    pub async fn get_direct_domains(&self) -> Result<Vec<String>> {
        self.get_client()?.get_direct_domains().await
    }

    pub async fn add_server(&self, config: ServerConfig) -> Result<()> {
        self.get_client()?.add_server(config.into()).await
    }

    pub async fn update_server(&self, orig_host: String, new_config: ServerConfig) -> Result<()> {
        self.get_client()?.update_server(orig_host, new_config.into()).await
    }

    pub async fn delete_server(&self, host: String) -> Result<()> {
        self.get_client()?.del_server(host).await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        self.get_client()?.set_domain(domain, server_host).await
    }

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        self.get_client()?.remove_domain(domain).await
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        self.get_client()?.set_app(app, server_host).await
    }

    pub async fn remove_app(&self, app: String) -> Result<()> {
        self.get_client()?.remove_app(app).await
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
        Ok(client.get_ttfb(server, domain).await? as u32)
    }

    fn get_client(&self) -> Result<&Arc<dyn ClientBackend>> {
        self.client.get().ok_or_else(|| anyhow!("client not initialized"))
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn spawn_tray() -> Result<()> {
    use std::process::Command;

    let exe = std::env::current_exe()?;
    let dir = exe.parent().unwrap_or_else(|| exe.as_path());
    let name = if cfg!(windows) { "cc-tray.exe" } else { "cc-tray" };
    let path = dir.join(name);
    Command::new(path).spawn()?;
    Ok(())
}
