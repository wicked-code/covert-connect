use crate::{egress_connector::EgressConnector, protocol::DataProtocol, router::Router};
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
    net::TcpListener,
    time::{sleep, timeout},
    io::{AsyncRead, AsyncWriteExt},
};

const MIN_NAT_PORT: u16 = 10000;
const MAX_NAT_PORT: u16 = 65535;
const BIND_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_BIND_ATTEMPTS: u32 = 15;

const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
    ports: RwLock<FxHashMap<SocketAddr, u16>>,
    port_index: AtomicU16,
    tcp_proxy_port: AtomicU16,
}

impl TcpProxyNat {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(FxHashMap::default()),
            closed_sessions: Mutex::new(Vec::new()),
            ports: RwLock::new(FxHashMap::default()),
            port_index: AtomicU16::new(MIN_NAT_PORT),
            tcp_proxy_port: AtomicU16::new(0),
        })
    }

    pub async fn init(self: &Arc<Self>) -> Result<()> {
        let self_clone = self.clone();
        let mut interval = tokio::time::interval(SESSION_CLOSE_TIMEOUT);
        tokio::spawn(async move {
            loop {
                interval.tick().await;

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

    pub async fn serve_proxy(self: &Arc<Self>, if_addr: IpAddr, router: Arc<Router>) -> Result<()> {
        let mut listener = self.bind_proxy(if_addr).await?;
        loop {
            let result = listener.accept().await;
            match result {
                Ok((stream, client_addr)) => {
                    let self_clone = self.clone();
                    let router = router.clone();
                    tokio::task::spawn(async move {
                        let port = client_addr.port();
                        let Ok(session) = self_clone.get_session(port) else {
                            tracing::error!("session not found for port {}", port);
                            return;
                        };

                        let target = session.dst_addr;
                        match router
                            .start_tunnel(stream, DataProtocol::Tcp, target.to_string(), target, session.src_addr)
                            .await
                        {
                            Ok(Some((stream, egress_connector))) => {
                                Self::direct_transfer(&egress_connector, stream, target).await;
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
    }

    async fn direct_transfer(
        egress_connector: &Arc<EgressConnector>,
        mut client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
    ) {
        tracing::info!("Direct connection to {}", target);

        let mut server = match egress_connector.connect_tcp(target).await {
            Ok(stream) => stream,
            Err(err) => {
                tracing::warn!("Direct connection to {} failed, err: {:?}", target, err);
                return;
            }
        };

        if let Err(err) = tokio::io::copy_bidirectional(&mut client, &mut server).await {
            tracing::warn!("Direct connection io error: {:?}, target: {}", err, target);
        }
    }

    async fn bind_proxy(self: &Arc<Self>, if_addr: IpAddr) -> Result<TcpListener> {
        let default_address = SocketAddr::new(if_addr, 0);

        // Bind may hang forever on a newly created interface (Windows bug, needs checking on Linux),
        // so retry with a timeout on each attempt.
        let mut last_err = None;
        for attempt in 1..=MAX_BIND_ATTEMPTS {
            match timeout(BIND_TIMEOUT, async {
                sleep(BIND_TIMEOUT).await;
                TcpListener::bind(default_address).await
            })
            .await
            {
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
        }

        Err(last_err.unwrap_or_else(|| anyhow!("failed to bind to {}", default_address)))
    }

    pub fn get_session(&self, port: u16) -> Result<Arc<TcpProxySession>> {
        let sessions = self.sessions.read();
        sessions.get(&port).cloned().ok_or_else(|| anyhow!("Session not found"))
    }

    pub fn get_port(&self, src_addr: SocketAddr, dst_addr: SocketAddr) -> u16 {
        let ports = self.ports.read();
        // TODO: ???? is it possible to have different src_addr.ip() but same src_addr.port()?
        match ports.get(&src_addr) {
            Some(port) => *port,
            None => {
                drop(ports);

                let mut port = self.port_index.fetch_add(1, Ordering::Relaxed);
                let tcp_proxy_port = self.tcp_proxy_port();
                while port >= MAX_NAT_PORT || port == tcp_proxy_port || self.sessions.read().contains_key(&port) {
                    if port >= MAX_NAT_PORT {
                        self.port_index.store(MIN_NAT_PORT, Ordering::Relaxed);
                        port = MIN_NAT_PORT;
                    } else {
                        port = self.port_index.fetch_add(1, Ordering::Relaxed);
                    }
                }

                let session = Arc::new(TcpProxySession { src_addr, dst_addr });

                self.sessions.write().insert(port, session);
                self.ports.write().insert(src_addr, port);
                port
            }
        }
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
        self.sessions.write().remove(&port);
        self.ports.write().remove(&src_addr);
    }
}
