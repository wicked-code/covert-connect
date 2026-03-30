use crate::{
    egress_connector::EgressConnector, protocol::DataProtocol, router::Router, streams::udp_stream::{UdpStream, UdpStreamData}
};
use anyhow::Result;
use cc_server::udp::udp_transfer;
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::{net::SocketAddr, sync::Arc};
use tun::DeviceWriter;
use tokio::io::{AsyncRead, AsyncWriteExt};

// TODO: move to settings
const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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

                let stream = UdpStream::new(writer.clone(), src_addr, dst_addr);
                let data = stream.data();
                self_clone.sessions.write().insert(src_addr, data.clone());
                data.send_packet(payload);

                match router
                    .start_tunnel(stream, DataProtocol::Udp, dst_addr.to_string(), dst_addr, src_addr)
                    .await
                {
                    Ok(Some((stream, egress_connector))) => {
                        Self::direct_transfer(&egress_connector, stream, dst_addr).await;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::warn!(
                            "server io error: {:?}, udp: src_addr={}, dst_addr={}",
                            err,
                            src_addr,
                            dst_addr
                        );
                    }
                }
            }
        });
    }

    async fn direct_transfer(
        egress_connector: &Arc<EgressConnector>,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
    ) {
        tracing::info!("Direct connection (UDP) to {}", target);

        let out_socket = match egress_connector.connect_udp(target).await {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!("Direct connection (UDP) to {} failed, err: {:?}", target, err);
                return;
            }
        };

        if let Err(err) = udp_transfer(client, out_socket).await {
            tracing::warn!("Direct connection (UDP) io error: {:?}, target: {}", err, target);
        }
    }
}
