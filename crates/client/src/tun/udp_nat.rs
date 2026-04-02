use anyhow::Result;
use cc_server::udp::udp_transfer;
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::{net::SocketAddr, sync::Arc};
use tokio::io::{AsyncRead, AsyncWriteExt};
use tun::DeviceWriter;

use crate::{
    egress_connector::EgressConnector,
    protocol::DataProtocol,
    router::Router,
    streams::udp_stream::{UdpStream, UdpStreamData},
    tun::dns_mapper::DnsMapper,
};

// TODO: move to settings
const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub struct UdpNat {
    sessions: RwLock<FxHashMap<SocketAddr, Arc<UdpStreamData>>>,
    router: Mutex<Option<Arc<Router>>>,
    writer: Mutex<Option<DeviceWriter>>,
    is_icmp: bool,
    dns_mapper: Arc<DnsMapper>,
}

impl UdpNat {
    pub fn new(is_icmp: bool, dns_mapper: Arc<DnsMapper>) -> Arc<Self> {
        Arc::new(Self {
            sessions: RwLock::new(FxHashMap::default()),
            router: Mutex::new(None),
            writer: Mutex::new(None),
            is_icmp,
            dns_mapper,
        })
    }

    pub async fn init(self: &Arc<Self>, router: Arc<Router>, writer: DeviceWriter) -> Result<()> {
        *self.router.lock() = Some(router);
        *self.writer.lock() = Some(writer);

        let self_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SESSION_CLOSE_TIMEOUT);
            let mut delete_sessions: Vec<Arc<UdpStreamData>> = Vec::new();
            loop {
                for session in &delete_sessions {
                    session.done();
                }
                delete_sessions.clear();

                interval.tick().await;

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
                    Ok(Some((stream, egress_connector))) => {
                        let use_dst_addr = egress_connector.lookup_host(&host).await.map_or_else(
                            || {
                                tracing::info!("UDP NAT lookup host failed {}", host);
                                dst_addr
                            },
                            |ip| SocketAddr::new(ip, dst_addr.port()),
                        );
                        if self_clone.is_icmp {
                            Self::direct_transfer_icmp(&egress_connector, stream, use_dst_addr).await;
                        } else {
                            Self::direct_transfer(&egress_connector, stream, use_dst_addr).await;
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

    async fn direct_transfer_icmp(
        egress_connector: &Arc<EgressConnector>,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
    ) {
        tracing::info!("Direct connection (ICMP) to {}", target);

        let out_socket = match egress_connector.connect_icmp(target).await {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!("Direct connection (ICMP) to {} failed, err: {:?}", target, err);
                return;
            }
        };

        if let Err(err) = udp_transfer(client, out_socket).await {
            tracing::warn!("Direct connection (ICMP) io error: {:?}, target: {}", err, target);
        }
    }
}
