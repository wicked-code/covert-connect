use anyhow::Result;
use arc_swap::ArcSwap;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    task,
    io::{self, AsyncRead, AsyncWriteExt},
    net::{TcpSocket, TcpStream},
};
use tokio_rustls::{
    TlsConnector,
    client::TlsStream,
    rustls::{self, RootCertStore, client::Tls12Resumption, pki_types},
};

use crate::{outbound::find_outbound_ip, streams::upgrade_stream::UgradeStream};

pub enum StreamType {
    TcpStream(TcpStream),
    UgradeStream(UgradeStream<TlsStream<TcpStream>>),
}

pub struct Transport {
    outbound_ip: ArcSwap<IpAddr>,
    tls_cfg: Arc<rustls::ClientConfig>,
}

impl Transport {
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

    pub async fn connect(&self, address: SocketAddr, host: &str, url_path: &Option<String>) -> Result<StreamType> {
        let server = self.connect_internal(address).await?;
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

            StreamType::UgradeStream(UgradeStream::from_stream(server, host, http_path))
        } else {
            StreamType::TcpStream(server)
        })
    }

    pub async fn direct_transfer(
        &self,
        mut client: impl AsyncWriteExt + Unpin + AsyncRead,
        target: SocketAddr,
    ) -> Result<()> {
        tracing::info!("direct connection to {}", target);

        let mut server = self.connect_internal(target).await?;

        tokio::io::copy_bidirectional(&mut client, &mut server).await?;
        Ok(())
    }

    async fn update(self: &Arc<Self>) -> Result<()> {
        let outbound_ip = find_outbound_ip().await?;
        self.outbound_ip.store(Arc::new(outbound_ip));
        Ok(())
    }

    async fn connect_internal(&self, target: SocketAddr) -> io::Result<TcpStream> {
        let outbound_address = SocketAddr::new(**self.outbound_ip.load(), 0);

        // bind socket to outbound IF
        let socket = match outbound_address {
            SocketAddr::V4(_) => TcpSocket::new_v4()?,
            SocketAddr::V6(_) => TcpSocket::new_v6()?,
        };
        socket.bind(outbound_address)?;

        socket.connect(target).await
    }
}
