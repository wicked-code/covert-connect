use anyhow::{Result, anyhow, bail};
use parking_lot::Mutex;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
#[cfg(not(target_os = "windows"))]
use sys_net::setup_dns;
#[cfg(target_os = "macos")]
use sys_net::{setup_routes, teardown_dns, teardown_routes, reset_network};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    select,
    task::JoinSet,
    time::sleep,
};
use tokio_util::sync::CancellationToken;
use tun::{AbstractDevice, DeviceWriter};

use crate::{
    egress::Egress,
    router::Router,
    tun::{dns_mapper::DnsMapper, dns_server::DnsServer, tcp_proxy_nat::TcpProxyNat, udp_nat::UdpNat},
    utils::cancellable_task::CancellableTask,
};
use net_packet::ip::{IpHeader, IpPacket, NextHeader};

use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use sys_net::flush_system_dns_cache;

const MAX_PACKET_SIZE: usize = 0xFFFF; // max IP packet size

const WAIT_IF_READY_TIMEOUT: Duration = Duration::from_secs(15);
const WAIT_IF_READY_INTERVAL: Duration = Duration::from_millis(100);

#[derive(PartialEq)]
enum ProcessResult {
    Consume,
    WriteBack,
}

pub struct TunService {
    dns_server: Arc<DnsServer>,
    tcp_proxy_nat_v4: Arc<TcpProxyNat>,
    tcp_proxy_nat_v6: Arc<TcpProxyNat>,
    udp_nat: Arc<UdpNat>,
    icmp_nat: Arc<UdpNat>,
    tun_loop_task: Arc<CancellableTask>,
    ipv6_serve_task: Arc<CancellableTask>,
    ipv4_serve_cancellation: Mutex<Option<CancellationToken>>,
    #[cfg(target_os = "macos")]
    tun_name: Mutex<Option<String>>,
    #[cfg(target_os = "macos")]
    enable_ipv6: Mutex<bool>,
}

impl TunService {
    pub fn new(egress: Arc<Egress>) -> Arc<Self> {
        let dns_mapper = DnsMapper::new();
        Arc::new(Self {
            tcp_proxy_nat_v4: TcpProxyNat::new(dns_mapper.clone(), egress.clone()),
            tcp_proxy_nat_v6: TcpProxyNat::new(dns_mapper.clone(), egress.clone()),
            udp_nat: UdpNat::new(false, dns_mapper.clone(), egress.clone()),
            icmp_nat: UdpNat::new(true, dns_mapper.clone(), egress),
            dns_server: DnsServer::new(dns_mapper),
            tun_loop_task: Arc::new(CancellableTask::new("TunServiceTunLoopTask")),
            ipv6_serve_task: Arc::new(CancellableTask::new("TunServiceIpv6ServeTask")),
            ipv4_serve_cancellation: Mutex::new(None),
            #[cfg(target_os = "macos")]
            tun_name: Mutex::new(None),
            #[cfg(target_os = "macos")]
            enable_ipv6: Mutex::new(false),
        })
    }

    pub async fn cleanup_at_start() {
        #[cfg(target_os = "macos")]
        teardown_dns();
    }

    pub async fn stop(&self) {
        // stop in parallel and wait for all to stop
        let mut set = JoinSet::new();
        set.spawn({
            let task = self.tun_loop_task.clone();
            async move { task.stop().await }
        });
        set.spawn({
            let task = self.ipv6_serve_task.clone();
            async move { task.stop().await }
        });
        set.spawn({
            let server = self.dns_server.clone();
            async move {
                if let Err(err) = server.stop().await {
                    tracing::error!("stop dns server error: {:?}", err);
                }
            }
        });
        set.spawn({
            let nat = self.tcp_proxy_nat_v4.clone();
            async move { nat.stop().await }
        });
        set.spawn({
            let nat = self.tcp_proxy_nat_v6.clone();
            async move { nat.stop().await }
        });
        set.spawn({
            let nat = self.udp_nat.clone();
            async move { nat.stop().await }
        });
        set.spawn({
            let nat = self.icmp_nat.clone();
            async move { nat.stop().await }
        });
        #[cfg(target_os = "macos")]
        {
            if let Some(tun_name) = self.tun_name.lock().clone() {
                set.spawn({
                    let enable_ipv6 = *self.enable_ipv6.lock();
                    async move {
                        teardown_routes(&tun_name, enable_ipv6);
                    }
                });
            }
        }

        while let Some(res) = set.join_next().await {
            if let Err(err) = res {
                tracing::error!("shutdown error: {:?}", err);
            }
        }

        if let Some(token) = self.ipv4_serve_cancellation.lock().take() {
            token.cancel()
        }

        #[cfg(target_os = "macos")]
        {
            *self.tun_name.lock() = None;
            *self.enable_ipv6.lock() = false;
            reset_network();
        }

        Self::cleanup_at_start().await;

        // Flush system DNS cache after stop
        if let Err(err) = flush_system_dns_cache().await {
            tracing::error!("flush system DNS cache error: {:?}", err);
        }
    }

    pub async fn serve(
        self: &Arc<Self>,
        router: Arc<Router>,
        on_started: impl FnOnce() + Send + 'static,
    ) -> Result<()> {
        let (if_addr_v4, if_addr_v6, writer) = self.init_tun().await?;

        self.tcp_proxy_nat_v4.start().await?;
        self.tcp_proxy_nat_v6.start().await?;
        self.udp_nat.start(router.clone(), writer.clone()).await?;
        self.icmp_nat.start(router.clone(), writer).await?;

        let self_clone = self.clone();
        let router_clone = router.clone();
        self.ipv6_serve_task.spawn(|token| async move {
            let listener = match self_clone.tcp_proxy_nat_v6.bind_proxy(IpAddr::V6(if_addr_v6)).await {
                Ok(listener) => listener,
                Err(e) => {
                    tracing::warn!("IPv6 TCP proxy failed to bind: {:?}", e);
                    return;
                }
            };

            if let Err(e) = self_clone
                .tcp_proxy_nat_v6
                .serve_proxy(listener, IpAddr::V6(if_addr_v6), router_clone, token)
                .await
            {
                tracing::warn!("IPv6 TCP proxy failed: {:?}", e);
            }
        });

        let listener = self.tcp_proxy_nat_v4.bind_proxy(IpAddr::V4(if_addr_v4)).await?;
        self.dns_server.start(if_addr_v4).await?;

        tokio::spawn(async {
            // TODO: ??? delay for dns start?
            // send actual request to our DNS and continue after answer or timeout (FLUSH_DNS_DELAY)
            flush_system_dns_cache()
                .await
                .inspect_err(|e| tracing::error!("flush dns error: {:?}", e))
                .ok();

            #[cfg(target_os = "macos")]
            reset_network();
        });

        let token = CancellationToken::new();
        self.ipv4_serve_cancellation.lock().replace(token.clone());

        on_started();

        self.tcp_proxy_nat_v4
            .serve_proxy(listener, IpAddr::V4(if_addr_v4), router.clone(), token)
            .await
    }

    async fn init_tun(self: &Arc<Self>) -> Result<(Ipv4Addr, Ipv6Addr, DeviceWriter)> {
        let mut tun_name = "cc_tun".to_string();
        let address_v4 = find_if_address()?;
        let gateaway_v4 = Ipv4Addr::from(u32::from(address_v4) + 1);

        let mut config = tun::Configuration::default();
        config
            .address(address_v4)
            .netmask((255, 255, 255, 240))
            .destination(gateaway_v4)
            .mtu(1500)
            .up();

        #[cfg(not(target_os = "macos"))]
        config.tun_name(&tun_name);

        #[cfg(target_os = "windows")]
        {
            use sys_net::fix_firewall;

            config.platform_config(|config| {
                config.dns_servers(&[IpAddr::V4(address_v4)]);
            });
            fix_firewall();
        }

        // TODO: ??? test ensure_root_privileges on linux
        #[cfg(target_os = "linux")]
        config.platform_config(|config| {
            // requiring root privilege to acquire complete functions
            config.ensure_root_privileges(true);
        });

        let dev = tun::create_as_async(&config)?;
        tun_name = dev.tun_name().unwrap_or(tun_name.to_string());

        // This method does't wait for tun to be fully up, but return address immediately
        // but it returns all addresses include IPv6 address, so we can use it to get IPv6 address and gateway
        let net_if = NetworkInterface::show()?
            .into_iter()
            .find(|x| {
                x.name == tun_name
                    && x.addr
                        .iter()
                        .any(|addr| matches!(addr, network_interface::Addr::V4(ifaddr) if ifaddr.ip == address_v4))
            })
            .ok_or_else(|| anyhow!("tun interface not found"))?;

        wait_interface_ready(&tun_name).await?;

        // remove Multicast
        #[cfg(target_os = "windows")]
        {
            let handle = net_route::Handle::new()?;
            let route = net_route::Route::new("224.0.0.0".parse().unwrap(), 4)
                .with_ifindex(net_if.index)
                .with_gateway(IpAddr::V4(address_v4));
            if let Err(err) = handle.delete(&route).await {
                tracing::warn!("delete multicast route error: {:?}", err);
            }
        }

        let address_v6 = net_if
            .addr
            .iter()
            .find_map(|addr| match addr.ip() {
                IpAddr::V6(addr_v6) => Some(addr_v6),
                IpAddr::V4(_) => None,
            })
            .unwrap_or(Ipv6Addr::UNSPECIFIED);
        let gateaway_v6 = Ipv6Addr::from(u128::from(address_v6) + 1);

        // setup dns
        #[cfg(not(target_os = "windows"))]
        setup_dns(&tun_name, IpAddr::V4(address_v4))?;
        #[cfg(target_os = "macos")]
        {
            *self.tun_name.lock() = Some(tun_name.clone());

            let enable_ipv6 = address_v6 != Ipv6Addr::UNSPECIFIED;
            *self.enable_ipv6.lock() = enable_ipv6;

            setup_routes(&tun_name, enable_ipv6)?;
        }

        let (writer, mut reader) = dev.split()?;

        let self_clone = self.clone();
        let mut writer_clone = writer.clone();
        self.tun_loop_task.spawn(|token| async move {
            // despite 1500 MTU, tcp packets may be larger, no reason to save memory here
            let mut read_buf = vec![0u8; MAX_PACKET_SIZE];
            let mut modify_buf = vec![0u8; MAX_PACKET_SIZE];
            loop {
                select! {
                    _ = token.cancelled() => break,
                    res = reader.read(&mut read_buf) => {
                        match res {
                            Ok(n) => {
                                if n == 0 {
                                    break;
                                }
                                modify_buf[..n].copy_from_slice(&read_buf[..n]);
                                let packet = &mut modify_buf[..n];
                                if self_clone
                                    .process_packet(packet, address_v4, gateaway_v4, address_v6, gateaway_v6)
                                    == ProcessResult::WriteBack
                                {
                                    writer_clone.write_all(packet).await.ok();
                                }
                            }
                            Err(err) => {
                                tracing::error!("tun read error: {:?}", err);
                                break;
                            }
                        }
                    }
                }
            }
        });

        Ok((address_v4, address_v6, writer))
    }

    fn process_packet(
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
                NextHeader::Tcp(mut tcp) => {
                    return self.process_tcp_v4_packet(&mut ipv4, &mut tcp, address_v4, gateway_v4);
                }
                NextHeader::Udp(mut udp) => return self.process_udp_v4_packet(&mut ipv4, &mut udp, address_v4),
                NextHeader::Icmpv4(mut icmp) => self.process_icmp_v4_packet(&mut ipv4, &mut icmp),
                NextHeader::Igmp(mut igmp) => self.process_igmp_v4_packet(&mut ipv4, &mut igmp),
                _ => (),
            },
            IpHeader::V6(mut ipv6) => match ip.next_header {
                NextHeader::Tcp(mut tcp) => {
                    return self.process_tcp_v6_packet(&mut ipv6, &mut tcp, address_v6, gateway_v6);
                }
                NextHeader::Udp(mut udp) => self.process_udp_v6_packet(&mut ipv6, &mut udp),
                NextHeader::Icmpv6(mut icmp) => self.process_icmp_v6_packet(&mut ipv6, &mut icmp),
                NextHeader::Igmp(mut igmp) => self.process_igmp_v6_packet(&mut ipv6, &mut igmp),
                _ => (),
            },
        }
        ProcessResult::Consume
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
            if should_reinject_local_dns_v4(ipv4.dst_addr(), address_v4) {
                return ProcessResult::WriteBack;
            }

            if is_local_v4(ipv4.dst_addr()) {
                return ProcessResult::Consume;
            }

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
            if is_local_v6(ipv6.dst_addr()) {
                return ProcessResult::Consume;
            }

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
    ) -> ProcessResult {
        if should_reinject_local_dns_v4(ipv4.dst_addr(), address_v4) {
            return ProcessResult::WriteBack;
        }

        if is_local_v4(ipv4.dst_addr()) {
            return ProcessResult::Consume;
        }

        self.udp_nat.send(
            SocketAddr::new(IpAddr::V4(ipv4.src_addr()), udp.src_port()),
            SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), udp.dst_port()),
            udp.payload(),
        );

        ProcessResult::Consume
    }

    fn process_udp_v6_packet(&self, ipv6: &mut net_packet::ipv6::Ipv6Header, udp: &mut net_packet::udp::UdpHeader) {
        if is_local_v6(ipv6.dst_addr()) {
            return;
        }

        self.udp_nat.send(
            SocketAddr::new(IpAddr::V6(ipv6.src_addr()), udp.src_port()),
            SocketAddr::new(IpAddr::V6(ipv6.dst_addr()), udp.dst_port()),
            udp.payload(),
        );
    }

    fn process_icmp_v4_packet(
        &self,
        ipv4: &mut net_packet::ipv4::Ipv4Header,
        icmp: &mut net_packet::icmpv4::Icmpv4Header,
    ) {
        if is_local_v4(ipv4.dst_addr()) {
            return;
        }
        // suport only Echo Request
        // other type may cause some unexpected behavior or security issue
        if icmp.icmp_type() != 8 {
            return;
        }

        self.icmp_nat.send(
            SocketAddr::new(IpAddr::V4(ipv4.src_addr()), 0),
            SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), 0),
            icmp.data(),
        );
    }

    fn process_icmp_v6_packet(
        &self,
        ipv6: &mut net_packet::ipv6::Ipv6Header,
        icmp: &mut net_packet::icmpv6::Icmpv6Header,
    ) {
        if is_local_v6(ipv6.dst_addr()) {
            return;
        }
        // suport only Echo Request
        if icmp.icmp_type() != 128 {
            return;
        }

        self.icmp_nat.send(
            SocketAddr::new(IpAddr::V6(ipv6.src_addr()), 0),
            SocketAddr::new(IpAddr::V6(ipv6.dst_addr()), 0),
            icmp.data(),
        );
    }

    fn process_igmp_v4_packet(&self, ipv4: &mut net_packet::ipv4::Ipv4Header, igmp: &mut net_packet::igmp::IgmpHeader) {
        tracing::debug!("IGMPv4 packet: {:?}, to {}", igmp, ipv4.dst_addr());
    }

    fn process_igmp_v6_packet(&self, ipv6: &mut net_packet::ipv6::Ipv6Header, igmp: &mut net_packet::igmp::IgmpHeader) {
        tracing::debug!("IGMPv6 packet: {:?}, to {}", igmp, ipv6.dst_addr());
    }
}

fn is_local_v4(addr: Ipv4Addr) -> bool {
    addr.is_loopback() || addr.is_link_local() || addr.is_broadcast() || addr.is_private() || addr.is_multicast()
}

#[cfg(target_os = "macos")]
fn should_reinject_local_dns_v4(dst_addr: Ipv4Addr, address_v4: Ipv4Addr) -> bool {
    // macOS can emit scoped traffic for the utun interface address onto the
    // utun device instead of delivering it directly to local sockets. A packet
    // read from utun is on the outbound side; writing it back injects it as
    // inbound traffic, allowing the kernel to deliver it to listeners bound to
    // address_v4.
    dst_addr == address_v4
}

#[cfg(not(target_os = "macos"))]
fn should_reinject_local_dns_v4(_: Ipv4Addr, _: Ipv4Addr) -> bool {
    false
}

fn is_local_v6(addr: Ipv6Addr) -> bool {
    addr.is_loopback() || addr.is_unique_local() || addr.is_unicast_link_local() || addr.is_multicast()
}

async fn wait_interface_ready(tun_name: &str) -> Result<()> {
    let mut cur_iter_to_show = 0;
    let instant = std::time::Instant::now();
    loop {
        let net_if = if_addrs::get_if_addrs()?
            .into_iter()
            .find_map(|x| if x.name == tun_name { Some(x) } else { None });
        if let Some(iface) = net_if
            && iface.is_oper_up()
        {
            break;
        }
        if instant.elapsed() > WAIT_IF_READY_TIMEOUT {
            bail!("timout waiting for tun interface");
        }
        if instant.elapsed().as_secs() > cur_iter_to_show {
            cur_iter_to_show += 1;
            tracing::info!("waiting for tun interface to be ready {}s", cur_iter_to_show);
        }
        sleep(WAIT_IF_READY_INTERVAL).await;
    }
    Ok(())
}

fn find_if_address() -> Result<Ipv4Addr> {
    let net_ifs = NetworkInterface::show()?;
    let address_by_index = |i: u8| -> Option<Ipv4Addr> {
        let addr = Ipv4Addr::new(172, i, 0, 1);
        let found = net_ifs.iter().any(|i| {
            i.addr
                .iter()
                .any(|a| matches!(a, network_interface::Addr::V4(ifaddr) if ifaddr.ip == addr))
        });
        (!found).then_some(addr)
    };

    if let Some(ipv4) = address_by_index(23) {
        return Ok(ipv4);
    }

    for i in 24..31 {
        if let Some(ipv4) = address_by_index(i) {
            return Ok(ipv4);
        }
    }
    for i in 16..24 {
        if let Some(ipv4) = address_by_index(i) {
            return Ok(ipv4);
        }
    }

    Err(anyhow!("no available address for tun interface found"))
}
