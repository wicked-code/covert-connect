use anyhow::Result;
use arc_swap::ArcSwap;
use hickory_proto::xfer::Protocol as DnsProtocol;
use hickory_resolver::{
    Resolver,
    config::{NameServerConfig, ResolverConfig},
    name_server::TokioConnectionProvider,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io,
    net::{TcpSocket, TcpStream, UdpSocket},
};

use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{self, RootCertStore, client::Tls12Resumption, pki_types},
};

const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

use crate::{streams::upgrade_stream::UpgradeStream, utils::cancellable_task::CancellableTask};
use sys_net::{NoInterfaceFoundError, find_default_if};

pub enum StreamType {
    TcpStream(TcpStream),
    UpgradeStream(Box<UpgradeStream<TlsStream<TcpStream>>>),
}

pub struct Egress {
    outbound_ipv4: ArcSwap<SocketAddr>,
    outbound_ipv6: ArcSwap<SocketAddr>,
    #[cfg(target_os = "linux")]
    outbound_if_name: ArcSwap<String>,
    #[cfg(target_os = "macos")]
    outbound_if_index: ArcSwap<u32>,
    outbound_dns: ArcSwap<Vec<IpAddr>>,
    tls_cfg: Arc<rustls::ClientConfig>,
    resolver: ArcSwap<Resolver<TokioConnectionProvider>>,
    update_task: CancellableTask,
}

impl Egress {
    pub fn new() -> Arc<Self> {
        let root_store = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.into(),
        };
        let mut tls_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        // it's ok to set it true, because we have internal replay protection
        // it's default to 10 sec thus replay may result in outbound connection only if sent in less than 10 sec
        // moreover if outbound ip is different there is no practical usage of such replay
        tls_cfg.enable_early_data = true;
        tls_cfg.resumption = tls_cfg.resumption.tls12_resumption(Tls12Resumption::SessionIdOnly);
        Arc::new(Self {
            outbound_ipv4: ArcSwap::from_pointee(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
            outbound_ipv6: ArcSwap::from_pointee(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0)),
            #[cfg(target_os = "linux")]
            outbound_if_name: ArcSwap::from_pointee(String::new()),
            #[cfg(target_os = "macos")]
            outbound_if_index: ArcSwap::from_pointee(0),
            outbound_dns: ArcSwap::from_pointee(Vec::new()),
            tls_cfg: Arc::new(tls_cfg),
            resolver: ArcSwap::from_pointee(
                Resolver::builder_with_config(ResolverConfig::new(), TokioConnectionProvider::default()).build(),
            ),
            update_task: CancellableTask::new("EgressUpdateTask"),
        })
    }

    pub async fn init(self: &Arc<Self>) -> Result<()> {
        if let Err(err) = self.update().await {
            if !err.is::<NoInterfaceFoundError>() {
                return Err(err);
            }
            // it's ok if no outbound at start, egress will keep trying to find a suitable interface in background
            tracing::warn!("No suitable default interface found");
        }

        let self_clone = self.clone();
        self.update_task.spawn(|token| async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    _ = tokio::time::sleep(UPDATE_INTERVAL) => {}
                }
                if let Err(err) = self_clone.update().await {
                    tracing::error!("Failed to update egress: {:?}", err);
                }
            }
        });

        Ok(())
    }

    pub async fn shutdown(&self) {
        self.update_task.stop().await;
    }

    pub async fn connect_with_upgrade(
        &self,
        address: SocketAddr,
        host: &str,
        url_path: &Option<String>,
    ) -> Result<StreamType> {
        let server = self.connect_tcp(address).await?;
        Ok(if let Some(http_path) = url_path {
            // HTTPS connect
            let host = if let Some(pos) = host.rfind(':') {
                &host[..pos]
            } else {
                host
            };

            let domain = pki_types::ServerName::try_from(host)?.to_owned();
            let tls_conn = TlsConnector::from(self.tls_cfg.clone());
            let server = tls_conn.connect(domain, server).await?;

            StreamType::UpgradeStream(Box::new(UpgradeStream::from_stream(server, host, http_path)))
        } else {
            StreamType::TcpStream(server)
        })
    }

    pub async fn connect_tcp(&self, target: SocketAddr) -> io::Result<TcpStream> {
        if target.is_ipv4() {
            let socket = TcpSocket::new_v4()?;
            // bind socket to outbound IF
            socket.bind(**self.outbound_ipv4.load())?;
            #[cfg(target_os = "linux")]
            {
                let if_name = self.outbound_if_name.load();
                bind_socket_to_interface(&socket, if_name.as_str())?;
            }
            #[cfg(target_os = "macos")]
            bind_socket_to_interface(&socket, **self.outbound_if_index.load(), false)?;
            let stream = socket.connect(target).await?;
            if let Err(err) = stream.set_nodelay(true) {
                tracing::warn!("failed to set TCP_NODELAY on outbound IPv4 stream: {:?}", err);
            }
            Ok(stream)
        } else {
            let socket = TcpSocket::new_v6()?;
            // bind socket to outbound IF
            socket.bind(**self.outbound_ipv6.load())?;
            #[cfg(target_os = "linux")]
            {
                let if_name = self.outbound_if_name.load();
                bind_socket_to_interface(&socket, if_name.as_str())?;
            }
            #[cfg(target_os = "macos")]
            bind_socket_to_interface(&socket, **self.outbound_if_index.load(), true)?;
            let stream = socket.connect(target).await?;
            if let Err(err) = stream.set_nodelay(true) {
                tracing::warn!("failed to set TCP_NODELAY on outbound IPv6 stream: {:?}", err);
            }
            Ok(stream)
        }
    }

    pub async fn bind_udp(&self, is_ipv6: bool) -> io::Result<UdpSocket> {
        let outbound_address = if is_ipv6 {
            **self.outbound_ipv6.load()
        } else {
            **self.outbound_ipv4.load()
        };

        let socket = UdpSocket::bind(outbound_address).await?;
        #[cfg(target_os = "linux")]
        {
            let if_name = self.outbound_if_name.load();
            bind_socket_to_interface(&socket, if_name.as_str())?;
        }
        #[cfg(target_os = "macos")]
        bind_socket_to_interface(&socket, **self.outbound_if_index.load(), is_ipv6)?;
        Ok(socket)
    }

    pub async fn connect_icmp(&self, target: SocketAddr) -> io::Result<UdpSocket> {
        let (domain, protocol, outbound_address) = if target.is_ipv4() {
            (Domain::IPV4, Protocol::ICMPV4, **self.outbound_ipv4.load())
        } else {
            (Domain::IPV6, Protocol::ICMPV6, **self.outbound_ipv6.load())
        };
        let socket = Socket::new(domain, Type::RAW, Some(protocol))?;
        socket.set_nonblocking(true)?;
        #[cfg(target_os = "linux")]
        {
            let if_name = self.outbound_if_name.load();
            bind_socket_to_interface(&socket, if_name.as_str())?;
        }
        #[cfg(target_os = "macos")]
        bind_socket_to_interface(&socket, **self.outbound_if_index.load(), target.is_ipv6())?;

        socket.bind(&outbound_address.into())?;

        let std_udp: std::net::UdpSocket = socket.into();
        let socket = UdpSocket::from_std(std_udp)?;

        socket.connect(target).await?;
        Ok(socket)
    }

    pub async fn lookup_host(&self, host: &str) -> Result<Option<IpAddr>> {
        let res = self.resolver.load().lookup_ip(host).await?;
        // TODO: ??? move to options or check if IPv6 enabled somehow
        // prefer ipv4
        let ip = res
            .iter()
            .reduce(|acc, val| if acc.is_ipv6() && val.is_ipv4() { val } else { acc });
        Ok(ip)
    }

    async fn update(self: &Arc<Self>) -> Result<()> {
        let net_if = find_default_if()?;
        #[cfg(target_os = "linux")]
        let current_if_name = self.outbound_if_name.load();
        #[cfg(target_os = "macos")]
        let current_if_index = **self.outbound_if_index.load();
        let net_if_changed =
            net_if.ipv4 != self.outbound_ipv4.load().ip() || net_if.ipv6 != self.outbound_ipv6.load().ip() || {
                #[cfg(target_os = "linux")]
                {
                    net_if.if_name != current_if_name.as_str()
                }
                #[cfg(target_os = "macos")]
                {
                    net_if.if_index != current_if_index
                }
                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                {
                    false
                }
            };

        if net_if_changed {
            self.outbound_ipv4.store(Arc::new(SocketAddr::new(net_if.ipv4, 0)));
            self.outbound_ipv6.store(Arc::new(SocketAddr::new(net_if.ipv6, 0)));
            #[cfg(target_os = "linux")]
            self.outbound_if_name.store(Arc::new(net_if.if_name.clone()));
            #[cfg(target_os = "macos")]
            self.outbound_if_index.store(Arc::new(net_if.if_index));
        } else {
            let prev_dns = self.outbound_dns.load().clone();
            if prev_dns.len() == net_if.dns.len() && prev_dns.iter().all(|ip| net_if.dns.contains(ip)) {
                return Ok(());
            }
        }

        self.outbound_dns.store(Arc::new(net_if.dns.clone()));
        self.update_resolver(net_if.dns, net_if.ipv4, net_if.ipv6).await?;

        Ok(())
    }

    async fn update_resolver(self: &Arc<Self>, dns_list: Vec<IpAddr>, if_ipv4: IpAddr, if_ipv6: IpAddr) -> Result<()> {
        let mut config = ResolverConfig::new();

        for dns_ip in dns_list {
            if is_invalid_address(dns_ip) {
                tracing::warn!("skipping invalid IF DNS server: {}", dns_ip);
                continue;
            }

            let mut ns = NameServerConfig::new(SocketAddr::new(dns_ip, 53), DnsProtocol::Udp);
            // set bind_addr to outbound IF for all name servers, so resolver will use correct IF to send dns queries
            ns.bind_addr = Some(SocketAddr::new(if dns_ip.is_ipv4() { if_ipv4 } else { if_ipv6 }, 0));
            config.add_name_server(ns);
        }

        // fallback to public dns if we can't get dns from IF
        // TODO: ??? move to options, same as in outbound.rs
        let dns_ips = [
            ("8.8.8.8:443".parse().unwrap(), "dns.google"),
            ("1.1.1.1:443".parse().unwrap(), "cloudflare-dns.com"),
        ];

        for dns_ip in dns_ips {
            let mut ns = NameServerConfig::new(dns_ip.0, DnsProtocol::Https);
            // set bind_addr to outbound IF for all name servers, so resolver will use correct IF to send dns queries
            ns.bind_addr = Some(SocketAddr::new(if dns_ip.0.is_ipv4() { if_ipv4 } else { if_ipv6 }, 0));
            ns.tls_dns_name = Some(dns_ip.1.to_string());
            config.add_name_server(ns);
        }

        self.resolver.store(Arc::new(
            Resolver::builder_with_config(config, TokioConnectionProvider::default()).build(),
        ));

        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn bind_socket_to_interface<S>(socket: &S, if_name: &str) -> io::Result<()>
where
    S: std::os::fd::AsFd,
{
    if if_name.is_empty() {
        return Ok(());
    }

    socket2::SockRef::from(socket).bind_device(Some(if_name.as_bytes()))
}

#[cfg(target_os = "macos")]
fn bind_socket_to_interface<S>(socket: &S, if_index: u32, is_ipv6: bool) -> io::Result<()>
where
    S: std::os::fd::AsFd,
{
    let Some(if_index) = std::num::NonZeroU32::new(if_index) else {
        return Ok(());
    };

    let socket = socket2::SockRef::from(socket);
    if is_ipv6 {
        socket.bind_device_by_index_v6(Some(if_index))
    } else {
        socket.bind_device_by_index_v4(Some(if_index))
    }
}

fn is_invalid_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // 198.18.0.0/15 (198.18.0.0 – 198.19.255.255)
            (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
            // 100.64.0.0/10 (100.64.0.0 – 100.127.255.255)
            || (octets[0] == 100 && (octets[1] & 0xC0) == 64)
        }
        _ => false,
    }
}
