use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
};

use anyhow::{Result, bail};
use net_packet::MAX_PACKET_SIZE;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UdpSocket,
};

// max domain name length + 1 byte for flag + 2 bytes for port + 2 bytes for total length
pub const MAX_HOST_AND_PORT_LEN: usize = 254 + 1 + 2 + 2;

pub async fn udp_transfer(
    client: impl AsyncRead + AsyncWrite + Unpin,
    out_socket: UdpSocket,
    lookup: impl AsyncFn(&str) -> Result<SocketAddr>,
) -> Result<()> {
    let (mut reader, mut writer) = tokio::io::split(client);
    let out_socket = Arc::new(out_socket);

    let addr_host_map = Arc::new(Mutex::new(FxHashMap::<SocketAddr, String>::default()));

    let client_to_udp =
        {
            let socket = out_socket.clone();
            let addr_host_map = addr_host_map.clone();
            async move {
                let host_addr_map = Mutex::new(FxHashMap::<String, SocketAddr>::default());

                let mut buf = vec![0u8; MAX_PACKET_SIZE + MAX_HOST_AND_PORT_LEN];
                loop {
                    // Read length header — clean EOF on first byte means transfer done
                    let n = reader.read(&mut buf[..2]).await?;
                    if n == 0 {
                        break;
                    }
                    if n < 2 {
                        reader.read_exact(&mut buf[1..2]).await?;
                    }
                    let len = ((buf[0] as usize) << 8) + buf[1] as usize;
                    reader.read_exact(&mut buf[..len]).await?;
                    let (addr, payload) =
                        match address_from_buf_with_lookup(&mut buf[..len], &lookup, &host_addr_map, &addr_host_map)
                            .await
                        {
                            Ok(result) => result,
                            Err(err) => {
                                tracing::error!("{:?}", err);
                                continue;
                            }
                        };
                    socket.send_to(payload, addr).await?;
                }
                Ok(())
            }
        };

    let udp_to_client = async move {
        let mut buf = vec![0u8; MAX_PACKET_SIZE + MAX_HOST_AND_PORT_LEN];
        loop {
            let (n, addr) = out_socket.recv_from(&mut buf[MAX_HOST_AND_PORT_LEN..]).await?;
            let end = MAX_HOST_AND_PORT_LEN + n;
            let host = addr_host_map.lock().get(&addr).cloned();

            fn write_len_flag_port(
                buf: &mut [u8],
                n: usize,
                prefix_len: usize,
                flag: u8,
                port: u16,
                end: usize,
            ) -> &mut [u8] {
                let len = n + prefix_len;
                let offset = MAX_HOST_AND_PORT_LEN - (prefix_len + 2);
                let wr_buff = &mut buf[offset..end];
                wr_buff[0] = ((len >> 8) & 0xff) as u8;
                wr_buff[1] = (len & 0xff) as u8;
                wr_buff[2] = flag;
                wr_buff[3] = (port >> 8) as u8;
                wr_buff[4] = (port & 0xff) as u8;
                wr_buff
            }

            if let Some(host) = host {
                if host.is_empty() {
                    tracing::error!("zero len host in address map for {:?}", addr);
                    continue;
                }

                let host_len = host.len();
                let prefix_len = 3 + host_len; // 1 byte flag, 2 byte port, the rest is host
                let wr_buff = write_len_flag_port(&mut buf, n, prefix_len, (host_len + 1) as u8, addr.port(), end);
                wr_buff[5..5 + host_len].copy_from_slice(host.as_bytes());
                writer.write_all(wr_buff).await?;
                continue;
            }

            match addr {
                SocketAddr::V4(addr) => {
                    let wr_buff = write_len_flag_port(&mut buf, n, 7, 0 /* IPv4 flag */, addr.port(), end);
                    wr_buff[5..9].copy_from_slice(&addr.ip().octets());
                    writer.write_all(wr_buff).await?;
                }
                SocketAddr::V6(addr) => {
                    let wr_buff = write_len_flag_port(&mut buf, n, 19, 1 /* IPv6 flag */, addr.port(), end);
                    wr_buff[5..21].copy_from_slice(&addr.ip().octets());
                    writer.write_all(wr_buff).await?;
                }
            }
        }
    };

    tokio::select! {
        result = client_to_udp => result,
        result = udp_to_client => result,
    }
}

pub async fn address_from_buf_with_lookup<'a>(
    buf: &'a mut [u8],
    lookup: &impl AsyncFn(&str) -> Result<SocketAddr>,
    host_addr_map: &Mutex<FxHashMap<String, SocketAddr>>,
    addr_host_map: &Arc<Mutex<FxHashMap<SocketAddr, String>>>,
) -> Result<(SocketAddr, &'a mut [u8])> {
    let (address, payload) = address_from_buf(buf)?;
    match address {
        AddressOrHost::Address(addr) => Ok((addr, payload)),
        AddressOrHost::HostAndPort(value) => {
            let host_and_port = value.to_string();
            let addr = host_addr_map.lock().get(&host_and_port).copied();
            match addr {
                Some(addr) => Ok((addr, payload)),
                None => {
                    let addr = lookup(&host_and_port).await?;
                    host_addr_map.lock().insert(host_and_port.clone(), addr);
                    addr_host_map.lock().insert(addr, value.host.clone());
                    Ok((addr, payload))
                }
            }
        }
    }
}

pub struct HostAndPort {
    pub host: String,
    pub port: u16,
}

impl fmt::Display for HostAndPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

pub enum AddressOrHost {
    Address(SocketAddr),
    HostAndPort(HostAndPort),
}

impl AddressOrHost {
    fn new(host: String, port: u16) -> Self {
        Self::HostAndPort(HostAndPort { host, port })
    }
}

pub fn address_from_buf(buf: &mut [u8]) -> Result<(AddressOrHost, &mut [u8])> {
    if buf.is_empty() {
        bail!("invalid packet length");
    }

    fn read_port(buf: &[u8]) -> u16 {
        ((buf[0] as u16) << 8) + (buf[1] as u16)
    }

    let addr_type = buf[0];
    let buf = &mut buf[1..];
    match addr_type {
        // IPv4 flag
        0 => {
            if buf.len() < 6 {
                bail!("invalid IPv4 packet length: {}", buf.len());
            }
            Ok((
                AddressOrHost::Address(SocketAddr::new(
                    IpAddr::V4(Ipv4Addr::new(buf[2], buf[3], buf[4], buf[5])),
                    read_port(buf),
                )),
                &mut buf[6..], // skip 4-byte dest address + 2-byte dest port
            ))
        }
        // IPv6 flag
        1 => {
            if buf.len() < 18 {
                bail!("invalid IPv6 packet length: {}", buf.len());
            }
            let mut addr = [0u8; 16];
            addr.copy_from_slice(&buf[2..18]);
            Ok((
                AddressOrHost::Address(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(addr)), read_port(buf))),
                &mut buf[18..], // skip 16-byte dest address + 2-byte dest port
            ))
        }
        // host and port string
        _ => {
            let addr_len = (addr_type - 1) as usize;
            let host_and_port_len = addr_len + 2 /* port */;
            if buf.len() < host_and_port_len {
                bail!("invalid domain packet length: {}", buf.len());
            }
            let port = read_port(buf);
            let domain = std::str::from_utf8(&buf[2..host_and_port_len])?.to_string();
            Ok((AddressOrHost::new(domain, port), &mut buf[host_and_port_len..]))
        }
    }
}
