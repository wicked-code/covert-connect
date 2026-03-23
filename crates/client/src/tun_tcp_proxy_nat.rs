use anyhow::{Result, anyhow};
use parking_lot::{RwLock, Mutex};
use rustc_hash::FxHashMap;
use std::sync::atomic::Ordering;
use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicU16},
};

const MIN_NAT_PORT: u16 = 10000;
const MAX_NAT_PORT: u16 = 65535;

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
    ports: RwLock<FxHashMap<SocketAddr, u16>>,
    port_index: AtomicU16,
}

impl TcpProxyNat {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(FxHashMap::default()),
            closed_sessions: Mutex::new(Vec::new()),
            ports: RwLock::new(FxHashMap::default()),
            port_index: AtomicU16::new(MIN_NAT_PORT),
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

    pub fn get_session(&self, port: u16) -> Result<Arc<TcpProxySession>> {
        let sessions = self.sessions.read();
        sessions.get(&port).cloned().ok_or_else(|| anyhow!("Session not found"))
    }

    pub fn get_port(&self, src_addr: SocketAddr, dst_addr: SocketAddr, proxy_port: u16) -> u16 {
        let ports = self.ports.read();
        // TODO: ???? is it possible to have different src_addr.ip() but same src_addr.port()?
        match ports.get(&src_addr) {
            Some(port) => *port,
            None => {
                drop(ports);

                let mut port = self.port_index.fetch_add(1, Ordering::Relaxed);
                while port >= MAX_NAT_PORT || port == proxy_port || self.sessions.read().contains_key(&port) {
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

    pub fn on_session_closed(&self, port: u16, src_addr: SocketAddr) {
        // delete session after timeout since there may be some packets in flight after session closed
        self.closed_sessions.lock().push(TcpProxyClosedSession { src_addr, port, time: std::time::Instant::now() });
    }

    fn delete_session(&self, port: u16, src_addr: SocketAddr) {
        self.sessions.write().remove(&port);
        self.ports.write().remove(&src_addr);
    }
}
