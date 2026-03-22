use anyhow::{Result, anyhow};
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::atomic::Ordering;
use std::{
    net::SocketAddr,
    sync::{Arc, atomic::AtomicU16},
};

pub struct TcpProxySession {
    pub src_addr: SocketAddr,
    pub dst_addr: SocketAddr,
}

pub struct TcpProxyNat {
    sessions: RwLock<FxHashMap<u16, Arc<TcpProxySession>>>,
    ports: RwLock<FxHashMap<SocketAddr, u16>>,
    port_index: AtomicU16,
}

impl TcpProxyNat {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(FxHashMap::default()),
            ports: RwLock::new(FxHashMap::default()),
            port_index: AtomicU16::new(10000),
        }
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

                // TODO: ??? handle port overflow (store empty port)
                let port = self.port_index.fetch_add(1, Ordering::Relaxed);
                if port + 1 == proxy_port {
                    self.port_index.fetch_add(1, Ordering::Relaxed);
                }

                let session = Arc::new(TcpProxySession { src_addr, dst_addr });

                self.sessions.write().insert(port, session);
                self.ports.write().insert(src_addr, port);
                port
            }
        }
    }

    pub fn delete_session(&self, port: u16, src_addr: SocketAddr) {
        self.sessions.write().remove(&port);
        self.ports.write().remove(&src_addr);
    }
}
