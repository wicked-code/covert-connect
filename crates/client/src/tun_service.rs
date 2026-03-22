use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{
        Arc, Weak,
        atomic::{AtomicU16, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use if_addrs::get_if_addrs;
use tokio::{
    io::{AsyncRead, AsyncWriteExt},
    net::{TcpListener, TcpSocket, UdpSocket},
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
    tpc_proxy_nat: Arc<TcpProxyNat>,
    tcp_proxy_port: AtomicU16,
}

impl TunService {
    pub fn new(router: Weak<Router>) -> Arc<Self> {
        Arc::new(Self {
            router,
            tpc_proxy_nat: Arc::new(TcpProxyNat::new()),
            tcp_proxy_port: AtomicU16::new(0),
        })
    }

    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        let outbound_ip = find_outbound_ip().await?;

        let (if_addr_v4, if_addr_v6) = self.init_tun().await?;
        self.serve_tcp_proxy(outbound_ip, if_addr_v4, if_addr_v6).await
    }

    async fn serve_tcp_proxy(
        self: &Arc<Self>,
        outbound_ip: IpAddr,
        if_addr_v4: Ipv4Addr,
        if_addr_v6: Ipv6Addr,
    ) -> Result<()> {
        // TODO: ??? add v6 listener
        let mut listener = self.bind_tcp_proxy(if_addr_v4).await?;
        loop {
            let result = listener.accept().await;
            match result {
                Ok((stream, client_addr)) => {
                    let self_clone = self.clone();
                    let router = self.router.upgrade().unwrap().clone();
                    tokio::task::spawn(async move {
                        let port = client_addr.port();
                        let Ok(session) = self_clone.tpc_proxy_nat.get_session(port) else {
                            tracing::error!("session not found for port {}", port);
                            return;
                        };

                        let target = session.dst_addr;
                        match router
                            .start_tunnel(stream, target.to_string(), session.src_addr, outbound_ip)
                            .await
                        {
                            Ok(res) => {
                                if let Some(stream) = res {
                                    if let Err(err) = self_clone.direct_connection(stream, target, outbound_ip).await {
                                        tracing::error!("connection error: {:?}", err);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("server io error: {:?}", e);
                            }
                        }

                        self_clone.tpc_proxy_nat.delete_session(port, session.src_addr);
                    });
                }
                Err(error) => {
                    drop(listener);
                    tracing::error!("accept failed: {:?}", error);
                    listener = self.bind_tcp_proxy(if_addr_v4).await?;
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
        // - set additional IPv6 address
        // - setup routing table
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

        // TODO: ??? probaly stop is needed to shutdown all gracefully...
        let self_clone = self.clone();
        tokio::task::spawn(async move {
            let mut framed = dev.into_framed();

            while let Some(packet) = framed.next().await {
                // TODO: spawn a task here
                // embed tun code into project and add clone for device or maybe for DeviceWriter only
                if let Ok(mut packet) = packet {
                    if self_clone.process_packet(&mut packet, address_v4, gateaway_v4).await == ProcessResult::WriteBack
                    {
                        // let mut tmp = packet.clone();
                        // match IpPacket::new(&mut tmp) {
                        //     Ok(ip) => {
                        //         match ip.header {
                        //             IpHeader::V4(ipv4) => match ip.next_header {
                        //                 NextHeader::Tcp(tcp) => tracing::info!(
                        //                     "Write back TCP packet: {}:{} -> {}:{}, protocol: {}",
                        //                     ipv4.src_addr(),
                        //                     tcp.src_port(),
                        //                     ipv4.dst_addr(),
                        //                     tcp.dst_port(),
                        //                     ipv4.protocol()
                        //                 ),
                        //                 _ => {}
                        //             },
                        //             IpHeader::V6(ipv6) => {
                        //                 // tracing::info!("Write back IPv6 packet: {}:{} -> {}:{}, next header: {}, payload len: {}",
                        //                 //     ipv6.src_addr(), ipv6.src_port(), ipv6.dst_addr(), ipv6.dst_port(), ipv6.next_header(), ip.payload.len());
                        //             }
                        //         }
                        //     }
                        //     Err(err) => {
                        //         tracing::error!("Invalid packet: {:?}", err);
                        //         tracing::error!("data: {:?}", tmp);
                        //         continue;
                        //     }
                        // };

                        framed.get_ref().send(&packet).await.ok();
                    }
                } else {
                    tracing::error!("tun read error: {:?}", packet.err());
                }
            }
        });

        // TODO: get IPv6 here
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

        tracing::info!("tun interface: {:?}", net_if);

        // TODO: ??? update router to all
        // looks like all routes goes to new IF already ???

        // let handle = net_route::Handle::new()?;
        // let route = net_route::Route::new("172.67.202.84".parse().unwrap(), 32).with_ifindex(net_if.index);
        // handle.add(&route).await?;
        // let route = net_route::Route::new("104.21.69.7".parse().unwrap(), 32).with_ifindex(net_if.index);
        // handle.add(&route).await?;

        Ok((address_v4, address_v6))
    }

    async fn process_packet(&self, packet: &mut Vec<u8>, address_v4: Ipv4Addr, gateway_v4: Ipv4Addr) -> ProcessResult {
        let ip = match IpPacket::new(packet) {
            Ok(ip) => ip,
            Err(err) => {
                tracing::error!("Invalid packet: {:?}", err);
                tracing::error!("data: {:?}", packet);
                return ProcessResult::Consume;
            }
        };

        match ip.header {
            IpHeader::V4(mut ipv4) => {
                match ip.next_header {
                    NextHeader::Tcp(mut tcp) => {
                        let tcp_proxy_port = self.tcp_proxy_port();
                        if tcp.src_port() == tcp_proxy_port && ipv4.src_addr() == address_v4 {
                            let Ok(session) = self.tpc_proxy_nat.get_session(tcp.dst_port()) else {
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
                            return ProcessResult::WriteBack;
                        } else {
                            let nat_port = self.tpc_proxy_nat.get_port(
                                SocketAddr::new(IpAddr::V4(ipv4.src_addr()), tcp.src_port()),
                                SocketAddr::new(IpAddr::V4(ipv4.dst_addr()), tcp.dst_port()),
                                tcp_proxy_port,
                            );

                            ipv4.set_src_addr(gateway_v4);
                            tcp.set_src_port(nat_port);
                            ipv4.set_dst_addr(address_v4);
                            tcp.set_dst_port(tcp_proxy_port);

                            ipv4.compute_checksum();
                            tcp.compute_checksum_v4(gateway_v4, address_v4);
                            return ProcessResult::WriteBack;
                        }
                    }
                    NextHeader::Udp(mut udp) => {
                        // TODO: ???
                        tracing::debug!("UDP packet: {:?}", udp);
                    }
                    NextHeader::Icmpv4(mut icmp) => {
                        // TODO: ???
                        tracing::debug!("ICMPv4 packet: {:?}", icmp);
                    }
                    _ => {}
                }
            }
            IpHeader::V6(mut ipv6) => {
                match ip.next_header {
                    NextHeader::Tcp(mut tcp) => {
                        // TODO: ???
                        tracing::debug!("TCP packet: {:?}", tcp);
                    }
                    NextHeader::Udp(mut udp) => {
                        // TODO: ???
                        tracing::debug!("UDP packet: {:?}", udp);
                    }
                    NextHeader::Icmpv6(mut icmp) => {
                        // TODO: ???
                        tracing::debug!("ICMPv6 packet: {:?}", icmp);
                    }
                    _ => {}
                }
            }
        }

        ProcessResult::Consume
    }

    async fn direct_connection(
        &self,
        mut client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
        outbound_ip: IpAddr,
    ) -> Result<()> {
        tracing::info!("direct connection to {}", target);

        let outbound_address = SocketAddr::new(outbound_ip, 0);

        // bind socket to outbound IF
        let socket = TcpSocket::new_v4()?;
        socket.bind(outbound_address)?;

        let mut server = socket.connect(target).await?;

        tokio::io::copy_bidirectional(&mut client, &mut server).await?;
        Ok(())
    }

    fn tcp_proxy_port(&self) -> u16 {
        self.tcp_proxy_port.load(Ordering::Relaxed)
    }

    async fn bind_tcp_proxy(self: &Arc<Self>, if_addr_v4: Ipv4Addr) -> Result<TcpListener> {
        let default_address = SocketAddr::new(IpAddr::V4(if_addr_v4), 0);
        tracing::info!("default_address: {:?}", default_address);

        // bind may hang forever on a newly created interface (Windows bug, need check on linux),
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
                    self.tcp_proxy_port.store(address.port(), Ordering::Relaxed);
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

async fn find_outbound_ip() -> Result<IpAddr> {
    // try public dns
    // TODO: ??? move ips to config or allow override via config
    for target in ["8.8.8.8", "1.1.1.1", "208.67.222.222"] {
        if let Ok(ip) = get_outbound_ip(IpAddr::V4(target.parse()?)).await {
            return Ok(ip);
        }
    }

    // fallback to IPv6 if IPv4 fails
    for target in ["2001:4860:4860::8888", "2606:4700:4700::1111", "2620:119:35::35"] {
        if let Ok(ip) = get_outbound_ip(IpAddr::V6(target.parse()?)).await {
            tracing::warn!("found V6");
            return Ok(ip);
        }
    }

    // fallback to first IF which is not a loopback
    get_if_addrs()?
        .into_iter()
        .find_map(|iface| {
            if !iface.is_loopback() && iface.ip().is_ipv4() {
                tracing::warn!("found loopback");
                Some(iface.ip())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("outbound ip not found"))
}

async fn get_outbound_ip(target: IpAddr) -> Result<IpAddr> {
    let bind_addr = if target.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
    let socket = UdpSocket::bind(bind_addr).await?;

    let target = SocketAddr::new(target, 53);
    socket
        .connect(target)
        .await
        .with_context(|| format!("Failed to connect UDP socket to {}", target))?;
    Ok(socket.local_addr()?.ip())
}
