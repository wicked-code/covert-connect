use anyhow::Result;
use arc_swap::ArcSwap;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io,
    net::{TcpSocket, TcpStream, UdpSocket},
    task,
};
use socket2::{Domain, Protocol, Socket, Type};

use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{self, RootCertStore, client::Tls12Resumption, pki_types},
};

use crate::{outbound::find_outbound_ip, streams::upgrade_stream::UpgradeStream};

pub enum StreamType {
    TcpStream(TcpStream),
    UpgradeStream(UpgradeStream<TlsStream<TcpStream>>),
}

pub struct EgressConnector {
    outbound_ip: ArcSwap<IpAddr>,
    tls_cfg: Arc<rustls::ClientConfig>,
}

impl EgressConnector {
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

    async fn update(self: &Arc<Self>) -> Result<()> {
        let outbound_ip = find_outbound_ip().await?;
        self.outbound_ip.store(Arc::new(outbound_ip));
        Ok(())
    }
}
