use anyhow::{Result, anyhow};
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWriteExt},
    net::TcpListener,
    select,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

use crate::{
    egress::Egress,
    protocol::DataProtocol,
    router::Router,
    tun::dns_mapper::DnsMapper,
    utils::{cancel_watcher::CancellableTaskHandle, cancellable_task::CancellableTask},
};

const MIN_NAT_PORT: u16 = 10000;
const MAX_NAT_PORT: u16 = 65535;
const BIND_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_BIND_ATTEMPTS: u32 = 15;

const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub struct TcpProxySession {
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
}

struct TcpProxyClosedSession {
    pub src_addr: SocketAddr,
    pub port: u16,
    pub time: std::time::Instant,
}

pub struct TcpProxyNat {
    sessions: RwLock<FxHashMap<u16, Arc<TcpProxySession>>>,
    closed_sessions: Mutex<Vec<TcpProxyClosedSession>>,
    session_closer: CancellableTask,
    ports: RwLock<FxHashMap<SocketAddr, u16>>,
    port_index: AtomicU16,
    tcp_proxy_port: AtomicU16,
    dns_mapper: Arc<DnsMapper>,
    egress: Arc<Egress>,
}

impl TcpProxyNat {
    pub fn new(dns_mapper: Arc<DnsMapper>, egress: Arc<Egress>) -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(FxHashMap::default()),
            closed_sessions: Mutex::new(Vec::new()),
            session_closer: CancellableTask::new("TcpProxyNatSessionCloser"),
            ports: RwLock::new(FxHashMap::default()),
            port_index: AtomicU16::new(MIN_NAT_PORT),
            tcp_proxy_port: AtomicU16::new(0),
            dns_mapper,
            egress,
        })
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {
        let self_clone = self.clone();
        let mut interval = tokio::time::interval(SESSION_CLOSE_TIMEOUT);
        self.session_closer.spawn(|token| async move {
            loop {
                select! {
                    _ = interval.tick() => {}
                    _ = token.cancelled() => break,
                }

                let mut delete_sessions = Vec::new();
                let mut closed_sessions = self_clone.closed_sessions.lock();
                closed_sessions.retain(|session| {
                    if session.time.elapsed() >= SESSION_CLOSE_TIMEOUT {
                        delete_sessions.push((session.port, session.src_addr));
                        false
                    } else {
                        true
                    }
                });
                drop(closed_sessions);

                for (port, src_addr) in delete_sessions {
                    self_clone.delete_session(port, src_addr);
                }
            }
        });

        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) {
        self.session_closer.stop().await;

        self.sessions.write().clear();
        self.ports.write().clear();
    }

    pub async fn serve_proxy(
        self: &Arc<Self>,
        mut listener: TcpListener,
        if_addr: IpAddr,
        router: Arc<Router>,
        token: CancellationToken,
    ) -> Result<()> {
        loop {
            let result = tokio::select! {
                res = listener.accept() => res,
                _ = token.cancelled() => break,
            };
            match result {
                Ok((stream, client_addr)) => {
                    if let Err(err) = stream.set_nodelay(true) {
                        tracing::warn!("failed to set TCP_NODELAY on NAT accepted stream: {:?}", err);
                    }
                    let self_clone = self.clone();
                    let router = router.clone();
                    tokio::task::spawn(async move {
                        let port = client_addr.port();
                        let Ok(session) = self_clone.get_session(port) else {
                            tracing::error!("session not found for port {}", port);
                            return;
                        };

                        let dst_addr = session.dst_addr;
                        let host = match self_clone.dns_mapper.host_by_ip(dst_addr.ip()).await {
                            Ok(value) => value,
                            Err(err) => {
                                tracing::warn!("TCP NAT lookup host for {} failed: {:?}", dst_addr.ip(), err);
                                return;
                            }
                        };
                        let host = host.unwrap_or_else(|| dst_addr.ip().to_string());
                        let endpoint = format!("{host}:{}", dst_addr.port());

                        match router
                            .start_tunnel(stream, DataProtocol::Tcp, &endpoint, session.src_addr)
                            .await
                        {
                            Ok(Some((stream, cancel_handle))) => {
                                let use_dst_addr = match self_clone.egress.lookup_host(&host).await {
                                    Ok(Some(ip)) => SocketAddr::new(ip, dst_addr.port()),
                                    Ok(None) => {
                                        tracing::info!("TCP NAT lookup host returned no IP for {}", host);
                                        return;
                                    }
                                    Err(err) => {
                                        tracing::info!("TCP NAT lookup host {} failed: {:?}", host, err);
                                        return;
                                    }
                                };
                                self_clone
                                    .direct_transfer(stream, use_dst_addr, &endpoint, cancel_handle)
                                    .await;
                            }
                            Ok(None) => {}
                            Err(err) => {
                                tracing::warn!("server io error: {:?}", err);
                            }
                        }

                        self_clone.on_session_closed(port, session.src_addr);
                    });
                }
                Err(error) => {
                    drop(listener);
                    tracing::error!("accept failed: {:?}", error);
                    listener = self.bind_proxy(if_addr).await?;
                }
            }
        }
        Ok(())
    }

    async fn direct_transfer(
        self: &Arc<Self>,
        mut client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
        endpoint: &str,
        cancel_handle: CancellableTaskHandle,
    ) {
        let mut server = match self.egress.connect_tcp(target).await {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!("Direct connection to {} failed, err: {:?}", endpoint, err);
                return;
            }
        };

        select! {
            _ = cancel_handle.token.cancelled() => {},
            result = tokio::io::copy_bidirectional(&mut client, &mut server) => {
                if let Err(err) = result {
                    tracing::warn!("Direct io error: {:?}, target: {}", err, endpoint);
                }
            }
        }
    }

    pub async fn bind_proxy(self: &Arc<Self>, if_addr: IpAddr) -> Result<TcpListener> {
        let default_address = SocketAddr::new(if_addr, 0);

        // Bind may hang forever on a newly created interface (Windows bug, needs checking on Linux),
        // so retry with a timeout on each attempt.
        let mut last_err = None;
        for attempt in 1..=MAX_BIND_ATTEMPTS {
            match timeout(BIND_TIMEOUT, async { TcpListener::bind(default_address).await }).await {
                Ok(Ok(listener)) => {
                    let address = listener.local_addr()?;
                    tracing::info!("proxy server started: {:?}", address);
                    self.set_proxy_port(address.port());
                    return Ok(listener);
                }
                Ok(Err(err)) => {
                    if attempt > 4 {
                        tracing::warn!("bind attempt {}/{} failed: {}", attempt, MAX_BIND_ATTEMPTS, err);
                    }
                    last_err = Some(err.into());
                }
                Err(_) => {
                    if attempt > 4 {
                        tracing::warn!("bind attempt {}/{} timed out", attempt, MAX_BIND_ATTEMPTS);
                    }
                    last_err = Some(anyhow!("bind to {} timed out", default_address));
                }
            }
            sleep(BIND_TIMEOUT).await;
        }

        Err(last_err.unwrap_or_else(|| anyhow!("failed to bind to {}", default_address)))
    }

    pub fn get_session(&self, port: u16) -> Result<Arc<TcpProxySession>> {
        let sessions = self.sessions.read();
        sessions.get(&port).cloned().ok_or_else(|| anyhow!("Session not found"))
    }

    fn get_new_port(&self) -> u16 {
        MIN_NAT_PORT + self.port_index.fetch_add(1, Ordering::Relaxed) % (MAX_NAT_PORT - MIN_NAT_PORT)
    }

    pub fn get_port(&self, src_addr: SocketAddr, dst_addr: SocketAddr, create_session: bool) -> Option<u16> {
        if let Some(port) = self.ports.read().get(&src_addr) {
            return Some(*port);
        }

        if !create_session {
            return None;
        }

        let mut port = self.get_new_port();
        let tcp_proxy_port = self.tcp_proxy_port();
        while port == tcp_proxy_port || self.sessions.read().contains_key(&port) {
            port = self.get_new_port();
        }

        let session = Arc::new(TcpProxySession { src_addr, dst_addr });

        let mut ports_wr = self.ports.write();
        if let Some(port) = ports_wr.get(&src_addr) {
            return Some(*port);
        }

        self.sessions.write().insert(port, session);
        ports_wr.insert(src_addr, port);
        Some(port)
    }

    fn set_proxy_port(&self, port: u16) {
        self.tcp_proxy_port.store(port, Ordering::Relaxed);
    }

    pub fn tcp_proxy_port(&self) -> u16 {
        self.tcp_proxy_port.load(Ordering::Relaxed)
    }

    pub fn on_session_closed(&self, port: u16, src_addr: SocketAddr) {
        // delete session after timeout since there may be some packets in flight after session closed
        self.closed_sessions.lock().push(TcpProxyClosedSession {
            src_addr,
            port,
            time: std::time::Instant::now(),
        });
    }

    fn delete_session(&self, port: u16, src_addr: SocketAddr) {
        self.ports.write().remove(&src_addr);
        self.sessions.write().remove(&port);
    }
}
