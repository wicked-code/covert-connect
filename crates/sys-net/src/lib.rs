#[cfg(target_os = "linux")]
mod linux;
use std::net::Ipv6Addr;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

pub struct DefaultIf {
    pub ipv4: std::net::IpAddr,
    pub ipv6: std::net::IpAddr,
    pub dns: Vec<std::net::IpAddr>,
}

fn is_ipv6_global(addr: Ipv6Addr) -> bool {
    // Exclude addresses that are strictly local or reserved
    !addr.is_loopback() && 
    !addr.is_unspecified() &&
    !addr.is_unicast_link_local() && 
    !addr.is_unique_local() &&
    !addr.is_multicast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
    use std::time::Duration;

    fn build_dns_query() -> Vec<u8> {
        let mut query = vec![0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        for label in ["example", "com"] {
            query.push(label.len() as u8);
            query.extend_from_slice(label.as_bytes());
        }
        query.extend_from_slice(&[0x00, 0x00, 0x01, 0x00, 0x01]);
        query
    }

    #[test]
    fn find_default_if_returns_valid_ipv4_address_and_dns() -> std::io::Result<()> {
        let default_if = find_default_if().expect("find_default_if() should succeed");

        assert!(
            matches!(default_if.ipv4, IpAddr::V4(_)),
            "expected ipv4 address, got {:?}",
            default_if.ipv4
        );
        assert_ne!(
            default_if.ipv4,
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            "ipv4 address must not be unspecified"
        );

        let local4 = SocketAddr::new(default_if.ipv4, 0);
        let socket4 = UdpSocket::bind(local4)?;
        socket4.set_read_timeout(Some(Duration::from_secs(2)))?;
        socket4.set_write_timeout(Some(Duration::from_secs(2)))?;
        socket4.connect(("8.8.8.8", 53))?;
        socket4.send(&[0u8])?;

        assert!(!default_if.dns.is_empty(), "expected at least one dns server address");

        let query = build_dns_query();
        let mut dns_reachable = false;
        let mut last_error = None;

        for dns_addr in &default_if.dns {
            let server_addr = SocketAddr::new(*dns_addr, 53);
            let bind_addr = match dns_addr {
                IpAddr::V4(_) => SocketAddr::new(default_if.ipv4, 0),
                IpAddr::V6(_) => {
                    if default_if.ipv6 == IpAddr::V6(Ipv6Addr::UNSPECIFIED) {
                        continue;
                    }
                    SocketAddr::new(default_if.ipv6, 0)
                }
            };

            let socket = match UdpSocket::bind(bind_addr) {
                Ok(s) => s,
                Err(err) => {
                    last_error = Some(err);
                    continue;
                }
            };

            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            socket.set_write_timeout(Some(Duration::from_secs(2)))?;

            if let Err(err) = socket.send_to(&query, server_addr) {
                last_error = Some(err);
                continue;
            }

            let mut buf = [0u8; 512];
            match socket.recv_from(&mut buf) {
                Ok((n, _)) if n > 0 => {
                    dns_reachable = true;
                    break;
                }
                Ok(_) => {
                    last_error = Some(std::io::Error::new(std::io::ErrorKind::Other, "empty dns response"));
                }
                Err(err) => {
                    last_error = Some(err);
                }
            }
        }

        assert!(
            dns_reachable,
            "dns server must be reachable and return a response: {:?}",
            last_error
        );

        if default_if.ipv6 != IpAddr::V6(Ipv6Addr::UNSPECIFIED) {
            let local6 = SocketAddr::new(default_if.ipv6, 0);
            let socket6 = UdpSocket::bind(local6)?;
            socket6.set_read_timeout(Some(Duration::from_secs(2)))?;
            socket6.set_write_timeout(Some(Duration::from_secs(2)))?;
            socket6.connect(("2001:4860:4860::8888", 53))?;
            socket6.send(&[0u8])?;
        }

        Ok(())
    }
}
