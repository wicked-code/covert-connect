use anyhow::{Result, anyhow};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tun::DeviceWriter;

use crate::{router::Router, tun_tcp_proxy_nat::TcpProxyNat, tun_udp_nat::UdpNat};
use net_packet::ip::{IpHeader, IpPacket, NextHeader};

use network_interface::{NetworkInterface, NetworkInterfaceConfig};

pub const MAX_PACKET_SIZE: usize = 0xFFFF; // max IP packet size

#[derive(PartialEq)]
enum ProcessResult {
    Consume,
    WriteBack,
}

pub struct TunService {
    tcp_proxy_nat_v4: Arc<TcpProxyNat>,
    tcp_proxy_nat_v6: Arc<TcpProxyNat>,
    udp_nat: Arc<UdpNat>,
}

impl TunService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tcp_proxy_nat_v4: TcpProxyNat::new(),
            tcp_proxy_nat_v6: TcpProxyNat::new(),
            udp_nat: UdpNat::new(),
        })
    }

    pub async fn serve(self: &Arc<Self>, router: Arc<Router>) -> Result<()> {
        // TODO: ??? add stop and serve should run more than once!!!
        // stop: should remove router and writer from udp_proxy_nat
        let (if_addr_v4, if_addr_v6, writer) = self.init_tun().await?;

        self.tcp_proxy_nat_v4.init().await?;
        self.tcp_proxy_nat_v6.init().await?;
        self.udp_nat.init(router.clone(), writer).await?;

        let self_clone = self.clone();
        let router_clone = router.clone();
        tokio::spawn(async move {
            if let Err(e) = self_clone
                .tcp_proxy_nat_v6
                .serve_proxy(IpAddr::V6(if_addr_v6), router_clone)
                .await
            {
                tracing::warn!("IPv6 TCP proxy failed: {:?}", e);
            }
        });

        self.tcp_proxy_nat_v4
            .serve_proxy(IpAddr::V4(if_addr_v4), router.clone())
            .await
    }

    async fn init_tun(self: &Arc<Self>) -> Result<(Ipv4Addr, Ipv6Addr, DeviceWriter)> {
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

        let (writer, mut reader) = dev.split()?;

        // TODO: ??? probaly stop is needed to shutdown all gracefully...
        let self_clone = self.clone();
        let mut writer_clone = writer.clone();
        tokio::task::spawn(async move {
            // TODO: ??? get mtu from tun
            let mut read_buf = vec![0u8; MAX_PACKET_SIZE];
            let mut modify_buf = vec![0u8; MAX_PACKET_SIZE];
            loop {
                match reader.read(&mut read_buf).await {
                    Ok(n) => {
                        if n == 0 {
                            break;
                        }
                        modify_buf[..n].copy_from_slice(&read_buf[..n]);
                        let mut packet = &mut modify_buf[..n];
                        if self_clone
                            .process_packet(&mut packet, address_v4, gateaway_v4, address_v6, gateaway_v6)
                            .await
                            == ProcessResult::WriteBack
                        {
                            writer_clone.write_all(&packet).await.ok();
                        }
                    }
                    Err(err) => {
                        tracing::error!("tun read error: {:?}", err);
                        break;
                    }
                }
            }
        });

        Ok((address_v4, address_v6, writer))
    }

    async fn process_packet(
        &self,
        packet: &mut [u8],
        address_v4: Ipv4Addr,
        gateway_v4: Ipv4Addr,
        address_v6: Ipv6Addr,
        gateway_v6: Ipv6Addr,
    ) -> ProcessResult {
        let ip = match IpPacket::try_from(packet) {
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
                NextHeader::Udp(mut udp) => self.process_udp_v4_packet(&mut ipv4, &mut udp),
                NextHeader::Icmpv4(mut icmp) => self.process_icmp_v4_packet(&mut ipv4, &mut icmp),
                NextHeader::Igmp(mut igmp) => self.process_igmp_v4_packet(&mut ipv4, &mut igmp),
                _ => ProcessResult::Consume,
            },
            IpHeader::V6(mut ipv6) => match ip.next_header {
                NextHeader::Tcp(mut tcp) => self.process_tcp_v6_packet(&mut ipv6, &mut tcp, address_v6, gateway_v6),
                NextHeader::Udp(mut udp) => self.process_udp_v6_packet(&mut ipv6, &mut udp),
                NextHeader::Icmpv6(mut icmp) => self.process_icmp_v6_packet(&mut ipv6, &mut icmp),
                NextHeader::Igmp(mut igmp) => self.process_igmp_v6_packet(&mut ipv6, &mut igmp),
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
    ) -> ProcessResult {
        tracing::debug!("UDPv4 packet: {:?}", udp);
        self.udp_nat.send(
            SocketAddr::new(IpAddr::V4(ipv4.src_addr()), udp.src_port()),
            SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), udp.dst_port()),
            udp.payload(),
        );
        ProcessResult::Consume
    }

    fn process_udp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        udp: &mut net_packet::udp::UdpHeader,
    ) -> ProcessResult {
        self.udp_nat.send(
            SocketAddr::new(IpAddr::V6(ipv6.src_addr()), udp.src_port()),
            SocketAddr::new(IpAddr::V6(ipv6.dst_addr()), udp.dst_port()),
            udp.payload(),
        );
        ProcessResult::Consume
    }

    fn process_icmp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        icmp: &mut net_packet::icmpv4::Icmpv4Header,
    ) -> ProcessResult {
        tracing::debug!("ICMPv4 packet: {:?}, to {}", icmp, ipv4.dst_addr());
        ProcessResult::Consume
    }

    fn process_icmp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        icmp: &mut net_packet::icmpv6::Icmpv6Header,
    ) -> ProcessResult {
        tracing::debug!("ICMPv6 packet: {:?}, to {}", icmp, ipv6.dst_addr());
        ProcessResult::Consume
    }

    fn process_igmp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        igmp: &mut net_packet::igmp::IgmpHeader,
    ) -> ProcessResult {
        tracing::debug!("IGMPv4 packet: {:?}, to {}", igmp, ipv4.dst_addr());
        ProcessResult::Consume
    }

    fn process_igmp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        igmp: &mut net_packet::igmp::IgmpHeader,
    ) -> ProcessResult {
        tracing::debug!("IGMPv6 packet: {:?}, to {}", igmp, ipv6.dst_addr());
        ProcessResult::Consume
    }
}
