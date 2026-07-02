use anyhow::Result;
use crypto::config::ProtocolConfig;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{Notify, RwLock},
    time::sleep,
};
use tokio_util::sync::CancellationToken;

use crate::{
    client_info::{ClientInfo, ServerInfo},
    config::{ClientConfig, ServerConfig},
    egress::Egress,
    router::Router,
    router_table::RouterTable,
    tun::service::TunService,
    utils::cancellable_task::CancellableTask,
};

const DEFAULT_ERROR_RETRY_INTERVAL_SEC: u64 = 1;
const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum ClientState {
    #[default]
    Smart,
    All,
    Off,
}

pub struct Client {
    cancel_token: CancellationToken,

    initialized: AtomicBool,
    working: AtomicBool,
    state: RwLock<ClientState>,
    state_notify: Arc<Notify>,
    info: Arc<ClientInfo>,

    router: Arc<Router>,
    tun_service: Arc<TunService>,
    egress: Arc<Egress>,

    update_task: CancellableTask,

    cfg_path: PathBuf,
}

impl Client {
    pub fn new(cfg_path: PathBuf) -> Arc<Self> {
        let egress = Egress::new();
        Arc::new(Client {
            cancel_token: CancellationToken::new(),
            tun_service: TunService::new(egress.clone()),
            info: ClientInfo::new(),
            state: RwLock::new(ClientState::Off),
            state_notify: Arc::new(Notify::new()),
            initialized: AtomicBool::new(false),
            working: AtomicBool::new(false),
            router: Router::new(egress.clone()),
            update_task: CancellableTask::new("update_connection_info"),
            egress,
            cfg_path,
        })
    }

    pub async fn initialize(self: &Arc<Self>) -> Result<()> {
        TunService::cleanup_at_start().await;
        self.egress.init().await?;
        self.load_config().await?;
        self.update().await;
        self.check_servers_initialized().await;
        self.initialized.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub async fn get_direct_apps(&self) -> Vec<String> {
        self.info.get_direct_apps().await
    }

    pub async fn get_direct_domains(&self) -> Vec<String> {
        self.info.get_direct_domains().await
    }

    pub async fn set_domain(self: &Arc<Self>, domain: String, server_host: String) -> Result<()> {
        self.info.set_domain(domain, server_host).await?;
        self.update_and_save().await;
        Ok(())
    }

    pub async fn remove_domain(self: &Arc<Self>, domain: String) -> Result<()> {
        self.info.remove_domain(&domain).await?;
        self.update_and_save().await;
        Ok(())
    }

    pub async fn set_app(self: &Arc<Self>, app: String, server_host: String) -> Result<()> {
        self.info.set_app(app, server_host).await?;
        self.update_and_save().await;
        Ok(())
    }

    pub async fn remove_app(self: &Arc<Self>, app: String) -> Result<()> {
        self.info.remove_app(&app).await?;
        self.update_and_save().await;
        Ok(())
    }

    pub async fn get_state(&self) -> ClientState {
        *self.state.read().await
    }

    pub async fn add_server(self: &Arc<Self>, config: ServerConfig) {
        self.info.add_server(config, &self.egress).await;
        self.update_and_save().await;
    }

    pub async fn del_server(self: &Arc<Self>, host: &str) -> Result<()> {
        let srv_count = self.info.del_server(host).await?;
        self.update_and_save().await;
        if srv_count == 0 {
            // turn off proxy if we have no servers
            self.set_state(ClientState::Off).await;
        }
        Ok(())
    }

    pub async fn set_enabled(self: &Arc<Self>, host: &str, value: bool) -> Result<()> {
        self.info.set_enabled(host, value).await?;
        self.update_and_save().await;
        // TODO: ??? terminate all connections
        Ok(())
    }

    pub async fn update_server(self: &Arc<Self>, orig_host: &str, config: ServerConfig) -> Result<()> {
        self.info.update_server(orig_host, config, &self.egress).await?;
        self.update_and_save().await;
        Ok(())
    }

    pub async fn get_servers(&self) -> Vec<ServerInfo> {
        self.info.get_servers().await
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    pub fn is_working(&self) -> bool {
        self.working.load(Ordering::Relaxed)
    }

    pub async fn get_ttfb(&self, host: &str, domain: &str) -> Result<usize> {
        self.router.get_ttfb(host, domain).await
    }

    pub async fn get_server_protocol(&self, host: &str, key: &str) -> Result<ProtocolConfig> {
        self.router.get_server_protocol(host, key).await
    }

    pub async fn set_state(&self, proxy_state: ClientState) {
        self.set_state_internal(proxy_state, true).await;
    }

    async fn set_state_internal(&self, proxy_state: ClientState, save_config: bool) {
        let mut wr_state = self.state.write().await;
        if *wr_state == proxy_state {
            return;
        }

        *wr_state = proxy_state;
        drop(wr_state);

        if save_config {
            self.save_config().await;
        }

        match proxy_state {
            ClientState::Off => {
                self.tun_service.stop().await;
                self.router.cancel_all().await;                
                self.info.clear_stats().await;
            }
            ClientState::Smart => {
                self.router.set_no_direct(false);
                self.state_notify.notify_one();
            }
            ClientState::All => {
                self.router.set_no_direct(true);
                self.state_notify.notify_one();
            }
        }
    }

    pub async fn shutdown(&self) {
        self.update_task.stop().await;
        self.egress.shutdown().await;
        self.cancel_token.cancel();
        self.set_state_internal(ClientState::Off, false).await;
    }

    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        let mut error_retry_interval_sec = DEFAULT_ERROR_RETRY_INTERVAL_SEC;
        loop {
            let state = *self.state.read().await;
            if state == ClientState::Off {
                self.working.store(false, Ordering::Relaxed);
                error_retry_interval_sec = DEFAULT_ERROR_RETRY_INTERVAL_SEC;

                // wait for state change
                tokio::select! {
                    _ = self.cancel_token.cancelled() => return Ok(()),
                    _ = self.state_notify.notified() => {}
                }
                continue;
            }

            let self_clone = self.clone();
            self.router.reset_cancel();
            let serve = self.tun_service.serve(self.router.clone(), move || {
                self_clone.working.store(true, Ordering::Relaxed);
            });
            if let Err(err) = serve.await {
                self.working.store(false, Ordering::Relaxed);
                tracing::warn!("tun service error: {:?}", err);

                // stop all working tasks in order to correct retry
                self.tun_service.stop().await;
                self.router.cancel_all().await;

                // wait before retry to avoid high cpu usage when error happens continuously
                sleep(Duration::from_secs(error_retry_interval_sec)).await;
                error_retry_interval_sec = if error_retry_interval_sec < 60 {
                    error_retry_interval_sec * 2
                } else {
                    error_retry_interval_sec
                };
            }
        }
    }

    async fn update_and_save(self: &Arc<Self>) {
        self.update().await;
        self.check_servers_initialized().await;
        self.save_config().await;
    }

    async fn check_servers_initialized(self: &Arc<Self>) {
        if !self.info.has_uninitialized_servers().await || self.update_task.is_running() {
            return;
        }

        tracing::warn!("not all servers are initialized");
        let self_clone = self.clone();
        self.update_task.spawn(|token| async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    _ = tokio::time::sleep(UPDATE_INTERVAL) => {}
                }
                self_clone.update().await;
                if !self_clone.info.has_uninitialized_servers().await {
                    tracing::info!("all servers are initialized");
                    return;
                }
            }
        });
    }

    async fn update(&self) {
        self.info.update_connection_info(&self.egress).await;
        self.router.update_table(RouterTable::from(self.info.clone()).await);
    }

    async fn load_config(&self) -> Result<()> {
        let cfg = ClientConfig::from_file(self.cfg_path.clone()).await?;

        self.info.add_direct_apps(&cfg.direct_apps).await;
        self.info.add_direct_domains(&cfg.direct_domains).await;

        *self.state.write().await = if cfg.servers.is_empty() {
            ClientState::Off
        } else {
            cfg.state
        };

        for srv in cfg.servers {
            self.info.add_server(srv, &self.egress).await;
        }

        Ok(())
    }

    async fn save_config(&self) {
        let cfg = ClientConfig {
            state: self.get_state().await,
            direct_domains: self.get_direct_domains().await,
            direct_apps: self.get_direct_apps().await,
            servers: self.get_servers().await.into_iter().map(|srv| srv.config).collect(),
        };

        if let Err(err) = cfg.save_to_file(self.cfg_path.clone()).await {
            tracing::error!("Failed to save config: {:?}", err);
        }
    }
}
