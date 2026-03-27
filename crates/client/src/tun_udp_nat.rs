use crate::{
    protocol::DataProtocol,
    router::Router,
    streams::udp_stream::{UdpStream, UdpStreamData},
};
use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::{net::SocketAddr, sync::Arc};
use tun::DeviceWriter;

const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub struct UdpNat {
    sessions: RwLock<FxHashMap<SocketAddr, Arc<UdpStreamData>>>,
    router: Mutex<Option<Arc<Router>>>,
    writer: Mutex<Option<DeviceWriter>>,
}

impl UdpNat {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(FxHashMap::default()),
            router: Mutex::new(None),
            writer: Mutex::new(None),
        })
    }

    pub async fn init(self: &Arc<Self>, router: Arc<Router>, writer: DeviceWriter) -> Result<()> {
        *self.router.lock() = Some(router);
        *self.writer.lock() = Some(writer);

        let self_clone = self.clone();
        let mut interval = tokio::time::interval(SESSION_CLOSE_TIMEOUT);
        tokio::spawn(async move {
            loop {
                interval.tick().await;

                let mut delete_sessions = Vec::new();
                let mut sessions = self_clone.sessions.write();
                sessions.retain(|_, session| {
                    if session.last_active().elapsed() >= SESSION_CLOSE_TIMEOUT {
                        delete_sessions.push(session.clone());
                        false
                    } else {
                        true
                    }
                });
                drop(sessions);

                for session in delete_sessions {
                    session.done();
                }
            }
        });

        Ok(())
    }

    pub fn send(self: &Arc<Self>, src_addr: SocketAddr, dst_addr: SocketAddr, payload: &[u8]) {
        tokio::task::spawn({
            let writer = if let Some(writer) = &*self.writer.lock() {
                writer.clone()
            } else {
                tracing::warn!("UDP NAT not initialized, writer not found");
                return;
            };

            let router = if let Some(router) = &*self.router.lock() {
                router.clone()
            } else {
                tracing::warn!("UDP NAT not initialized, router not found");
                return;
            };

            let payload = payload.to_vec();
            let self_clone = self.clone();
            async move {
                let session = self_clone.sessions.read().get(&src_addr).cloned();
                if let Some(session) = session {
                    session.send_packet(payload);
                    return;
                }

                let stream = UdpStream::new(writer.clone());
                let data = stream.data();
                self_clone.sessions.write().insert(src_addr, data);

                if let Err(err) = router
                    .start_tunnel(stream, DataProtocol::Udp, dst_addr.to_string(), dst_addr, src_addr)
                    .await
                {
                    tracing::warn!("server io error: {:?}", err);
                }
            }
        });
    }
}
