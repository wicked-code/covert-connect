use anyhow::{Result, anyhow, bail};
use std::sync::{Arc, OnceLock, atomic::Ordering};
use tokio::net::lookup_host;

use client::client::ClientState;

use flutter_rust_bridge::frb;

use crate::api::backend::ClientBackend;
use crate::api::wrappers::{ProtocolConfig, ServerConfig};
use client::log::{LogLine, init_trace_log};

const MAX_CONNECT_RETRY: usize = 5;
const CONNECT_RETRY_INTERVAL_MS: u64 = 1000;

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
}

impl ClientService {
    #[frb(sync)]
    pub fn new() -> ClientService {
        ClientService {
            client: Default::default(),
        }
    }

    /// Mobile (Android/iOS): run the client in-process.
    #[cfg(any(target_os = "android", target_os = "ios"))]
    pub async fn start(&self) -> Result<()> {
        use crate::api::backend::LocalBackend;
        use client::client::Client;
        use directories::ProjectDirs;

        if let Err(err) = init_trace_log("cc-client-ui") {
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

        flutter_rust_bridge::spawn(async move {
            if let Err(err) = client_instance.serve().await {
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
        use client::api::{ClientApiClient, connect_client_api};

        if let Err(err) = init_trace_log("cc-client-ui") {
            println!("Failed to initialize trace log: {:?}", err);
            bail!("Failed to initialize trace log: {:?}", err);
        }

        let show_only = std::env::args().skip(1).any(|a| a == "/show");
        if !show_only && let Err(err) = spawn_tray() {
            tracing::warn!("failed to spawn cc-tray: {:?}", err);
        }

        let try_connect = async || -> Result<ClientApiClient> {
            let mut retry_count = 0;
            Ok(loop {
                match connect_client_api().await {
                    Ok(api) => break api,
                    Err(err) => {
                        use std::time::Duration;
                        use tokio::time::sleep;

                        if retry_count >= MAX_CONNECT_RETRY {
                            bail!(
                                "Failed to connect to client API after {} attempts: {:?}",
                                retry_count,
                                err
                            );
                        }
                        retry_count += 1;
                        sleep(Duration::from_millis(CONNECT_RETRY_INTERVAL_MS)).await;
                    }
                }
            })
        };

        let first_connect = if show_only {
            try_connect().await
        } else {
            connect_client_api().await
        };

        let api = match first_connect {
            Ok(api) => api,
            Err(err) => {
                tracing::debug!("waiting connection (show_only: {}) err: {:?}", show_only, err);
                spawn_client()?;
                try_connect().await?
            }
        };

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
                    err_count: s.state.err_count.lock().value(),
                    success_count: s.state.success_count.lock().value(),
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

    pub async fn shutdown(&self) -> Result<()> {
        self.get_client()?.shutdown().await
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

    pub async fn get_log(&self, start: Option<u64>, end: Option<u64>, limit: usize) -> Result<Vec<LogLine>> {
        self.get_client()?.get_log(start, end, limit).await
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
    let dir = exe.parent().unwrap_or(exe.as_path());

    #[cfg(target_os = "macos")]
    {
        // The tray is packaged as its own helper .app bundle inside
        //   /Contents/Library/LoginItems/CovertConnectTray.app
        // so that it has a distinct CFBundleIdentifier from the parent app
        let helper_app = exe
            .ancestors()
            .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))
            .map(|app| app.join("Contents/Library/LoginItems/CovertConnectTray.app"));

        if let Some(helper) = helper_app
            && helper.exists()
        {
            Command::new("/usr/bin/open").arg("-a").arg(&helper).spawn()?;
            return Ok(());
        }

        let path = dir.join("cc-tray");
        Command::new(path).spawn()?;
        return Ok(());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let name = if cfg!(windows) { "cc-tray.exe" } else { "cc-tray" };
        let path = dir.join(name);
        Command::new(path).spawn()?;
        Ok(())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn spawn_client() -> Result<()> {
    use privesc::PrivilegedCommand;

    let exe = std::env::current_exe()?;
    let dir = exe.parent().unwrap_or(exe.as_path());
    let name = if cfg!(windows) { "cc-client.exe" } else { "cc-client" };
    let path = dir.join(name);
    let result = PrivilegedCommand::new(path).arg("install").gui(true).run()?;
    if !result.success() {
        if let Some(stderr) = result.stderr_str() {
            bail!("Failed to spawn cc-client: {}", stderr);
        }

        bail!("Failed to spawn cc-client with status: {}", result.status);
    }
    Ok(())
}
