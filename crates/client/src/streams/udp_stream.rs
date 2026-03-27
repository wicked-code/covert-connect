use parking_lot::Mutex;
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    vec,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tun::DeviceWriter;

pub struct UdpStreamData {
    packets_to_send: Mutex<Vec<Vec<u8>>>,
    waker: Mutex<Option<Waker>>,
    done: Mutex<bool>,
    last_active: Mutex<std::time::Instant>,
}

pub struct UdpStream {
    writer: DeviceWriter,
    data: Arc<UdpStreamData>,
    read_state: ReadState,
    write_data: vec::Vec<u8>,
}

#[derive(PartialEq)]
enum ReadState {
    Read { pos: usize, packet: Vec<u8> },
    Wait,
}

impl UdpStreamData {
    pub fn new() -> Self {
        Self {
            packets_to_send: Mutex::new(Vec::new()),
            waker: Mutex::new(None),
            done: Mutex::new(false),
            last_active: Mutex::new(std::time::Instant::now()),
        }
    }

    pub fn send_packet(&self, mut packet: Vec<u8>) {
        *self.last_active.lock() = std::time::Instant::now();
        let mut packets = self.packets_to_send.lock();
        // insert packet length in big-endian format
        packet.insert(0, ((packet.len() >> 8) & 0xff) as u8);
        packet.insert(1, (packet.len() & 0xff) as u8);
        packets.push(packet);
        drop(packets);
        let waker = self.waker.lock();
        if let Some(waker) = &*waker {
            waker.wake_by_ref();
        }
    }

    pub fn done(&self) {
        *self.done.lock() = true;
        let waker = self.waker.lock();
        if let Some(waker) = &*waker {
            waker.wake_by_ref();
        }
    }

    pub fn last_active(&self) -> std::time::Instant {
        *self.last_active.lock()
    }
}

impl UdpStream {
    pub fn new(writer: DeviceWriter) -> Self {
        Self {
            writer,
            data: Arc::new(UdpStreamData::new()),
            read_state: ReadState::Wait,
            write_data: vec::Vec::new(),
        }
    }

    pub fn data(&self) -> Arc<UdpStreamData> {
        self.data.clone()
    }
}

impl AsyncRead for UdpStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.read_state == ReadState::Wait {
            if let Some(packet) = this.data.packets_to_send.lock().pop() {
                this.read_state = ReadState::Read { pos: 0, packet };
            } else {
                if this.data.done.lock().clone() {
                    return Poll::Ready(Ok(()));
                }

                let mut waker = this.data.waker.lock();
                if !waker.as_ref().map_or(false, |w| w.will_wake(cx.waker())) {
                    *waker = Some(cx.waker().clone());
                }
                return Poll::Pending;
            }
        }

        if let ReadState::Read {
            ref mut pos,
            ref packet,
        } = this.read_state
        {
            let consumed = usize::min(packet.len() - *pos, buf.remaining());
            buf.put_slice(&packet[*pos..*pos + consumed]);
            if *pos + consumed < packet.len() {
                *pos += consumed;
            } else {
                this.read_state = ReadState::Wait;
            }
        }

        Ok(()).into()
    }
}

impl AsyncWrite for UdpStream {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        let this = self.get_mut();
        let data = &mut this.write_data;

        let prev_len = data.len();
        data.extend_from_slice(&buf);
        if data.len() < 2 {
            return Poll::Ready(Ok(buf.len()));
        }

        let packet_len = ((data[0] as usize) << 8) + data[1] as usize;
        if data.len() < packet_len {
            return Poll::Ready(Ok(buf.len()));
        }

        let chunk_len = packet_len + 2;
        tokio::task::spawn({
            let mut writer = this.writer.clone();
            let packet = data[2..chunk_len].to_vec();
            async move {
                if let Err(e) = writer.write_all(&packet).await {
                    tracing::warn!("udp stream write error: {:?}", e);
                }
            }
        });

        Poll::Ready(Ok(chunk_len - prev_len))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}
