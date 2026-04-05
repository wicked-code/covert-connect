use anyhow::Result;
use cc_server::udp::udp_transfer;
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncWriteExt},
    select,
};
use tun::DeviceWriter;

use crate::{
    cancel_watcher::CancellableTaskHandle,
    cancellable_task::CancellableTask,
    egress::Egress,
    protocol::DataProtocol,
    router::Router,
    streams::udp_stream::{UdpStream, UdpStreamData},
    tun::dns_mapper::DnsMapper,
};

// TODO: move to settings
const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

pub struct UdpNat {
    sessions: RwLock<FxHashMap<SocketAddr, Arc<UdpStreamData>>>,
    router: Mutex<Option<Arc<Router>>>,
    writer: Mutex<Option<DeviceWriter>>,
    is_icmp: bool,
    dns_mapper: Arc<DnsMapper>,
    egress: Arc<Egress>,
    session_closer: CancellableTask,
}

impl UdpNat {
    pub fn new(is_icmp: bool, dns_mapper: Arc<DnsMapper>, egress: Arc<Egress>) -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(FxHashMap::default()),
            session_closer: CancellableTask::new("UdpNatSessionCloser"),
            router: Mutex::new(None),
            writer: Mutex::new(None),
            is_icmp,
            dns_mapper,
            egress,
        })
    }

    pub async fn start(self: &Arc<Self>, router: Arc<Router>, writer: DeviceWriter) -> Result<()> {
        *self.router.lock() = Some(router);
        *self.writer.lock() = Some(writer);

        let self_clone = self.clone();
        self.session_closer.spawn(|token| async move {
            let mut interval = tokio::time::interval(SESSION_CLOSE_TIMEOUT);
            let mut delete_sessions: Vec<Arc<UdpStreamData>> = Vec::new();
            loop {
                for session in &delete_sessions {
                    session.done();
                }
                delete_sessions.clear();

                select! {
                    _ = interval.tick() => {}
                    _ = token.cancelled() => break,
                }

                let mut sessions = self_clone.sessions.write();
                sessions.retain(|_, session| {
                    if session.last_active().elapsed() >= SESSION_CLOSE_TIMEOUT {
                        delete_sessions.push(session.clone());
                        false
                    } else {
                        true
                    }
                });
            }
        });

        Ok(())
    }

    pub async fn stop(self: &Arc<Self>) {
        self.session_closer.stop().await;

        self.sessions.write().clear();
        self.router.lock().take();
        self.writer.lock().take();
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

                let stream = UdpStream::new(writer.clone(), src_addr, dst_addr, self_clone.is_icmp);
                let data = stream.data();
                self_clone.sessions.write().insert(src_addr, data.clone());
                data.send_packet(payload);

                let host = self_clone.dns_mapper.host_by_ip(dst_addr.ip());
                let host = host.map_or_else(
                    || dst_addr.to_string(),
                    |h| format!("{h}:{}", dst_addr.port().to_string()),
                );

                let data_protocol = if self_clone.is_icmp {
                    DataProtocol::Icmp
                } else {
                    DataProtocol::Udp
                };
                match router.start_tunnel(stream, data_protocol, host.clone(), src_addr).await {
                    Ok(Some((stream, cancel_handle))) => {
                        let use_dst_addr = self_clone.egress.lookup_host(&host).await.map_or_else(
                            || {
                                tracing::info!("UDP NAT lookup host failed {}", host);
                                dst_addr
                            },
                            |ip| SocketAddr::new(ip, dst_addr.port()),
                        );
                        if self_clone.is_icmp {
                            self_clone
                                .direct_transfer_icmp(stream, use_dst_addr, cancel_handle)
                                .await;
                        } else {
                            self_clone.direct_transfer(stream, use_dst_addr, cancel_handle).await;
                        }
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
        self: &Arc<Self>,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
        cancel_handle: CancellableTaskHandle,
    ) {
        tracing::info!("Direct connection (UDP) to {}", target);

        let out_socket = match self.egress.connect_udp(target).await {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!("Direct connection (UDP) to {} failed, err: {:?}", target, err);
                return;
            }
        };

        select! {
            _ = cancel_handle.token.cancelled() => {},
            result = udp_transfer(client, out_socket) => {
                if let Err(err) = result {
                    tracing::warn!("Direct connection (UDP) io error: {:?}, target: {}", err, target);
                }
            }
        }
    }

    async fn direct_transfer_icmp(
        self: &Arc<Self>,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
        cancel_handle: CancellableTaskHandle,
    ) {
        tracing::info!("Direct connection (ICMP) to {}", target);

        let out_socket = match self.egress.connect_icmp(target).await {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!("Direct connection (ICMP) to {} failed, err: {:?}", target, err);
                return;
            }
        };

        select! {
            _ = cancel_handle.token.cancelled() => {},
            result = udp_transfer(client, out_socket) => {
                if let Err(err) = result {
                    tracing::warn!("Direct connection (ICMP) io error: {:?}, target: {}", err, target);
                }
            }
        }
    }
}
