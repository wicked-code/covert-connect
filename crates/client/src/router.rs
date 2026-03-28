use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
use sys_process::{Protocol, process_path_by_local_addr};
use tokio::{
    io::{AsyncRead, AsyncWriteExt},
    sync::RwLock,
};

use rand::prelude::*;
use rand_chacha::ChaCha20Rng;

use crate::{
    config::{ServerConfig, ServerConnectConfig, default_server_address},
    protocol::{self, DataProtocol, SelectedServer, Server},
    streams::ttfb_stream::TtfbStream,
    transport::{StreamType, Transport},
    tun_service::TunService,
};
use crypto::config::ProtocolConfig;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum RouterState {
    #[default]
    Smart,
    All,
    Off,
}

pub struct Router {
    initialized: AtomicBool,
    state: RwLock<RouterState>,

    servers: RwLock<Vec<Server>>,
    direct_apps: RwLock<Vec<String>>,
    direct_domains: RwLock<Vec<String>>,

    tun_service: Arc<TunService>,
    transport: Arc<Transport>,
}

impl Router {
    pub fn new(state: RouterState) -> Result<Arc<Self>> {
        Ok(Arc::new(Router {
            tun_service: TunService::new(),
            servers: Default::default(),
            direct_apps: Default::default(),
            direct_domains: Default::default(),
            state: RwLock::new(state),
            initialized: AtomicBool::new(false),
            transport: Transport::new(),
        }))
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

    pub async fn remove_app_internal(&self, app: &str) -> Result<()> {
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

    pub async fn get_state(&self) -> RouterState {
        *self.state.read().await
    }

    pub async fn set_state(&self, proxy_state: RouterState) -> Result<()> {
        *self.state.write().await = proxy_state;
        Ok(())
    }

    pub async fn add_server(&self, config: ServerConfig) {
        self.servers.write().await.push(Server {
            config,
            state: Default::default(),
        });
    }

    pub async fn del_server(&self, host: &str) -> Result<()> {
        let mut wr_servers = self.servers.write().await;
        if let Some(idx) = wr_servers.iter().position(|s| s.config.host == host) {
            wr_servers.remove(idx);
            if wr_servers.len() == 0 {
                drop(wr_servers);
                // turn off proxy if we have no servers
                self.set_state(RouterState::Off).await
            } else {
                Ok(())
            }
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

    pub async fn get_server_protocol(&self, host: &str, key: &str) -> Result<ProtocolConfig> {
        let conn_cfg = ServerConnectConfig::new(host, key).await?;

        match self
            .transport
            .connect(conn_cfg.address, &conn_cfg.host, &conn_cfg.url_path)
            .await?
        {
            StreamType::TcpStream(stream) => protocol::get_server_protocol(stream, key).await,
            StreamType::UgradeStream(stream) => protocol::get_server_protocol(stream, key).await,
        }
    }

    async fn ensure_config_initialized(&self, sel_srv: &SelectedServer) {
        if sel_srv.address != default_server_address() {
            return;
        }

        let servers = self.servers.read().await;
        if let Some(srv) = servers.iter().find(|s| s.config.host == sel_srv.host) {
            let mut new_config = srv.config.clone();
            drop(servers);

            if let Err(err) = new_config.init().await {
                tracing::error!("ensure init config: {:?}", err);
            } else {
                self.update_server(&sel_srv.host, new_config).await.ok();
            }
        } else {
            tracing::error!("ensure_config_initialized server not found");
        }
    }

    pub async fn get_ttfb(&self, host: &str, domain: &str) -> Result<usize> {
        let selected: SelectedServer;
        if let Some(srv) = self.servers.read().await.iter().find(|s| s.config.host == host) {
            selected = srv.into();
        } else {
            anyhow::bail!("host not found");
        }

        let ttfb = Arc::new(AtomicU64::new(0));
        let req_stream = TtfbStream::new(ttfb.clone());

        let rng = ChaCha20Rng::from_entropy();
        let res = self
            .start_tunnel_with_server(req_stream, DataProtocol::Tcp, domain.to_owned() + ":80", selected, rng)
            .await;

        let ttfb = ttfb.load(Ordering::Relaxed) as usize;

        // TODO: ??? move inside start_tunnel_with_server or even deeper, start_tunnel_with_server should suppress this error
        // rutls may return https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
        // ignore unexpected-eof it's not a problem in our case
        if let Err(ref e) = res {
            if let Some(io_err) = e.downcast_ref::<std::io::Error>()
                && io_err.kind() != std::io::ErrorKind::UnexpectedEof
            {
                res?;
            }
        }

        Ok(ttfb)
    }

    pub async fn remove_server(&self, host: &str) -> Result<()> {
        let mut servers = self.servers.write().await;
        if let Some(pos) = servers.iter().position(|s| s.config.host == host) {
            servers.remove(pos);
            Ok(())
        } else {
            Err(anyhow!("host not found"))
        }
    }

    pub async fn get_servers(&self) -> Vec<Server> {
        self.servers.read().await.iter().cloned().collect()
    }

    pub async fn select_server(
        &self,
        target_host: &str,
        mut rng: impl CryptoRng + Rng,
        client_addr: SocketAddr,
        data_protocol: &DataProtocol,
    ) -> Result<Option<SelectedServer>> {
        let mut process_name = String::from("");
        match process_path_by_local_addr(client_addr, match data_protocol {
            DataProtocol::Tcp => Protocol::TCP,
            DataProtocol::Udp => Protocol::UDP,
            DataProtocol::Icmp => Protocol::TCP,
        }) {
            Ok(process_path) => {
                tracing::info!("{} connecting to {}", process_path, target_host);
                let a = Path::new(&process_path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                process_name = a.into_owned();
            }
            Err(err) => tracing::warn!("unknown connecting to {}\n{}", target_host, err),
        }

        // process filter, check for direct first
        if !process_name.is_empty() {
            let apps = self.direct_apps.read().await;
            for app in apps.iter() {
                if process_name.contains(app) {
                    return Ok(None);
                }
            }
            drop(apps);
        }

        // check direct domains
        // TODO: ??? use binary search
        let direct_domains = self.direct_domains.read().await;
        for domain in direct_domains.iter() {
            if target_host.contains(domain) {
                return Ok(None);
            }
        }
        drop(direct_domains);

        // prepare domain filter
        let mut domain = target_host.to_owned();
        if let Some(port_pos) = target_host.rfind(':') {
            domain.truncate(port_pos);
        }

        let mut domain_variants = Vec::new();
        let mut accumulated = String::with_capacity(domain.len());
        let parts = domain.split('.').rev().enumerate();
        for (i, part) in parts {
            accumulated.insert_str(0, part);
            if i > 0 {
                domain_variants.push(accumulated.clone());
            }
            accumulated.insert(0, '.');
        }

        let servers = self.servers.read().await;
        if servers.len() == 0 {
            bail!("no servers found")
        }

        // select server by process name
        for srv in servers.iter() {
            if !srv.config.enabled {
                continue;
            }

            if let Some(apps) = &srv.config.apps {
                for app in apps.iter() {
                    if process_name.contains(app) {
                        return Ok(Some(srv.into()));
                    }
                }
            }
        }

        // select server by domain
        let mut total_weight = 0_usize;
        let mut enabled_count = 0_usize;
        let mut unweighted_count = 0_usize;
        for srv in servers.iter() {
            if !srv.config.enabled {
                continue;
            }

            if let Some(domains) = &srv.config.domains {
                for domain in &domain_variants {
                    if domains.binary_search(domain).is_ok() {
                        return Ok(Some(srv.into()));
                    }
                }
            }

            enabled_count += 1;
            if let Some(weight) = srv.config.weight {
                total_weight += weight as usize;
            } else {
                unweighted_count += 1;
            }
        }

        let avr_weight = if total_weight > 0 {
            total_weight / (enabled_count - unweighted_count)
        } else {
            100 / unweighted_count
        };

        let rnd_val = rng.gen_range(0..avr_weight * enabled_count);

        let mut cur_weight = 0_usize;
        for srv in servers.iter() {
            if !srv.config.enabled {
                continue;
            }

            cur_weight += srv.config.weight.unwrap_or(avr_weight as u8) as usize;

            if cur_weight >= rnd_val {
                return Ok(Some(srv.into()));
            }
        }

        Ok(Some(servers.first().unwrap().into()))
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Relaxed)
    }
 
    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        self.initialized.store(true, Ordering::Relaxed);
        self.transport.init().await?;
        self.tun_service.serve(self.clone()).await
    }

    pub async fn start_tunnel(
        &self,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        data_protocol: DataProtocol,
        target_host: String,
        target_addr: SocketAddr,
        client_addr: SocketAddr,
    ) -> Result<()> {
        // TODO: ??? add target: SocketAddr and outbound_ip: IpAddr
        // target should be used to connect instead of url in case we mesmatch url or target_host not found
        let mut rng = ChaCha20Rng::from_entropy();
        let selected = self.select_server(&target_host, &mut rng, client_addr, &data_protocol).await?;
        if let Some(server) = selected {
            self.ensure_config_initialized(&server).await;
            let res = self
                .start_tunnel_with_server(client, data_protocol, target_addr.to_string(), server, rng)
                .await;
            // TODO: ??? move inside start_tunnel_with_server or even deeper, start_tunnel_with_server should suppress this error
            // rutls may return https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
            // ignore unexpected-eof it's not a problem in our case
            if let Err(ref e) = res {
                if let Some(io_err) = e.downcast_ref::<std::io::Error>()
                    && io_err.kind() != std::io::ErrorKind::UnexpectedEof
                {
                    res?;
                }
            }

            Ok(())
        } else {
            // TODO: ??? use ip instead of parse
            self.transport.direct_transfer(client, target_addr).await
        }
    }

    async fn start_tunnel_with_server(
        &self,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        data_protocol: DataProtocol,
        target_host: String,
        selected: SelectedServer,
        rng: impl CryptoRng + Rng,
    ) -> Result<()> {
        match self
            .transport
            .connect(selected.address, &selected.host, &selected.url_path)
            .await?
        {
            StreamType::TcpStream(stream) => protocol::process_tunnel(stream, client, data_protocol, target_host, rng, selected).await,
            StreamType::UgradeStream(stream) => {
                protocol::process_tunnel(stream, client, data_protocol, target_host, rng, selected).await
            }
        }
    }
}
