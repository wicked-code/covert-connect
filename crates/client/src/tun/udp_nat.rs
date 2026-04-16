use anyhow::{Result, anyhow};
use cc_server::{icmp::icmp_transfer, udp::udp_transfer};
use net_packet::ip::IpPacket;
use parking_lot::{Mutex, RwLock};
use rustc_hash::FxHashMap;
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    io::{AsyncRead, AsyncWriteExt},
    select,
};
use tun::DeviceWriter;

use crate::{
    egress::Egress,
    protocol::DataProtocol,
    router::Router,
    streams::udp_stream::{AddressOrHostWithOrig, UdpStream, UdpStreamData},
    tun::dns_mapper::DnsMapper,
    utils::{cancel_watcher::CancellableTaskHandle, cancellable_task::CancellableTask},
};

// TODO: move to settings
const SESSION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
const SESSION_CLOSE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

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
            let mut interval = tokio::time::interval(SESSION_CLOSE_CHECK_INTERVAL);
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
                let is_icmp = self_clone.is_icmp;
                let session = self_clone.sessions.read().get(&src_addr).cloned();

                let host = match self_clone.dns_mapper.host_by_ip(dst_addr.ip()).await {
                    Ok(value) => value,
                    Err(err) => {
                        tracing::warn!("UDP NAT lookup host for {} failed: {:?}", dst_addr.ip(), err);
                        Self::send_host_unreachable(writer, dst_addr, src_addr, &payload).await;
                        return;
                    }
                };

                let dst_address_or_host = match host {
                    Some(ref host) => AddressOrHostWithOrig::new(host.clone(), dst_addr),
                    None => AddressOrHostWithOrig::Address(dst_addr),
                };

                if let Some(session) = session {
                    if is_icmp {
                        session.send_icmp_packet(payload);
                    } else {
                        session.send_udp_packet(payload, dst_address_or_host);
                    }
                    return;
                }

                let stream = UdpStream::new(writer.clone(), src_addr, dst_addr, is_icmp);
                let data = stream.data();
                self_clone.sessions.write().insert(src_addr, data.clone());
                if is_icmp {
                    data.send_icmp_packet(payload);
                } else {
                    data.send_udp_packet(payload, dst_address_or_host);
                }

                let host = host.unwrap_or_else(|| dst_addr.ip().to_string());
                let endpoint = format!("{host}:{}", dst_addr.port());

                let data_protocol = if self_clone.is_icmp {
                    DataProtocol::Icmp
                } else {
                    DataProtocol::Udp
                };
                match router.start_tunnel(stream, data_protocol, endpoint, src_addr).await {
                    Ok(Some((stream, cancel_handle))) => {
                        let use_dst_addr = match self_clone.egress.lookup_host(&host).await {
                            Ok(Some(ip)) => SocketAddr::new(ip, dst_addr.port()),
                            Ok(None) => {
                                tracing::info!("UDP NAT lookup host returned no IP for {}", host);
                                return;
                            }
                            Err(err) => {
                                tracing::info!("UDP NAT lookup host {} failed: {:?}", host, err);
                                return;
                            }
                        };
                        if self_clone.is_icmp {
                            self_clone
                                .direct_transfer_icmp(stream, use_dst_addr, host, cancel_handle)
                                .await;
                        } else {
                            self_clone
                                .direct_transfer(stream, use_dst_addr, host, cancel_handle)
                                .await;
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

                data.last_active_expire(SESSION_CLOSE_TIMEOUT);
            }
        });
    }

    async fn direct_transfer(
        self: &Arc<Self>,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
        host: String,
        cancel_handle: CancellableTaskHandle,
    ) {
        tracing::info!("Direct connection (UDP) to {} ({})", target, host);

        let out_socket = match self.egress.bind_udp(target.is_ipv6()).await {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!(
                    "Direct connection (UDP) to {} ({}) failed, err: {:?}",
                    target,
                    host,
                    err
                );
                return;
            }
        };

        let egress_clone = self.egress.clone();
        select! {
            _ = cancel_handle.token.cancelled() => {},
            result = udp_transfer(client, out_socket, async move |host_and_port| {
                let (host, port) = host_and_port.split_once(':').ok_or_else(|| anyhow!("invalid host:port format: {}", host_and_port))?;
                let port: u16 = port.parse().map_err(|_| anyhow!("invalid port: {}", port))?;
                let address = egress_clone.lookup_host(host).await?.ok_or_else(|| anyhow!("lookup failed for {}", host))?;

                Ok(SocketAddr::new(address, port))
            }) => {
                if let Err(err) = result {
                    tracing::warn!("Direct connection (UDP) io error: {:?}, target: {} ({})", err, target, host);
                }
            }
        }
    }

    async fn direct_transfer_icmp(
        self: &Arc<Self>,
        client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
        host: String,
        cancel_handle: CancellableTaskHandle,
    ) {
        tracing::info!("Direct connection (ICMP) to {} ({})", target, host);

        let out_socket = match self.egress.connect_icmp(target).await {
            Ok(socket) => socket,
            Err(err) => {
                tracing::warn!(
                    "Direct connection (ICMP) to {} ({}) failed, err: {:?}",
                    target,
                    host,
                    err
                );
                return;
            }
        };

        select! {
            _ = cancel_handle.token.cancelled() => {},
            result = icmp_transfer(client, out_socket) => {
                if let Err(err) = result {
                    tracing::warn!("Direct connection (ICMP) io error: {:?}, target: {} ({})", err, target, host);
                }
            }
        }
    }

    async fn send_host_unreachable(mut writer: DeviceWriter, src_addr: SocketAddr, dst_addr: SocketAddr, payload: &[u8]) {
        let icmp_type = if src_addr.is_ipv4() {
            net_packet::icmpv4::ICMP_DEST_UNREACHABLE
        } else {
            net_packet::icmpv6::ICMPV6_DEST_UNREACHABLE
        };
        let icmp_code = if src_addr.is_ipv4() {
            net_packet::icmpv4::ICMP_UNREACH_HOST
        } else {
            net_packet::icmpv6::ICMPV6_UNREACH_ADDR
        };

        let packet = match IpPacket::build_icmp(
            icmp_type,
            icmp_code,
            0,
            src_addr,
            dst_addr,
            payload,
        ) {
            Ok(packet) => packet,
            Err(err) => {
                tracing::warn!("Failed to build Host Unreachable packet for {}: {:?}", src_addr, err);
                return;
            }
        };

        if let Err(err) = writer.write_all(&packet).await {
            tracing::warn!("Failed to send host unreachable packet for {}: {:?}", dst_addr, err);
        }
    }
}
