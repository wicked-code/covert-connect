use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Weak},
    time::Duration,
};

use anyhow::{Result, anyhow};
use tokio::{
    net::TcpListener,
    time::{sleep, timeout},
};

use crate::{
    router::Router,
    tun_tcp_proxy_nat::TcpProxyNat,
};
use futures_util::StreamExt;
use net_packet::ip::{IpHeader, IpPacket, NextHeader};

use network_interface::{NetworkInterface, NetworkInterfaceConfig};

const BIND_TIMEOUT: Duration = Duration::from_millis(1000);
const MAX_BIND_ATTEMPTS: u32 = 15;

#[derive(PartialEq)]
enum ProcessResult {
    Consume,
    WriteBack,
}

pub struct TunService {
    router: Weak<Router>,
    tcp_proxy_nat_v4: Arc<TcpProxyNat>,
    tcp_proxy_nat_v6: Arc<TcpProxyNat>,
}

impl TunService {
    pub fn new(router: Weak<Router>) -> Arc<Self> {
        Arc::new(Self {
            router,
            tcp_proxy_nat_v4: TcpProxyNat::new(),
            tcp_proxy_nat_v6: TcpProxyNat::new(),
        })
    }

    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        // TODO: ??? add stop and serve should run more than once!!!
        let (if_addr_v4, if_addr_v6) = self.init_tun().await?;

        self.tcp_proxy_nat_v4.init().await?;
        self.tcp_proxy_nat_v6.init().await?;

        let self_clone = self.clone();
        let tcp_proxy_nat_v6 = self.tcp_proxy_nat_v6.clone();
        tokio::spawn(async move {
            if let Err(e) = self_clone
                .serve_tcp_proxy(IpAddr::V6(if_addr_v6), tcp_proxy_nat_v6)
                .await
            {
                tracing::warn!("IPv6 TCP proxy failed: {:?}", e);
            }
        });

        self.serve_tcp_proxy(IpAddr::V4(if_addr_v4), self.tcp_proxy_nat_v4.clone())
            .await
    }

    async fn serve_tcp_proxy(self: &Arc<Self>, if_addr: IpAddr, tcp_proxy_nat: Arc<TcpProxyNat>) -> Result<()> {
        // TODO: ??? do not store weak reference, pass router to serve and then to serve_tcp_proxy
        let mut listener = self.bind_tcp_proxy(if_addr, tcp_proxy_nat.clone()).await?;
        loop {
            let result = listener.accept().await;
            match result {
                Ok((stream, client_addr)) => {
                    let tcp_proxy_nat = tcp_proxy_nat.clone();
                    let router = self.router.upgrade().unwrap().clone();
                    tokio::task::spawn(async move {
                        let port = client_addr.port();
                        let Ok(session) = tcp_proxy_nat.get_session(port) else {
                            tracing::error!("session not found for port {}", port);
                            return;
                        };

                        let target = session.dst_addr;
                        if let Err(err) = router
                            .start_tunnel(stream, target.to_string(), target, session.src_addr)
                            .await
                        {
                            tracing::warn!("server io error: {:?}", err);
                        }

                        tcp_proxy_nat.on_session_closed(port, session.src_addr);
                    });
                }
                Err(error) => {
                    drop(listener);
                    tracing::error!("accept failed: {:?}", error);
                    listener = self.bind_tcp_proxy(if_addr, tcp_proxy_nat.clone()).await?;
                }
            }
        }
    }

    async fn init_tun(self: &Arc<Self>) -> Result<(Ipv4Addr, Ipv6Addr)> {
        // TODO: ??? check adapters to select available ip for if
        let tun_name = "cc_tun";
        let address_v4 = Ipv4Addr::new(172, 23, 0, 1);
        let gateaway_v4 = Ipv4Addr::from(u32::from(address_v4) + 1);

        // TODO: ??? modify library
        // - clone writer to be able to write from other tasks without lock
        // - use sudo networksetup -ordernetworkservices to set priority for IF on macos
        // TODO: ??? setup dns
        let mut config = tun::Configuration::default();
        config
            .tun_name(tun_name)
            .address(address_v4)
            .netmask((255, 255, 255, 240))
            .destination(gateaway_v4)
            .up();

        // TODO: ??? test ensure_root_privileges on linux
        #[cfg(target_os = "linux")]
        config.platform_config(|config| {
            // requiring root privilege to acquire complete functions
            config.ensure_root_privileges(true);
        });

        let dev = tun::create_as_async(&config)?;

        // TODO: ??? may be we need to wait for IF to be ready after creation...
        let net_if = NetworkInterface::show()?
            .into_iter()
            .find(|x| {
                x.name == tun_name
                    && x.addr
                        .iter()
                        .any(|addr| matches!(addr, network_interface::Addr::V4(ifaddr) if ifaddr.ip == address_v4))
            })
            .ok_or_else(|| anyhow!("tun interface not found"))?;

        let address_v6 = net_if
            .addr
            .iter()
            .find_map(|addr| match addr.ip() {
                IpAddr::V6(addr_v6) => Some(addr_v6),
                IpAddr::V4(_) => None,
            })
            .unwrap_or(Ipv6Addr::UNSPECIFIED);
        let gateaway_v6 = Ipv6Addr::from(u128::from(address_v6) + 1);

        tracing::info!("tun interface: {:?}", net_if);

        // TODO: ??? probaly stop is needed to shutdown all gracefully...
        let self_clone = self.clone();
        tokio::task::spawn(async move {
            let mut framed = dev.into_framed();

            while let Some(packet) = framed.next().await {
                // TODO: spawn a task here
                // embed tun code into project and add clone for device or maybe for DeviceWriter only
                if let Ok(mut packet) = packet {
                    if self_clone
                        .process_packet(&mut packet, address_v4, gateaway_v4, address_v6, gateaway_v6)
                        .await
                        == ProcessResult::WriteBack
                    {
                        framed.get_ref().send(&packet).await.ok();
                    }
                } else {
                    tracing::error!("tun read error: {:?}", packet.err());
                }
            }
        });

        Ok((address_v4, address_v6))
    }

    async fn process_packet(
        &self,
        packet: &mut Vec<u8>,
        address_v4: Ipv4Addr,
        gateway_v4: Ipv4Addr,
        address_v6: Ipv6Addr,
        gateway_v6: Ipv6Addr,
    ) -> ProcessResult {
        let ip = match IpPacket::new(packet) {
            Ok(ip) => ip,
            Err(err) => {
                tracing::error!("Invalid packet: {:?}", err);
                tracing::error!("data: {:?}", packet);
                return ProcessResult::Consume;
            }
        };

        match ip.header {
            IpHeader::V4(mut ipv4) => match ip.next_header {
                NextHeader::Tcp(mut tcp) => self.process_tcp_v4_packet(&mut ipv4, &mut tcp, address_v4, gateway_v4),
                NextHeader::Udp(mut udp) => self.process_udp_v4_packet(&mut ipv4, &mut udp, address_v4, gateway_v4),
                NextHeader::Icmpv4(mut icmp) => {
                    self.process_icmp_v4_packet(&mut ipv4, &mut icmp, address_v4, gateway_v4)
                }
                NextHeader::Igmp(mut igmp) => self.process_igmp_v4_packet(&mut ipv4, &mut igmp, address_v4, gateway_v4),
                _ => ProcessResult::Consume,
            },
            IpHeader::V6(mut ipv6) => match ip.next_header {
                NextHeader::Tcp(mut tcp) => self.process_tcp_v6_packet(&mut ipv6, &mut tcp, address_v6, gateway_v6),
                NextHeader::Udp(mut udp) => self.process_udp_v6_packet(&mut ipv6, &mut udp, address_v6, gateway_v6),
                NextHeader::Icmpv6(mut icmp) => {
                    self.process_icmp_v6_packet(&mut ipv6, &mut icmp, address_v6, gateway_v6)
                }
                NextHeader::Igmp(mut igmp) => self.process_igmp_v6_packet(&mut ipv6, &mut igmp, address_v6, gateway_v6),
                _ => ProcessResult::Consume,
            },
        }
    }

    fn process_tcp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        tcp: &mut net_packet::tcp::TcpHeader,
        address_v4: Ipv4Addr,
        gateway_v4: Ipv4Addr,
    ) -> ProcessResult {
        let tcp_proxy_port = self.tcp_proxy_nat_v4.tcp_proxy_port();
        if tcp.src_port() == tcp_proxy_port && ipv4.src_addr() == address_v4 {
            let Ok(session) = self.tcp_proxy_nat_v4.get_session(tcp.dst_port()) else {
                tracing::error!("session not found for port {}", tcp.dst_port());
                return ProcessResult::Consume;
            };

            let IpAddr::V4(src_ip_v4) = session.src_addr.ip() else {
                tracing::error!("invalid session for port {}", tcp.dst_port());
                return ProcessResult::Consume;
            };
            let IpAddr::V4(dst_ip_v4) = session.dst_addr.ip() else {
                tracing::error!("invalid session for port {}", tcp.dst_port());
                return ProcessResult::Consume;
            };

            ipv4.set_src_addr(dst_ip_v4);
            tcp.set_src_port(session.dst_addr.port());
            ipv4.set_dst_addr(src_ip_v4);
            tcp.set_dst_port(session.src_addr.port());

            ipv4.compute_checksum();
            tcp.compute_checksum_v4(src_ip_v4, dst_ip_v4);
        } else {
            let nat_port = self.tcp_proxy_nat_v4.get_port(
                SocketAddr::new(IpAddr::V4(ipv4.src_addr()), tcp.src_port()),
                SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), tcp.dst_port()),
            );

            ipv4.set_src_addr(gateway_v4);
            tcp.set_src_port(nat_port);
            ipv4.set_dst_addr(address_v4);
            tcp.set_dst_port(tcp_proxy_port);

            ipv4.compute_checksum();
            tcp.compute_checksum_v4(gateway_v4, address_v4);
        }

        ProcessResult::WriteBack
    }

    fn process_tcp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        tcp: &mut net_packet::tcp::TcpHeader,
        address_v6: Ipv6Addr,
        gateway_v6: Ipv6Addr,
    ) -> ProcessResult {
        let tcp_proxy_port = self.tcp_proxy_nat_v6.tcp_proxy_port();
        if tcp.src_port() == tcp_proxy_port && ipv6.src_addr() == address_v6 {
            let Ok(session) = self.tcp_proxy_nat_v6.get_session(tcp.dst_port()) else {
                tracing::error!("session not found for port {}", tcp.dst_port());
                return ProcessResult::Consume;
            };

            let IpAddr::V6(src_ip_v6) = session.src_addr.ip() else {
                tracing::error!("invalid session for port {}", tcp.dst_port());
                return ProcessResult::Consume;
            };
            let IpAddr::V6(dst_ip_v6) = session.dst_addr.ip() else {
                tracing::error!("invalid session for port {}", tcp.dst_port());
                return ProcessResult::Consume;
            };

            ipv6.set_src_addr(dst_ip_v6);
            tcp.set_src_port(session.dst_addr.port());
            ipv6.set_dst_addr(src_ip_v6);
            tcp.set_dst_port(session.src_addr.port());

            tcp.compute_checksum_v6(src_ip_v6, dst_ip_v6);
        } else {
            let nat_port = self.tcp_proxy_nat_v6.get_port(
                SocketAddr::new(IpAddr::V6(ipv6.src_addr()), tcp.src_port()),
                SocketAddr::new(IpAddr::V6(ipv6.dst_addr()), tcp.dst_port()),
            );

            ipv6.set_src_addr(gateway_v6);
            tcp.set_src_port(nat_port);
            ipv6.set_dst_addr(address_v6);
            tcp.set_dst_port(tcp_proxy_port);

            tcp.compute_checksum_v6(gateway_v6, address_v6);
        }

        ProcessResult::WriteBack
    }

    fn process_udp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        udp: &mut net_packet::udp::UdpHeader,
        address_v4: Ipv4Addr,
        gateway_v4: Ipv4Addr,
    ) -> ProcessResult {
        tracing::debug!("UDPv4 packet: {:?}", udp);
        ProcessResult::Consume
    }

    fn process_udp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        udp: &mut net_packet::udp::UdpHeader,
        address_v6: Ipv6Addr,
        gateway_v6: Ipv6Addr,
    ) -> ProcessResult {
        tracing::debug!("UDPv6 packet: {:?}", udp);
        ProcessResult::Consume
    }

    fn process_icmp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        icmp: &mut net_packet::icmpv4::Icmpv4Header,
        address_v4: Ipv4Addr,
        gateway_v4: Ipv4Addr,
    ) -> ProcessResult {
        tracing::debug!("ICMPv4 packet: {:?}", icmp);
        ProcessResult::Consume
    }

    fn process_icmp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        icmp: &mut net_packet::icmpv6::Icmpv6Header,
        address_v6: Ipv6Addr,
        gateway_v6: Ipv6Addr,
    ) -> ProcessResult {
        tracing::debug!("ICMPv6 packet: {:?}", icmp);
        ProcessResult::Consume
    }

    fn process_igmp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        igmp: &mut net_packet::igmp::IgmpHeader,
        address_v4: Ipv4Addr,
        gateway_v4: Ipv4Addr,
    ) -> ProcessResult {
        tracing::debug!("IGMPv4 packet: {:?}", igmp);
        ProcessResult::Consume
    }

    fn process_igmp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        igmp: &mut net_packet::igmp::IgmpHeader,
        address_v6: Ipv6Addr,
        gateway_v6: Ipv6Addr,
    ) -> ProcessResult {
        tracing::debug!("IGMPv6 packet: {:?}", igmp);
        ProcessResult::Consume
    }

    async fn bind_tcp_proxy(self: &Arc<Self>, if_addr: IpAddr, tcp_proxy_nat: Arc<TcpProxyNat>) -> Result<TcpListener> {
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
                    tcp_proxy_nat.set_tcp_proxy_port(address.port());
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
}
