use std::net::{IpAddr, SocketAddr};

use anyhow::{Context, Result, anyhow};
use if_addrs::get_if_addrs;
use tokio::net::UdpSocket;

pub async fn find_outbound_ip() -> Result<IpAddr> {
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
