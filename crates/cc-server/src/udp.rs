
use std::sync::Arc;

use anyhow::Result;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::UdpSocket,
};

// TODO: ??? move MAX_PACKET_SIZE to common place (used many times)
pub const MAX_PACKET_SIZE: usize = 0xFFFF; // max IP packet size

pub async fn udp_transfer(client: impl AsyncRead + AsyncWrite + Unpin, out_socket: UdpSocket) -> Result<()> {
    let (mut reader, mut writer) = tokio::io::split(client);
    let out_socket = Arc::new(out_socket);

    let client_to_udp = {
        let socket = out_socket.clone();
        async move {
            let mut buf = vec![0u8; MAX_PACKET_SIZE];
            loop {
                // Read length header — clean EOF on first byte means transfer done
                let n = reader.read(&mut buf[..2]).await?;
                if n == 0 { break; }
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

    let udp_to_client = async move {
        let mut buf = vec![0u8; MAX_PACKET_SIZE + 2];
        loop {
            let n = out_socket.recv(&mut buf[2..]).await?;
            buf[0] = ((n >> 8) & 0xff) as u8;
            buf[1] = (n & 0xff) as u8;
            writer.write_all(&buf[..n + 2]).await?;
        }
    };

    tokio::select! {
        result = client_to_udp => result,
        result = udp_to_client => result,
    }
}
