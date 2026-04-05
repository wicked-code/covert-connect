use anyhow::Result;
use crypto::config::ProtocolConfig;
use serde::{Deserialize, Serialize};
use std::{
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

use crate::{
    client_info::{ClientInfo, ServerInfo},
    config::{ServerConfig, default_server_address},
    egress::Egress,
    router::Router,
    router_table::RouterTable,
    tun::service::TunService,
};

const DEFAULT_ERROR_RETRY_INTERVAL_SEC: u64 = 1;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum ClientState {
    #[default]
    Smart,
    All,
    Off,
}

pub struct Client {
    initialized: AtomicBool,
    working: AtomicBool,
    state: RwLock<ClientState>,
    state_notify: Arc<Notify>,
    info: Arc<ClientInfo>,

    router: Arc<Router>,
    tun_service: Arc<TunService>,
    egress: Arc<Egress>,
}

impl Client {
    pub fn new(state: ClientState) -> Arc<Self> {
        let egress = Egress::new();
        Arc::new(Client {
            tun_service: TunService::new(egress.clone()),
            info: ClientInfo::new(),
            state: RwLock::new(state),
            state_notify: Arc::new(Notify::new()),
            initialized: AtomicBool::new(false),
            working: AtomicBool::new(false),
            router: Router::new(egress.clone()),
            egress,
        })
    }

    pub async fn get_direct_apps(&self) -> Vec<String> {
        self.info.get_direct_apps().await
    }

    pub async fn add_direct_apps(&self, apps: &Vec<String>) {
        self.info.add_direct_apps(apps).await;
        self.update_router().await;
    }

    pub async fn add_direct_domains(&self, hosts: &Vec<String>) {
        self.info.add_direct_domains(hosts).await;
        self.update_router().await;
    }

    pub async fn get_direct_domains(&self) -> Vec<String> {
        self.info.get_direct_domains().await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        self.info.set_domain(domain, server_host).await?;
        self.update_router().await;
        Ok(())
    }

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        self.info.remove_domain(domain).await?;
        self.update_router().await;
        Ok(())
    }

    pub async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        self.info.set_app(app, server_host).await?;
        self.update_router().await;
        Ok(())
    }

    pub async fn remove_app(&self, app: String) -> Result<()> {
        self.info.remove_app(app).await?;
        self.update_router().await;
        Ok(())
    }

    pub async fn get_state(&self) -> ClientState {
        *self.state.read().await
    }

    pub async fn add_server(&self, config: ServerConfig) {
        self.info.add_server(config).await;
        self.update_router().await;
    }

    pub async fn del_server(&self, host: &str) -> Result<()> {
        let srv_count = self.info.del_server(host).await?;
        self.update_router().await;
        if srv_count == 0 {
            // turn off proxy if we have no servers
            self.set_state(ClientState::Off).await
        } else {
            Ok(())
        }
    }

    pub async fn set_enabled(&self, host: &str, value: bool) -> Result<()> {
        self.info.set_enabled(host, value).await?;
        self.update_router().await;
        // TODO: ??? terminate all connections
        Ok(())
    }

    pub async fn update_server(&self, orig_host: &str, config: ServerConfig) -> Result<()> {
        self.info.update_server(orig_host, config).await?;
        self.update_router().await;
        Ok(())
    }

    pub async fn get_servers(&self) -> Vec<ServerInfo> {
        self.info.get_servers().await
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }

    pub async fn get_ttfb(&self, host: &str, domain: &str) -> Result<usize> {
        self.router.get_ttfb(host, domain).await
    }

    pub async fn get_server_protocol(&self, host: &str, key: &str) -> Result<ProtocolConfig> {
        self.router.get_server_protocol(host, key).await
    }

    pub async fn set_state(&self, proxy_state: ClientState) -> Result<()> {
        let mut wr_state = self.state.write().await;
        if *wr_state == proxy_state {
            return Ok(());
        }

        *wr_state = proxy_state;
        drop(wr_state);

        match proxy_state {
            ClientState::Off => {
                self.tun_service.stop().await;
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
        Ok(())
    }

    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        self.egress.init().await?;
        self.update_router().await;

        let mut error_retry_interval_sec = DEFAULT_ERROR_RETRY_INTERVAL_SEC;
        loop {
            let state = *self.state.read().await;
            if state == ClientState::Off {
                self.working.store(false, Ordering::Relaxed);
                error_retry_interval_sec = DEFAULT_ERROR_RETRY_INTERVAL_SEC;
                self.initialized.store(true, Ordering::Relaxed);

                // wait for state change
                self.state_notify.notified().await;
                continue;
            }

            let self_clone = self.clone();
            let serve = self.tun_service.serve(self.router.clone(), move || {
                self_clone.working.store(true, Ordering::Relaxed);
                self_clone.initialized.store(true, Ordering::Relaxed);
            });
            if let Err(err) = serve.await {
                self.working.store(false, Ordering::Relaxed);
                tracing::warn!("tun service error: {:?}", err);

                // stop all working tasks in order to correct retry
                self.tun_service.stop().await;

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

    async fn update_router(&self) {
        // TODO: ??? register notification in egress and on egress update update router
        // but update egress only if previous ip is not valid anymore!!!
        for srv in self.info.get_servers().await.iter() {
            self.ensure_config_initialized(srv).await;
        }
        self.router.update_table(RouterTable::from(self.info.clone()).await);
    }

    async fn ensure_config_initialized(&self, srv: &ServerInfo) {
        if srv.config.address != default_server_address() {
            return;
        }

        let mut new_config = srv.config.clone();
        if let Err(err) = new_config.init().await {
            tracing::error!("init config error: {:?}", err);
        } else {
            self.info.update_server(&srv.config.host, new_config).await.ok();
        }
    }
}
