use anyhow::Result;
use arc_swap::ArcSwap;
use hickory_resolver::{
    Resolver,
    config::{NameServerConfig, ResolverConfig},
    name_server::TokioConnectionProvider,
};
use hickory_proto::xfer::Protocol as DnsProtocol;
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io,
    net::{TcpSocket, TcpStream, UdpSocket},
    task,
};

use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{self, RootCertStore, client::Tls12Resumption, pki_types},
};

use crate::{streams::upgrade_stream::UpgradeStream};
use sys_net::{find_outbound_ip, get_dns_by_if_addr};

pub enum StreamType {
    TcpStream(TcpStream),
    UpgradeStream(UpgradeStream<TlsStream<TcpStream>>),
}

pub struct Egress {
    outbound_ip: ArcSwap<IpAddr>,
    tls_cfg: Arc<rustls::ClientConfig>,
    resolver: ArcSwap<Resolver<TokioConnectionProvider>>,
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
            outbound_ip: ArcSwap::from_pointee(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            tls_cfg: Arc::new(tls_cfg),
            resolver: ArcSwap::from_pointee(
                Resolver::builder_with_config(ResolverConfig::new(), TokioConnectionProvider::default()).build(),
            ),
        })
    }

    pub async fn init(self: &Arc<Self>) -> Result<()> {
        self.update().await?;

        let self_clone = self.clone();
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        task::spawn_blocking(move || {
            let mut notifier = if_addrs::IfChangeNotifier::new().unwrap();
            loop {
                if let Ok(_) = notifier.wait(None) {
                    let self_clone = self_clone.clone();
                    tokio::spawn(async move {
                        self_clone.update().await.ok();
                    });
                }
            }
        });
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        task::spawn(async move {
            loop {
                // TODO: ??? it should be better way, but if_addrs::IfChangeNotifier Not available on iOS/macOS
                delay(std::time::Duration::from_secs(60)).await;
                self_clone.update().await.ok();
            }
        });

        Ok(())
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

            StreamType::UpgradeStream(UpgradeStream::from_stream(server, host, http_path))
        } else {
            StreamType::TcpStream(server)
        })
    }

    pub async fn connect_tcp(&self, target: SocketAddr) -> io::Result<TcpStream> {
        let outbound_address = SocketAddr::new(**self.outbound_ip.load(), 0);

        // bind socket to outbound IF
        let socket = match outbound_address {
            SocketAddr::V4(_) => TcpSocket::new_v4()?,
            SocketAddr::V6(_) => TcpSocket::new_v6()?,
        };
        socket.bind(outbound_address)?;

        socket.connect(target).await
    }

    pub async fn connect_udp(&self, target: SocketAddr) -> io::Result<UdpSocket> {
        let outbound_address = SocketAddr::new(**self.outbound_ip.load(), 0);
        let socket = UdpSocket::bind(outbound_address).await?;

        socket.connect(target).await?;
        Ok(socket)
    }

    pub async fn connect_icmp(&self, target: SocketAddr) -> io::Result<UdpSocket> {
        let (domain, protocol) = if target.is_ipv4() {
            (Domain::IPV4, Protocol::ICMPV4)
        } else {
            (Domain::IPV6, Protocol::ICMPV6)
        };
        let socket = Socket::new(domain, Type::RAW, Some(protocol))?;
        socket.set_nonblocking(true)?;

        let outbound_address = SocketAddr::new(**self.outbound_ip.load(), 0);
        socket.bind(&outbound_address.into())?;

        let std_udp: std::net::UdpSocket = socket.into();
        let socket = UdpSocket::from_std(std_udp)?;

        socket.connect(target).await?;
        Ok(socket)
    }

    pub async fn lookup_host(&self, host: &str) -> Option<IpAddr> {
        self.resolver.load().lookup_ip(host).await.ok().and_then(|lookup| lookup.iter().next())
    }

    async fn update(self: &Arc<Self>) -> Result<()> {
        let outbound_ip = find_outbound_ip().await?;
        self.outbound_ip.store(Arc::new(outbound_ip));

        let mut config = ResolverConfig::new();

        // try outbound IF dns first
        let if_dns_ips = get_dns_by_if_addr(outbound_ip).await?;
        for dns_ip in if_dns_ips {
            if is_invalid_dns(dns_ip) {
                tracing::warn!("skipping invalid IF DNS server: {}", dns_ip);
                continue;
            }
            
            let mut ns = NameServerConfig::new(SocketAddr::new(dns_ip, 53), DnsProtocol::Udp);
            // set bind_addr to outbound IF for all name servers, so resolver will use correct IF to send dns queries
            ns.bind_addr = Some(SocketAddr::new(outbound_ip, 0));
            config.add_name_server(ns);        
        }

        // fallback to public dns if we can't get dns from IF
        // TODO: ??? move to options, same as in outbound.rs
        let dns_ips = [
            "8.8.8.8".parse().unwrap(), 
            "1.1.1.1".parse().unwrap(),
        ];

        for dns_ip in dns_ips {
            let mut ns = NameServerConfig::new(SocketAddr::new(dns_ip, 53), DnsProtocol::Quic);
            // set bind_addr to outbound IF for all name servers, so resolver will use correct IF to send dns queries
            ns.bind_addr = Some(SocketAddr::new(outbound_ip, 0));
            config.add_name_server(ns);
        }        

        self.resolver.store(Arc::new(
            Resolver::builder_with_config(config, TokioConnectionProvider::default()).build(),
        ));

        Ok(())
    }
}

fn is_invalid_dns(ip: IpAddr) -> bool {
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
