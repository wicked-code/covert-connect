
use std::{net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr}, sync::Arc};

use anyhow::{Result, bail};
use net_packet::MAX_PACKET_SIZE;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UdpSocket,
};

pub async fn udp_transfer(client: impl AsyncRead + AsyncWrite + Unpin, out_socket: UdpSocket) -> Result<()> {
    let (mut reader, mut writer) = tokio::io::split(client);
    let out_socket = Arc::new(out_socket);

    let client_to_udp = {
        let socket = out_socket.clone();
        async move {
            let mut buf = vec![0u8; MAX_PACKET_SIZE + 21];
            loop {
                // Read length header — clean EOF on first byte means transfer done
                let n = reader.read(&mut buf[..2]).await?;
                if n == 0 { break; }
                if n < 2 {
                    reader.read_exact(&mut buf[1..2]).await?;
                }
                let len = ((buf[0] as usize) << 8) + buf[1] as usize;
                reader.read_exact(&mut buf[..len]).await?;
                let (addr, payload) = match address_from_buf(&mut buf[..len]) {
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
        let mut buf = vec![0u8; MAX_PACKET_SIZE + 21];
        loop {
            let (n, addr) = out_socket.recv_from(&mut buf[21..]).await?;
            match addr {
                SocketAddr::V4(addr) => {
                    let len = n + 7;
                    let wr_buff = &mut buf[12..n + 21];
                    wr_buff[0] = ((len >> 8) & 0xff) as u8;
                    wr_buff[1] = (len & 0xff) as u8;
                    wr_buff[2] = 4; // IPv4 flag
                    wr_buff[3] = (addr.port() >> 8) as u8;
                    wr_buff[4] = addr.port() as u8;
                    wr_buff[5..9].copy_from_slice(&addr.ip().octets());
                    writer.write_all(wr_buff).await?;
                }
                SocketAddr::V6(addr) => {
                    let len = n + 19;
                    buf[0] = ((len >> 8) & 0xff) as u8;
                    buf[1] = (len & 0xff) as u8;
                    buf[2] = 6; // IPv6 flag
                    buf[3] = (addr.port() >> 8) as u8;
                    buf[4] = addr.port() as u8;
                    buf[5..21].copy_from_slice(&addr.ip().octets());
                    writer.write_all(&buf[..len + 2]).await?;
                }
            }
        }
    };

    tokio::select! {
        result = client_to_udp => result,
        result = udp_to_client => result,
    }
}

pub fn address_from_buf(buf: &mut [u8]) -> Result<(SocketAddr, &mut [u8])> {
    if buf.is_empty() {
        bail!("invalid packet length");
    }
    let addr_type = buf[0];
    let buf = &mut buf[1..];
    match addr_type {
        4 => {
            if buf.len() < 6 {
                bail!("invalid IPv4 packet length: {}", buf.len());
            }
            Ok((SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(buf[2], buf[3], buf[4], buf[5])),
                ((buf[0] as u16) << 8) | (buf[1] as u16),
            ),
            &mut buf[6..] // skip 4-byte dest address + 2-byte dest port
            ))
        }, // IPv4
        6 => {
            if buf.len() < 18 {
                bail!("invalid IPv6 packet length: {}", buf.len());
            }
                    let mut addr = [0u8; 16];
                    addr.copy_from_slice(&buf[2..18]);
                    Ok((SocketAddr::new(
                        IpAddr::V6(Ipv6Addr::from(addr)),
                        ((buf[0] as u16) << 8) | (buf[1] as u16),
                    ),
                    &mut buf[18..] // skip 16-byte dest address + 2-byte dest port
                ))
        }, // IPv6
        _ => bail!("invalid address type: {}", addr_type),
    }
}
