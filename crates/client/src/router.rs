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
    net::{TcpStream, lookup_host},
    sync::RwLock,
};

use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{self, RootCertStore, client::Tls12Resumption, pki_types},
};

use crate::{
    protocol::{self, SelectedServer, Server},
    config::{ServerConfig, ServerConnectConfig, default_server_address},
    streams::{ttfb_stream::TtfbStream, upgrade_stream::UgradeStream},
    proxy_service::ProxyService,
    tun_service::TunService
};
use crypto::config::ProtocolConfig;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum RouterState {
    #[default]
    Smart,
    All,
    Off,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum RouterMode {
    #[default]
    Proxy,
    Tun,
}

pub struct Router {
    initialized: AtomicBool,
    state: RwLock<RouterState>,
    mode: RwLock<RouterMode>,

    servers: RwLock<Vec<Server>>,
    apps: RwLock<Vec<String>>,

    tls_cfg: Arc<rustls::ClientConfig>,

    tun_service: Arc<TunService>,
    proxy_service: Arc<ProxyService>,
}

enum StreamType {
    TcpStream(TcpStream),
    UgradeStream(UgradeStream<TlsStream<TcpStream>>),
}

impl Router {
    pub fn new(proxy_port: u16, state: RouterState, mode: RouterMode) -> Result<Arc<Self>> {
        let root_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.into(),
        };
        let mut tls_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        // it's ok to set it true, because we have internal replay protection
        // it's default to 10 sec thus replay may result in outbound connection only if sent in less than 10 sec
        // moreover if outbound ip is different there is no practical usage of such replay
        tls_cfg.enable_early_data = true;
        tls_cfg.resumption = tls_cfg.resumption.tls12_resumption(Tls12Resumption::SessionIdOnly);
        Ok(Arc::new_cyclic(|weak_self| Router {
            proxy_service: ProxyService::new(proxy_port, weak_self.clone()),
            tun_service: TunService::new(),
            servers: Default::default(),
            apps: Default::default(),
            state: RwLock::new(state),
            mode: RwLock::new(mode),
            tls_cfg: Arc::new(tls_cfg),
            initialized: AtomicBool::new(false),
        }))
    }

    pub fn get_proxy_address(&self) -> SocketAddr {
        self.proxy_service.get_proxy_address()
    }

    pub async fn set_proxy_port(&self, port: u16) -> Result<()> {
        self.proxy_service.set_proxy_port(port).await
    }

    pub async fn get_apps(&self) -> Vec<String> {
        self.apps.read().await.clone()
    }

    pub async fn add_apps(&self, apps: &Vec<String>) {
        self.apps.write().await.extend_from_slice(apps.as_slice());
    }

    pub async fn add_domains(&self, hosts: &Vec<String>) {
        self.proxy_service.add_domains(hosts).await;
    }

    pub async fn get_domains(&self) -> Vec<String> {
        self.proxy_service.get_domains().await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        if !server_host.is_empty() {
            self.proxy_service.remove_domain(&domain).await.ok();

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
            if !self.proxy_service.get_domains().await.iter().any(|d| d == &domain) {
                self.proxy_service.add_domains_and_reset(&vec![domain.clone()]).await?;
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

    pub async fn remove_domain(&self, domain: String) -> Result<()> {
        self.proxy_service.remove_domain(&domain).await?;
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
            if !self.apps.read().await.iter().any(|d| d == &app) {
                self.apps.write().await.push(app.clone());
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
        let mut wr_apps = self.apps.write().await;
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

        if *self.mode.read().await == RouterMode::Proxy {
            self.proxy_service.set_proxy_state(proxy_state).await?;
        }

        Ok(())
    }

    pub async fn get_mode(&self) -> RouterMode {
        *self.mode.read().await
    }

    pub async fn set_mode(&self, mode: RouterMode) -> Result<()> {
        if *self.mode.read().await == mode {
            return Ok(()); // no change
        }

        *self.mode.write().await = mode;
        match mode {
            RouterMode::Proxy => self.tun_service.stop().await,
            RouterMode::Tun => self.proxy_service.stop().await,
        }
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
            .start_tunnel_with_server(req_stream, domain.to_owned() + ":80", selected, rng)
            .await;

        let ttfb = ttfb.load(Ordering::Relaxed) as usize;
        // ignore errors if we have ttfb > 0
        // rutls may return https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
        if ttfb == 0 {
            res?;
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
    ) -> Result<Option<SelectedServer>> {
        let mut process_name = String::from("");
        match process_path_by_local_addr(client_addr, Protocol::TCP) {
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
            let apps = self.apps.read().await;
            for app in apps.iter() {
                if process_name.contains(app) {
                    return Ok(None);
                }
            }
            drop(apps);
        }

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

        let rnd_val = rng.gen_range(0..avr_weight * servers.len());

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

    pub async fn serve(&self) -> Result<()> {
        self.initialized.store(true, Ordering::Relaxed);

        loop {
            if let Err(e) = self.server_int().await {
                tracing::error!("server error: {e}");
                // try change mode and try one more time
                let mode = match *self.mode.read().await {
                    RouterMode::Proxy => RouterMode::Tun,
                    RouterMode::Tun => RouterMode::Proxy,
                };
                *self.mode.write().await = mode;
                self.server_int().await?;
            }
        }
    }

    async fn server_int(&self) -> Result<()> {
        let state = *self.state.read().await;
        match *self.mode.read().await {
            RouterMode::Proxy => self.proxy_service.clone().serve(state).await,
            RouterMode::Tun => self.tun_service.clone().serve().await,
        }
    }

    pub async fn start_tunnel(
        &self,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target_host: String,
        client_addr: SocketAddr,
    ) -> Result<()> {
        let mut rng = ChaCha20Rng::from_entropy();
        let selected = self.select_server(&target_host, &mut rng, client_addr).await?;
        if let Some(server) = selected {
            self.ensure_config_initialized(&server).await;
            self.start_tunnel_with_server(client, target_host, server, rng).await
        } else {
            self.direct_connection(client, target_host).await
        }
    }

    async fn start_tunnel_with_server(
        &self,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target_host: String,
        selected: SelectedServer,
        rng: impl CryptoRng + Rng,
    ) -> Result<()> {
        match self
            .connect(selected.address, &selected.host, &selected.url_path)
            .await?
        {
            StreamType::TcpStream(stream) => protocol::process_tunnel(stream, client, target_host, rng, selected).await,
            StreamType::UgradeStream(stream) => {
                protocol::process_tunnel(stream, client, target_host, rng, selected).await
            }
        }
    }

    async fn connect(&self, address: SocketAddr, host: &str, url_path: &Option<String>) -> Result<StreamType> {
        let server = TcpStream::connect(address).await?;
        Ok(if let Some(http_path) = url_path {
            // HTTPS connect
            let host = if let Some(pos) = host.rfind(':') {
                &host[..pos]
            } else {
                host
            };

            let domain = pki_types::ServerName::try_from(host)?.to_owned();
            let tls_conn = TlsConnector::from(self.tls_cfg.clone());
            let server = tls_conn.connect(domain, server).await?;

            StreamType::UgradeStream(UgradeStream::from_stream(server, host, http_path))
        } else {
            StreamType::TcpStream(server)
        })
    }

    async fn direct_connection(
        &self,
        mut client: impl AsyncWriteExt + Unpin + AsyncRead,
        target_host: String,
    ) -> Result<()> {
        tracing::debug!("direct connection to {}", target_host);
        // TODO: ??? for VPN mode!
        // // todo get direct IF (get it once or probaly update once per reasonable time 10 sec?)
        // let local_ip = Ipv4Addr::new(192, 168, 50, 117);
        // let local_address = SocketAddr::new(local_ip.into(), 0);

        // target_host: String,

        // // bind socket to outbound IF
        // let socket = TcpSocket::new_v4()?;
        // socket.bind(local_address)?;

        // let server = socket.connect(remote_addr).await?;
        let target_address: SocketAddr = lookup_host(&target_host)
            .await?
            .reduce(|acc, val| if acc.is_ipv6() && val.is_ipv4() { val } else { acc })
            .ok_or_else(|| anyhow!("host {target_host} notfound"))?;
        let mut server = TcpStream::connect(target_address).await?;

        tokio::io::copy_bidirectional(&mut client, &mut server).await?;
        Ok(())
    }
}
