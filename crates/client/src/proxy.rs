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
use sys_connections::{Protocol, process_path_by_local_addr};
use tokio::{
    io::{AsyncRead, AsyncWriteExt},
    net::{TcpListener, TcpStream, lookup_host},
    select,
    sync::{Mutex, RwLock, oneshot},
};

use axum::{
    body::Body,
    extract::Request,
    http::{Method, StatusCode, uri},
    response::{IntoResponse, Response},
};
use hyper::{body::Incoming, server::conn::http1, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{self, RootCertStore, client::Tls12Resumption, pki_types},
};
use tower::util::ServiceExt;

use crate::pac_file_service::PacFileService;
use crate::protocol::{self, SelectedServer, Server};
use crate::upgrade_stream::UgradeStream;
use crate::{
    config::{ServerConfig, ServerConnectConfig, default_server_address},
    ttfb_stream::TtfbStream,
};
use crypto::config::ProtocolConfig;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq)]
pub enum ProxyState {
    #[default]
    Pac,
    All,
    Off,
}

pub struct Proxy {
    servers: RwLock<Vec<Server>>,
    apps: RwLock<Vec<String>>,
    pac_service: Arc<PacFileService>,
    proxy_state: RwLock<ProxyState>,
    tls_cfg: Arc<rustls::ClientConfig>,
    initialized: AtomicBool,
    restart: Mutex<Option<oneshot::Sender<()>>>,
}

enum StreamType {
    TcpStream(TcpStream),
    UgradeStream(UgradeStream<TlsStream<TcpStream>>),
}

impl Proxy {
    pub fn new(proxy_port: u16, proxy_state: ProxyState) -> Result<Arc<Self>> {
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

        Ok(Arc::new(Proxy {
            pac_service: PacFileService::new(proxy_port)?,
            servers: Default::default(),
            apps: Default::default(),
            proxy_state: RwLock::new(proxy_state),
            tls_cfg: Arc::new(tls_cfg),
            initialized: AtomicBool::new(false),
            restart: Default::default(),
        }))
    }

    pub fn get_proxy_address(&self) -> SocketAddr {
        self.pac_service.get_proxy_address()
    }

    pub async fn set_proxy_port(&self, port: u16) -> Result<()> {
        self.pac_service.set_new_port(port);
        self.restart
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow!("restart channel not initialized"))?
            .send(())
            .unwrap();
        self.reset_proxy().await
    }

    pub async fn get_apps(&self) -> Vec<String> {
        self.apps.read().await.clone()
    }

    pub async fn add_apps(&self, apps: &Vec<String>) {
        self.apps.write().await.extend_from_slice(apps.as_slice());
    }

    pub async fn add_domains(&self, hosts: &Vec<String>) {
        self.pac_service.add_domains(hosts).await;
    }

    pub async fn get_domains(&self) -> Vec<String> {
        self.pac_service.get_domains().await
    }

    pub async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        if !server_host.is_empty() {
            self.pac_service.remove_domain(&domain).await.ok();

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

                Ok(())
            } else {
                Err(anyhow!("host not found"))
            }
        } else {
            if !self.pac_service.get_domains().await.iter().any(|d| d == &domain) {
                self.pac_service.add_domains(&vec![domain.clone()]).await;
            }

            self.remove_domain_from_servers(&domain).await
        }
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
        self.pac_service.remove_domain(&domain).await?;
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

    pub async fn update_pac_content(&self) {
        self.pac_service.update_content().await;
    }

    pub async fn get_proxy_state(&self) -> ProxyState {
        *self.proxy_state.read().await
    }

    pub async fn reset_proxy(&self) -> Result<()> {
        let proxy_state = *self.proxy_state.read().await;
        if proxy_state == ProxyState::Pac {
            self.pac_service.update_content().await;
            self.pac_service.set_proxy_pac().await
        } else {
            Ok(())
        }
    }

    pub async fn set_proxy_state(&self, proxy_state: ProxyState) -> Result<()> {
        *self.proxy_state.write().await = proxy_state;

        self.set_proxy_state_int(proxy_state).await
    }

    pub async fn set_proxy_state_int(&self, proxy_state: ProxyState) -> Result<()> {
        match proxy_state {
            ProxyState::All => self.pac_service.set_proxy_all().await,
            ProxyState::Pac => self.pac_service.set_proxy_pac().await,
            ProxyState::Off => self.pac_service.restore_proxy().await,
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
                self.set_proxy_state(ProxyState::Off).await
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

    pub async fn serve(self: Arc<Self>) -> Result<()> {
        self.initialized.store(true, Ordering::Relaxed);

        let pac_router = self.pac_service.clone().new_router().await?;

        let proxy = self.clone();
        let handle_request = move |request: Request<Incoming>, client_addr: SocketAddr| {
            let pac_router = pac_router.clone();
            let req = request.map(Body::new);
            let proxy = proxy.clone();
            async move {
                if req.method() == Method::CONNECT {
                    serve_proxy_connection(req, proxy, client_addr).await
                } else if req.uri().scheme() == Some(&uri::Scheme::HTTP) {
                    let loc = req.uri().to_string().replace("http", "https");
                    Ok((StatusCode::TEMPORARY_REDIRECT, [("Location", loc.as_str())]).into_response())
                } else {
                    pac_router.oneshot(req).await.map_err(|err| match err {})
                }
            }
        };

        let proxy_state = *self.proxy_state.read().await;
        if proxy_state != ProxyState::Off {
            self.set_proxy_state_int(proxy_state).await?;
        }

        let (restart_tx, mut restart_rx) = oneshot::channel();
        self.restart.lock().await.replace(restart_tx);

        let proxy_address = self.pac_service.get_proxy_address();
        let mut listener = TcpListener::bind(proxy_address).await?;

        tracing::info!("proxy server started: {:?}", proxy_address);

        loop {
            select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, client_addr)) => {
                            let io = TokioIo::new(stream);
                            let handle_request = handle_request.clone();
                            tokio::task::spawn(async move {
                                let service =
                                    hyper::service::service_fn(move |request: Request<Incoming>| handle_request(request, client_addr));

                                if let Err(err) = http1::Builder::new()
                                    .preserve_header_case(true)
                                    .title_case_headers(true)
                                    .serve_connection(io, service)
                                    .with_upgrades()
                                    .await
                                {
                                    tracing::info!("Failed to serve connection: {:?}", err);
                                }
                            });
                        }
                        Err(error) => {
                            drop(listener);
                            tracing::error!("accept failed: {:?}", error);
                            listener = TcpListener::bind(proxy_address).await?;
                        }
                    }
                },
                _ = &mut restart_rx => {
                    let (new_restart_tx, new_restart_rx) = oneshot::channel();
                    restart_rx = new_restart_rx;
                    self.restart.lock().await.replace(new_restart_tx);

                    let proxy_address = self.pac_service.get_proxy_address();
                    listener = TcpListener::bind(proxy_address).await?;
                }
            }
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

async fn start_tunnel(
    upgraded: Upgraded,
    target_host: String,
    proxy: Arc<Proxy>,
    client_addr: SocketAddr,
) -> Result<()> {
    let client = TokioIo::new(upgraded);

    let mut rng = ChaCha20Rng::from_entropy();
    let selected = proxy.select_server(&target_host, &mut rng, client_addr).await?;
    if let Some(server) = selected {
        proxy.ensure_config_initialized(&server).await;
        proxy.start_tunnel_with_server(client, target_host, server, rng).await
    } else {
        proxy.direct_connection(client, target_host).await
    }
}

pub async fn serve_proxy_connection(
    req: Request,
    proxy: Arc<Proxy>,
    client_addr: SocketAddr,
) -> Result<Response, hyper::Error> {
    if let Some(host_addr) = req.uri().authority().map(|auth| auth.to_string()) {
        tokio::task::spawn(async move {
            match hyper::upgrade::on(req).await {
                Ok(upgraded) => {
                    if let Err(e) = start_tunnel(upgraded, host_addr, proxy, client_addr).await {
                        if let Some(io_err) = e.downcast_ref::<std::io::Error>()
                            && io_err.kind() == std::io::ErrorKind::UnexpectedEof
                        {
                            // suppress logging of unexpected eof errors
                            // https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
                            return;
                        }

                        tracing::warn!("server io error: {}", e);
                    };
                }
                Err(e) => tracing::warn!("upgrade error: {}", e),
            }
        });

        Ok(Response::new(Body::empty()))
    } else {
        tracing::warn!("CONNECT host is not socket addr: {:?}", req.uri());
        Ok(StatusCode::BAD_REQUEST.into_response())
    }
}
