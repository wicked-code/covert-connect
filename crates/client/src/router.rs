use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
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
    time::Duration,
};

use rand::prelude::*;
use rand_chacha::ChaCha20Rng;

use crate::{
    cancel_watcher::{CancelWatcher, CancellableTaskHandle},
    config::ServerConnectConfig,
    egress::{Egress, StreamType},
    protocol::{self, DataProtocol},
    router_table::{RouteResult, RouterTable, ServerContext},
    streams::ttfb_stream::TtfbStream,
};
use crypto::config::ProtocolConfig;

const MAX_CANCEL_WAIT_SECS: Duration = Duration::from_secs(10);

pub struct Router {
    egress: Arc<Egress>,
    no_direct: AtomicBool,
    table: ArcSwap<RouterTable>,
    cancel_watcher: CancelWatcher,
}

impl Router {
    pub fn new(egress: Arc<Egress>) -> Arc<Self> {
        Arc::new(Router {
            egress,
            no_direct: AtomicBool::new(false),
            table: ArcSwap::new(RouterTable::new()),
            cancel_watcher: CancelWatcher::new(),
        })
    }

    pub async fn cancel_all(&self) {
        self.cancel_watcher.wait_for_shutdown(MAX_CANCEL_WAIT_SECS).await;
    }

    pub fn reset_cancel(&self) {
        self.cancel_watcher.reset();
    }

    pub fn set_no_direct(&self, no_direct: bool) {
        self.no_direct.store(no_direct, Ordering::Relaxed);
    }

    pub fn update_table(&self, new_table: Arc<RouterTable>) {
        self.table.store(new_table);
    }

    pub async fn get_server_protocol(&self, host: &str, key: &str) -> Result<ProtocolConfig> {
        let conn_cfg = ServerConnectConfig::new(host, key).await?;

        match self
            .egress
            .connect_with_upgrade(conn_cfg.address, &conn_cfg.host, &conn_cfg.url_path)
            .await?
        {
            StreamType::TcpStream(stream) => protocol::get_server_protocol(stream, key).await,
            StreamType::UpgradeStream(stream) => protocol::get_server_protocol(stream, key).await,
        }
    }

    pub async fn get_ttfb(&self, host: &str, domain: &str) -> Result<usize> {
        let server = self
            .table
            .load()
            .server_by_host(host)
            .ok_or_else(|| anyhow!("host not found"))?;

        let ttfb = Arc::new(AtomicU64::new(0));
        let req_stream = TtfbStream::new(ttfb.clone());

        let rng = ChaCha20Rng::from_entropy();
        self.start_tunnel_with_server(req_stream, DataProtocol::Tcp, domain.to_owned() + ":80", server, rng)
            .await?;

        Ok(ttfb.load(Ordering::Relaxed) as usize)
    }

    pub async fn start_tunnel(
        &self,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        data_protocol: DataProtocol,
        target_host: String,
        client_addr: SocketAddr,
    ) -> Result<Option<(impl AsyncWriteExt + Unpin + AsyncRead, CancellableTaskHandle)>> {
        let mut rng = ChaCha20Rng::from_entropy();

        let mut process_name = String::from("");
        match process_path_by_local_addr(
            client_addr,
            match data_protocol {
                DataProtocol::Tcp => Protocol::TCP,
                DataProtocol::Udp => Protocol::UDP,
                DataProtocol::Icmp => Protocol::UDP,
            },
        ) {
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

        if let Some(server) = self.select_server(&target_host, &process_name, &mut rng) {
            self.start_tunnel_with_server(client, data_protocol, target_host, server, rng)
                .await?;
            Ok(None)
        } else {
            Ok(Some((client, self.cancel_watcher.create_handle())))
        }
    }

    async fn start_tunnel_with_server(
        &self,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        data_protocol: DataProtocol,
        target_host: String,
        server: Arc<ServerContext>,
        rng: impl CryptoRng + Rng,
    ) -> Result<()> {
        match self
            .egress
            .connect_with_upgrade(server.address, &server.host, &server.url_path)
            .await?
        {
            StreamType::TcpStream(stream) => {
                protocol::process_tunnel(
                    stream,
                    client,
                    data_protocol,
                    target_host,
                    rng,
                    &server.protocol,
                    server.state.clone(),
                    self.cancel_watcher.create_handle(),
                )
                .await
            }
            StreamType::UpgradeStream(stream) => {
                protocol::process_tunnel(
                    stream,
                    client,
                    data_protocol,
                    target_host,
                    rng,
                    &server.protocol,
                    server.state.clone(),
                    self.cancel_watcher.create_handle(),
                )
                .await
            }
        }
        .or_else(|err| match err.downcast_ref::<std::io::Error>() {
            // rutls may return https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
            // ignore unexpected-eof it's not a problem in our case
            Some(io_err) if io_err.kind() == std::io::ErrorKind::UnexpectedEof => Ok(()),
            _ => Err(err),
        })
    }

    pub fn select_server(
        &self,
        target_host: &str,
        process_name: &str,
        mut rng: impl CryptoRng + Rng,
    ) -> Option<Arc<ServerContext>> {
        let table = self.table.load();
        let mut route = table.servers_by_process(process_name);

        if let RouteResult::NoRoute = route {
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

            // select server by domain
            for domain in &domain_variants {
                // any server, try check domain

                route = table.servers_by_domain(domain);
                match route {
                    RouteResult::NoRoute => {}
                    _ => break,
                };
            }
        }

        let servers = match route {
            RouteResult::Servers(servers) => servers,
            RouteResult::Direct => {
                if self.no_direct.load(Ordering::Relaxed) {
                    table.servers()
                } else {
                    return None;
                }
            } // direct route, no server
            RouteResult::NoRoute => table.servers(), // no route, try any server
        };

        if servers.is_empty() {
            return None;
        }

        let mut total_weight = 0_usize;
        let mut unweighted_count = 0_usize;
        for srv in servers.iter() {
            if let Some(weight) = srv.weight
                && weight > 0
            {
                total_weight += weight as usize;
            } else {
                unweighted_count += 1;
            }
        }

        let srv_count = servers.len();
        let avr_weight = if total_weight > 0 {
            total_weight / (srv_count - unweighted_count)
        } else if unweighted_count > 0 {
            100 / unweighted_count
        } else {
            return None;
        };

        let rnd_val = rng.gen_range(0..avr_weight * srv_count);

        let mut result = None;
        let mut cur_weight = 0_usize;
        for srv in servers.into_iter() {
            cur_weight += srv.weight.unwrap_or(avr_weight as u8) as usize;

            if cur_weight >= rnd_val {
                return Some(srv);
            } else if result.is_none() {
                result = Some(srv);
            }
        }

        result
    }
}
