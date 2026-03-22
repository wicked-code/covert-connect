use anyhow::{Result, anyhow, bail};
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use std::env;
use std::sync::{Arc, OnceLock, atomic::Ordering};
use tokio::net::lookup_host;

use client::config::ServerConfig as ClientServerConfig;
use client::router::Router;

pub use client::router::RouterState;

use flutter_rust_bridge::{DartFnFuture, frb};

use crate::api::log::{LogLine, WriterNotifier, get_trace_log, init_trace_log};
use crate::api::wrappers::{ProtocolConfig, ServerConfig};

#[derive(Clone)]
pub struct RouterConfig {
    pub state: RouterState,
    pub direct_domains: Vec<String>,
    pub direct_apps: Vec<String>,
    pub servers: Vec<ServerConfig>,
}

#[derive(Clone)]
pub struct RouterStatus {
    pub initialized: bool,
    pub servers: Vec<ServerInfo>,
}

#[derive(Clone)]
pub struct ServerInfo {
    pub state: ServerState,
    pub config: ServerConfig,
    pub ip: String,
    pub port: u16,
}

#[derive(Clone)]
pub struct ServerState {
    pub rx_total: u64,
    pub tx_total: u64,
    pub err_count: u64,
    pub succes_count: u64,
}

pub struct RouterService {
    /// flutter_rust_bridge:ignore
    router: OnceLock<Arc<Router>>,
    /// flutter_rust_bridge:ignore
    writer_notifier: OnceLock<Arc<WriterNotifier>>,
}

impl RouterService {
    #[frb(sync)]
    pub fn new() -> RouterService {
        return {
            RouterService {
                router: Default::default(),
                writer_notifier: OnceLock::new(),
            }
        };
    }

    pub async fn start(&self, cfg: RouterConfig) -> Result<()> {
        match init_trace_log() {
            Ok(notifier) => {
                self.writer_notifier.set(notifier).ok();
            }
            Err(e) => {
                println!("Failed to initialize trace log: {:?}", e);
                bail!("Failed to initialize trace log: {:?}", e);
            }
        }

        let mut router_state = cfg.state;
        if cfg.servers.is_empty() {
            // turn off proxy if no servers
            router_state = RouterState::Off;
        }

        let router_instance = Router::new(router_state)?;
        self.router
            .set(router_instance.clone())
            .map_err(|_| anyhow!("router already initialized"))?;

        flutter_rust_bridge::spawn(async move {
            router_instance.add_direct_apps(&cfg.direct_apps).await;
            router_instance.add_direct_domains(&cfg.direct_domains).await;

            for srv in cfg.servers {
                let mut cfg: ClientServerConfig = srv.into();
                cfg.init()
                    .await
                    .inspect_err(|e| tracing::error!("config: {:?}", e))
                    .ok();
                router_instance.add_server(cfg).await;
            }

            if let Err(err) = router_instance.serve().await {
                tracing::error!("serve: {:?}", err);
            }
        });

        Ok(())
    }

    pub async fn get_config(&self) -> Result<RouterConfig> {
        let router = self.get_router()?;

        return Ok(RouterConfig {
            state: router.get_state().await,
            direct_domains: router.get_direct_domains().await,
            direct_apps: router.get_direct_apps().await,
            servers: router
                .get_servers()
                .await
                .into_iter()
                .map(|srv| srv.config.into())
                .collect(),
        });
    }

    pub async fn get_status(&self) -> Result<RouterStatus> {
        let proxy = self.get_router()?;

        let servers = proxy
            .get_servers()
            .await
            .iter()
            .map(|s| ServerInfo {
                state: ServerState {
                    rx_total: s.state.rx_total.load(Ordering::Relaxed),
                    tx_total: s.state.tx_total.load(Ordering::Relaxed),
                    err_count: s.state.err_count.load(Ordering::Relaxed),
                    succes_count: s.state.succes_count.load(Ordering::Relaxed),
                },
                config: s.config.clone().into(),
                ip: s.config.address.ip().to_string(),
                port: s.config.address.port(),
            })
            .collect();

        Ok(RouterStatus {
            initialized: proxy.is_initialized(),
            servers,
        })
    }

    pub async fn get_state(&self) -> Result<RouterState> {
        Ok(self.get_router()?.get_state().await)
    }

    pub async fn set_state(&self, state: RouterState) -> Result<()> {
        self.get_router()?.set_state(state).await
    }

    pub async fn set_server_enabled(&self, host: String, value: bool) -> Result<()> {
        self.get_router()?.set_enabled(&host, value).await
    }

    pub async fn get_server_protocol(&self, server: String, key: String) -> Result<ProtocolConfig> {
        let protocol = self.get_router()?.get_server_protocol(&server, &key).await?;
        Ok(protocol.into())
    }

    pub async fn stop(&self) -> Result<()> {
        self.get_router()?.set_state(RouterState::Off).await
    }

    pub async fn get_direct_apps(&self) -> Result<Vec<String>> {
        Ok(self.get_router()?.get_direct_apps().await)
    }

    pub async fn get_direct_domains(&self) -> Result<Vec<String>> {
        Ok(self.get_router()?.get_direct_domains().await)
    }

    pub async fn add_server(&self, config: ServerConfig) -> Result<()> {
        let mut cfg: ClientServerConfig = config.into();
        cfg.init()
            .await
            .inspect_err(|e| tracing::error!("config init: {:?}", e))
            .ok();
        self.get_router()?.add_server(cfg).await;
        Ok(())
    }

    pub async fn update_server(&self, orig_host: String, new_config: ServerConfig) -> Result<()> {
        let mut cfg: ClientServerConfig = new_config.into();
        cfg.init()
            .await
            .inspect_err(|e| tracing::error!("config init: {:?}", e))
            .ok();
        self.get_router()?.update_server(&orig_host, cfg).await
    }

    pub async fn delete_server(&self, host: String) -> Result<()> {
        self.get_router()?.del_server(&host).await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        let proxy = self.get_router()?;
        proxy.set_domain(domain, server_host).await
    }

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        let proxy = self.get_router()?;
        proxy.remove_domain(domain).await
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        let proxy = self.get_router()?;
        proxy.set_app(app, server_host).await
    }

    pub async fn remove_app(&self, app: String) -> Result<()> {
        let proxy = self.get_router()?;
        proxy.remove_app(app).await
    }

    pub async fn log(message: String) {
        tracing::info!(message);
    }

    pub async fn get_log(start: Option<u64>, limit: usize) -> Result<Vec<LogLine>> {
        get_trace_log(start, limit).await
    }

    pub async fn check_domain(domain: String) -> Result<bool> {
        if let Ok(mut res) = lookup_host(domain).await {
            Ok(res.next().is_some())
        } else {
            Ok(false)
        }
    }

    pub async fn get_ttfb(&self, server: String, domain: String) -> Result<u32> {
        let proxy = self.get_router()?;
        Ok(proxy.get_ttfb(&server, &domain).await? as u32)
    }

    pub async fn get_autostart() -> Result<bool> {
        let auto = RouterService::init_autostart()?;
        Ok(auto.is_enabled()?)
    }

    pub async fn set_autostart(enabled: bool) -> Result<()> {
        let auto = RouterService::init_autostart()?;
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

    pub async fn register_logger(
        &self,
        callback: impl Fn(String) -> DartFnFuture<()> + Send + Sync + 'static,
    ) -> Result<u64> {
        self.get_writer_notifier()?.register_logger(callback).await
    }

    pub async fn unregister_logger(&self, id: u64) -> Result<()> {
        self.get_writer_notifier()?.unregister_logger(id).await
    }

    fn get_writer_notifier(&self) -> Result<&Arc<WriterNotifier>> {
        self.writer_notifier
            .get()
            .ok_or_else(|| anyhow!("writer notifier not initialized"))
    }

    fn get_router(&self) -> Result<&Arc<Router>> {
        self.router.get().ok_or_else(|| anyhow!("router not initialized"))
    }
}

#[frb(mirror(RouterState))]
pub enum _RouterState {
    Smart,
    All,
    Off,
}
