use std::sync::Arc;

use anyhow::Result;
use net_packet::MAX_PACKET_SIZE;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UdpSocket,
};

pub async fn icmp_transfer(client: impl AsyncRead + AsyncWrite + Unpin, out_socket: UdpSocket) -> Result<()> {
    let (mut reader, mut writer) = tokio::io::split(client);
    let out_socket = Arc::new(out_socket);

    let client_to_icmp = {
        let socket = out_socket.clone();
        async move {
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
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
                socket.send(&buf[..len]).await?;
            }
            Ok(())
        }
    };

    let icmp_to_client = async move {
        let mut buf = vec![0u8; MAX_PACKET_SIZE + 2];
        loop {
            let n = out_socket.recv(&mut buf[2..]).await?;
            buf[0] = ((n >> 8) & 0xff) as u8;
            buf[1] = (n & 0xff) as u8;
            writer.write_all(&buf[..n + 2]).await?;
        }
    };

    tokio::select! {
        result = client_to_icmp => result,
        result = icmp_to_client => result,
    }
}
