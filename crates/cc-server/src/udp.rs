
use anyhow::Result;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, AsyncRead},
    net::UdpSocket,
};

// TODO: ??? move MAX_PACKET_SIZE to common place (used many times)
pub const MAX_PACKET_SIZE: usize = 0xFFFF; // max IP packet size

pub async fn udp_transfer(mut client: impl AsyncWriteExt + AsyncRead + Unpin, out_socket: UdpSocket) -> Result<()> {
    let mut read_packet_len = 0;
    let mut read_buf = vec![0u8; MAX_PACKET_SIZE];
    let mut recv_buf = vec![0u8; MAX_PACKET_SIZE];
    loop {
        tokio::select! {
            result = if read_packet_len == 0 { client.read_exact(&mut read_buf[..2]) } else { client.read_exact(&mut read_buf[..read_packet_len]) } => {
                result?;
                if read_packet_len == 0 {
                    read_packet_len = ((read_buf[0] as usize) << 8) + read_buf[1] as usize;
                    if read_packet_len > MAX_PACKET_SIZE {
                        anyhow::bail!("packet size too big");
                    }
                } else {
                    out_socket.send(&read_buf[..read_packet_len]).await?;
                    read_packet_len = 0;
                }
            }
            result = out_socket.recv(&mut recv_buf[2..]) => {
                let n = result?;
                if n == 0 { break; }
                recv_buf[0] = ((n >> 8) & 0xff) as u8;
                recv_buf[1] = (n & 0xff) as u8;
                client.write_all(&recv_buf[..n + 2]).await?;
            }
        }
    }

    Ok(())
}
