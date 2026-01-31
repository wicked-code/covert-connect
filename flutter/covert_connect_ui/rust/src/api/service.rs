use anyhow::{anyhow, Result};
use auto_launch::{AutoLaunch, AutoLaunchBuilder};
use std::env;
use std::sync::{atomic::Ordering, Arc, OnceLock};
use tokio::net::lookup_host;

use client::config::ServerConfig as ClientServerConfig;
use client::proxy::Proxy;

pub use client::proxy::ProxyState;

use flutter_rust_bridge::{DartFnFuture, frb};

use crate::api::log::{LogLine, WriterNotifier, get_trace_log, init_trace_log};
use crate::api::wrappers::{ProtocolConfig, ServerConfig};

#[derive(Clone)]
pub struct ProxyConfig {
    pub state: ProxyState,
    pub port: u16,
    pub domains: Vec<String>,
    pub apps: Vec<String>,
    pub servers: Vec<ServerConfig>,
}

#[derive(Clone)]
pub struct ProxyStateFull {
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

pub struct ProxyService {
    /// flutter_rust_bridge:ignore
    proxy: OnceLock<Arc<Proxy>>,
    /// flutter_rust_bridge:ignore
    writer_notifier: OnceLock<Arc<WriterNotifier>>,
}

impl ProxyService {
    #[frb(sync)]
    pub fn new() -> ProxyService {
        let writer_notifier = OnceLock::new();
        match init_trace_log() {
            Ok(notifier) => {
                writer_notifier.set(notifier).ok();
            }
            Err(e) => {
                println!("Failed to initialize trace log: {:?}", e);
            },
        }
        return {
            ProxyService {
                proxy: Default::default(),
                writer_notifier: writer_notifier,
            }
        };
    }

    pub async fn start(&self, cfg: ProxyConfig) -> Result<()> {
        let mut proxy_state = cfg.state;
        if cfg.servers.is_empty() {
            // turn off proxy if no servers
            proxy_state = ProxyState::Off;
        }

        let proxy_instance = Proxy::new(cfg.port, proxy_state)?;
        self.proxy
            .set(proxy_instance.clone())
            .map_err(|_| anyhow!("proxy already initialized"))?;

        flutter_rust_bridge::spawn(async move {
            proxy_instance.add_apps(&cfg.apps).await;
            proxy_instance.add_domains(&cfg.domains).await;
            proxy_instance.update_pac_content().await;

            for srv in cfg.servers {
                let mut cfg: ClientServerConfig = srv.into();
                cfg.init().await.inspect_err(|e| tracing::error!("config: {:?}", e)).ok();
                proxy_instance.add_server(cfg).await;
            }

            if let Err(err) = proxy_instance.serve().await {
                tracing::error!("serve: {:?}", err);
            }
        });

        Ok(())
    }

    pub async fn get_config(&self) -> Result<ProxyConfig> {
        let proxy = self.get_proxy()?;

        return Ok(ProxyConfig {
            state: proxy.get_proxy_state().await,
            port: proxy.get_proxy_address().port(),
            servers: proxy
                .get_servers()
                .await
                .into_iter()
                .map(|srv| srv.config.into())
                .collect(),
            domains: proxy.get_domains().await,
            apps: proxy.get_apps().await,
        });
    }

    pub async fn get_state(&self) -> Result<ProxyStateFull> {
        let proxy = self.get_proxy()?;

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

        Ok(ProxyStateFull {
            initialized: proxy.is_initialized(),
            servers,
        })
    }

    pub async fn get_proxy_state(&self) -> Result<ProxyState> {
        Ok(self.get_proxy()?.get_proxy_state().await)
    }

    pub async fn set_proxy_state(&self, proxy_state: ProxyState) -> Result<()> {
        self.get_proxy()?.set_proxy_state(proxy_state).await
    }

    pub async fn set_server_enabled(&self, host: String, value: bool) -> Result<()> {
        self.get_proxy()?.set_enabled(&host, value).await
    }

    pub async fn get_server_protocol(&self, server: String, key: String) -> Result<ProtocolConfig> {
        let protocol = self.get_proxy()?.get_server_protocol(&server, &key).await?;
        Ok(protocol.into())
    }

    pub async fn stop(&self) -> Result<()> {
        self.get_proxy()?.set_proxy_state(ProxyState::Off).await
    }

    pub async fn get_apps(&self) -> Result<Vec<String>> {
        Ok(self.get_proxy()?.get_apps().await)
    }

    pub async fn get_domains(&self) -> Result<Vec<String>> {
        Ok(self.get_proxy()?.get_domains().await)
    }

    pub async fn add_server(&self, config: ServerConfig) -> Result<()> {
        let mut cfg: ClientServerConfig = config.into();
        cfg.init().await.inspect_err(|e| tracing::error!("config init: {:?}", e)).ok();        
        self.get_proxy()?.add_server(cfg).await;
        Ok(())
    }

    pub async fn update_server(&self, orig_host: String, new_config: ServerConfig) -> Result<()> {
        let mut cfg: ClientServerConfig = new_config.into();
        cfg.init().await.inspect_err(|e| tracing::error!("config init: {:?}", e)).ok();           
        self.get_proxy()?.update_server(&orig_host, cfg).await
    }

    pub async fn delete_server(&self, host: String) -> Result<()> {
        self.get_proxy()?.del_server(&host).await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        let proxy = self.get_proxy()?;
        proxy.set_domain(domain, server_host).await?;
        proxy.reset_proxy().await
    }

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        let proxy = self.get_proxy()?;
        proxy.remove_domain(domain).await?;
        proxy.reset_proxy().await
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        let proxy = self.get_proxy()?;
        proxy.set_app(app, server_host).await?;
        proxy.reset_proxy().await
    }

    pub async fn remove_app(&self, app: String) -> Result<()> {
        let proxy = self.get_proxy()?;
        proxy.remove_app(app).await?;
        proxy.reset_proxy().await
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
        let proxy = self.get_proxy()?;
        Ok(proxy.get_ttfb(&server, &domain).await? as u32)
    }

    pub async fn get_proxy_port(&self) -> Result<u16> {
        let proxy = self.get_proxy()?;
        Ok(proxy.get_proxy_address().port())
    }

    pub async fn set_proxy_port(&self, port: u16) -> Result<()> {
        let proxy = self.get_proxy()?;
        proxy.set_proxy_port(port).await
    }

    pub async fn get_autostart() -> Result<bool> {
        let auto = ProxyService::init_autostart()?;
        Ok(auto.is_enabled()?)
    }

    pub async fn set_autostart(enabled: bool) -> Result<()> {
        let auto = ProxyService::init_autostart()?;
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

    pub async fn register_logger(&self, callback: impl Fn(String) -> DartFnFuture<()> + Send + Sync + 'static) -> Result<u64> {
        self.get_writer_notifier()?.register_logger(callback).await
    }

    pub async fn unregister_logger(&self, id: u64) -> Result<()> {
        self.get_writer_notifier()?.unregister_logger(id).await
    }

    fn get_writer_notifier(&self) -> Result<&Arc<WriterNotifier>> {
        self.writer_notifier.get().ok_or_else(|| anyhow!("writer notifier not initialized"))
    }

    fn get_proxy(&self) -> Result<&Arc<Proxy>> {
        self.proxy.get().ok_or_else(|| anyhow!("proxy not initialized"))
    }
}

#[frb(mirror(ProxyState))]
pub enum _ProxyState {
    Pac,
    All,
    Off,
}
