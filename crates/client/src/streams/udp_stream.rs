use parking_lot::Mutex;
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    vec,
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};

pub struct UdpStreamData {
    packets_to_send: Mutex<Vec<Vec<u8>>>,
    waker: Mutex<Option<Waker>>,
    done: Mutex<bool>,
    last_active: Mutex<std::time::Instant>,
}

pub struct UdpStream<W: AsyncWrite + Clone> {
    writer: W,
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
        let len = packet.len();
        packet.insert(0, ((len >> 8) & 0xff) as u8);
        packet.insert(1, (len & 0xff) as u8);
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

impl<W: AsyncWrite + Clone> UdpStream<W> {
    pub fn new(writer: W) -> Self {
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

impl<W: AsyncWrite + Clone + Unpin> AsyncRead for UdpStream<W> {
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

impl<W: AsyncWrite + Clone + Unpin + Send + 'static> AsyncWrite for UdpStream<W> {
    fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
        let this = self.get_mut();
        let data = &mut this.write_data;

        let prev_len = data.len();
        data.extend_from_slice(&buf);
        if data.len() < 2 {
            return Poll::Ready(Ok(buf.len()));
        }

        let packet_len = ((data[0] as usize) << 8) + data[1] as usize;
        if data.len() < packet_len + 2 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{RawWaker, RawWakerVTable};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{sleep, Duration};

    #[derive(Clone)]
    struct MockWriter {
        written: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl MockWriter {
        fn new() -> Self {
            Self {
                written: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl AsyncWrite for MockWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            self.written.lock().push(buf.to_vec());
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    fn noop_waker() -> Waker {
        fn noop(_: *const ()) {}
        fn clone_noop(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone_noop, noop, noop, noop);
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    fn new_stream() -> UdpStream<MockWriter> {
        UdpStream::new(MockWriter::new())
    }

    fn new_stream_with_writer() -> (UdpStream<MockWriter>, Arc<Mutex<Vec<Vec<u8>>>>) {
        let w = MockWriter::new();
        let written = w.written.clone();
        (UdpStream::new(w), written)
    }

    // ════════════════════════════════════════════════
    // 1. send_packet from a separate task after delay
    //    → reader wakes up and receives data
    // ════════════════════════════════════════════════

    #[tokio::test]
    async fn read_wakes_up_after_delayed_send_packet() {
        let mut stream = new_stream();
        let data = stream.data();

        // Spawn a task that sends a packet after a short delay
        tokio::spawn(async move {
            sleep(Duration::from_millis(50)).await;
            data.send_packet(vec![0xAA, 0xBB]);
        });

        // This read will initially pend, then wake up when the packet arrives
        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        // send_packet prepends 2-byte length: [0x00, 0x02, 0xAA, 0xBB]
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], &[0x00, 0x02, 0xAA, 0xBB]);
    }

    // ════════════════════════════════════════════════
    // 2. Multiple send_packets from concurrent tasks
    //    → all packets can be read correctly
    // ════════════════════════════════════════════════

    #[tokio::test]
    async fn read_all_packets_from_concurrent_senders() {
        let mut stream = new_stream();
        let data = stream.data();

        let count = 10;
        for i in 0..count {
            let data = data.clone();
            tokio::spawn(async move {
                sleep(Duration::from_millis(5 * i)).await;
                data.send_packet(vec![i as u8]);
            });
        }

        // Signal done after all senders have had time to complete
        let data_done = stream.data();
        tokio::spawn(async move {
            sleep(Duration::from_millis(200)).await;
            data_done.done();
        });

        let mut received = Vec::new();
        let mut buf = vec![0u8; 64];
        loop {
            let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
            if n == 0 {
                break; // EOF
            }
            // Each 1-byte payload becomes [0x00, 0x01, <byte>]
            assert_eq!(n, 3);
            assert_eq!(buf[0], 0x00);
            assert_eq!(buf[1], 0x01);
            received.push(buf[2]);
        }

        received.sort();
        let expected: Vec<u8> = (0..count as u8).collect();
        assert_eq!(received, expected);
    }

    // ════════════════════════════════════════════════
    // 3. All poll_read execution paths
    // ════════════════════════════════════════════════

    // 3a. No packets, not done → Pending, waker registered
    #[tokio::test]
    async fn poll_read_pending_when_empty_and_not_done() {
        let mut stream = new_stream();
        let mut buf = [0u8; 64];
        let mut read_buf = ReadBuf::new(&mut buf);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let result = Pin::new(&mut stream).poll_read(&mut cx, &mut read_buf);
        assert!(result.is_pending());
        // Waker should be stored
        assert!(stream.data.waker.lock().is_some());
    }

    // 3b. No packets, done → Ready(Ok(())) (EOF, 0 bytes)
    #[tokio::test]
    async fn poll_read_eof_when_done_and_empty() {
        let mut stream = new_stream();
        stream.data.done();

        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    // 3c. Packet available → transitions to Read, returns data
    #[tokio::test]
    async fn poll_read_returns_data_when_packet_available() {
        let mut stream = new_stream();
        stream.data.send_packet(vec![0x01, 0x02, 0x03]);

        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        // 3-byte payload → [0x00, 0x03, 0x01, 0x02, 0x03]
        assert_eq!(n, 5);
        assert_eq!(&buf[..n], &[0x00, 0x03, 0x01, 0x02, 0x03]);
    }

    // 3d. Packet larger than read buffer → partial read, then remainder
    #[tokio::test]
    async fn poll_read_partial_then_remainder() {
        let mut stream = new_stream();
        stream.data.send_packet(vec![0xAA, 0xBB, 0xCC, 0xDD]);
        // 4-byte payload → [0x00, 0x04, 0xAA, 0xBB, 0xCC, 0xDD] — 6 bytes

        // Read with 3-byte buffer
        let mut buf = vec![0u8; 3];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[..n], &[0x00, 0x04, 0xAA]);

        // Continue reading the rest
        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[..n], &[0xBB, 0xCC, 0xDD]);
    }

    // 3e. Multiple packets are consumed sequentially (LIFO from Vec::pop)
    #[tokio::test]
    async fn poll_read_multiple_packets_sequential() {
        let mut stream = new_stream();
        stream.data.send_packet(vec![0x11]);
        stream.data.send_packet(vec![0x22]);

        let mut buf = vec![0u8; 64];

        // pop() returns last pushed first
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(buf[2], 0x22);

        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(buf[2], 0x11);
    }

    // 3f. Packets are drained before signaling EOF
    #[tokio::test]
    async fn poll_read_drains_packets_before_eof() {
        let mut stream = new_stream();
        stream.data.send_packet(vec![0x42]);
        stream.data.done();

        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        // 1-byte payload → [0x00, 0x01, 0x42]
        assert_eq!(n, 3);
        assert_eq!(&buf[..n], &[0x00, 0x01, 0x42]);

        // Now EOF
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    // 3g. Waker is updated when a new context is used
    #[tokio::test]
    async fn poll_read_updates_waker() {
        let mut stream = new_stream();
        let mut buf = [0u8; 64];
        let mut read_buf = ReadBuf::new(&mut buf);

        let waker1 = noop_waker();
        let mut cx1 = Context::from_waker(&waker1);
        let _ = Pin::new(&mut stream).poll_read(&mut cx1, &mut read_buf);
        assert!(stream.data.waker.lock().is_some());

        // Poll again with a different waker — should update
        let waker2 = noop_waker();
        let mut cx2 = Context::from_waker(&waker2);
        let mut read_buf2 = ReadBuf::new(&mut buf);
        let _ = Pin::new(&mut stream).poll_read(&mut cx2, &mut read_buf2);
        assert!(stream.data.waker.lock().is_some());
    }

    // ════════════════════════════════════════════════
    // 4. All poll_write execution paths
    // ════════════════════════════════════════════════

    // 4a. Partial length header (< 2 bytes) → buffers, nothing written
    #[tokio::test]
    async fn poll_write_buffers_partial_header() {
        let (mut stream, written) = new_stream_with_writer();

        Pin::new(&mut stream).write_all(&[0x00]).await.unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());
    }

    // 4b. Header present but payload incomplete → buffers, nothing written
    #[tokio::test]
    async fn poll_write_buffers_partial_payload() {
        let (mut stream, written) = new_stream_with_writer();

        // Length = 4, but only 2 data bytes sent
        Pin::new(&mut stream)
            .write_all(&[0x00, 0x04, 0xAA, 0xBB])
            .await
            .unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());
    }

    // 4c. Complete chunk in one write → packet dispatched to writer
    #[tokio::test]
    async fn poll_write_complete_chunk() {
        let (mut stream, written) = new_stream_with_writer();

        // [len=3][0xAA, 0xBB, 0xCC]
        Pin::new(&mut stream)
            .write_all(&[0x00, 0x03, 0xAA, 0xBB, 0xCC])
            .await
            .unwrap();
        // Let spawned task run
        tokio::task::yield_now().await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], vec![0xAA, 0xBB, 0xCC]);
    }

    // 4d. Chunk assembled across multiple writes
    #[tokio::test]
    async fn poll_write_chunk_across_multiple_writes() {
        let (mut stream, written) = new_stream_with_writer();

        // First write: partial header
        Pin::new(&mut stream).write_all(&[0x00]).await.unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());

        // Second write: rest of header + partial data
        Pin::new(&mut stream)
            .write_all(&[0x03, 0x01, 0x02])
            .await
            .unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());

        // Third write: final data byte
        Pin::new(&mut stream).write_all(&[0x03]).await.unwrap();
        tokio::task::yield_now().await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], vec![0x01, 0x02, 0x03]);
    }

    // 4e. Empty payload chunk (len=0) — dispatches empty write
    #[tokio::test]
    async fn poll_write_zero_length_chunk() {
        let (mut stream, written) = new_stream_with_writer();

        // [0x00, 0x00] means len=0, chunk_len=2, packet=data[2..2]=empty
        // write_all on empty slice is a no-op, so nothing appears in mock writer
        Pin::new(&mut stream).write_all(&[0x00, 0x00]).await.unwrap();
        tokio::task::yield_now().await;

        let packets = written.lock();
        assert!(packets.is_empty());
    }

    // 4f. Large packet (> 256 bytes)
    #[tokio::test]
    async fn poll_write_large_packet() {
        let (mut stream, written) = new_stream_with_writer();

        let payload = vec![0xAB; 500];
        let mut chunk = Vec::new();
        chunk.push(((payload.len() >> 8) & 0xff) as u8); // 0x01
        chunk.push((payload.len() & 0xff) as u8); // 0xF4
        chunk.extend_from_slice(&payload);

        Pin::new(&mut stream).write_all(&chunk).await.unwrap();
        tokio::task::yield_now().await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0], payload);
    }

    // ════════════════════════════════════════════════
    // UdpStreamData unit tests
    // ════════════════════════════════════════════════

    #[test]
    fn send_packet_prepends_big_endian_length() {
        let data = UdpStreamData::new();
        data.send_packet(vec![0xAA, 0xBB, 0xCC]);
        let packets = data.packets_to_send.lock();
        // 3-byte payload → length header [0x00, 0x03]
        assert_eq!(packets[0], vec![0x00, 0x03, 0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn send_packet_large_length() {
        let data = UdpStreamData::new();
        let payload = vec![0x42; 300];
        data.send_packet(payload.clone());
        let packets = data.packets_to_send.lock();
        // 300 = 0x012C
        assert_eq!(packets[0][0], 0x01);
        assert_eq!(packets[0][1], 0x2C);
        assert_eq!(&packets[0][2..], &payload[..]);
    }

    #[test]
    fn send_packet_empty_payload() {
        let data = UdpStreamData::new();
        data.send_packet(vec![]);
        let packets = data.packets_to_send.lock();
        // empty payload → length header [0x00, 0x00]
        assert_eq!(packets[0], vec![0x00, 0x00]);
    }

    #[test]
    fn done_sets_flag() {
        let data = UdpStreamData::new();
        assert!(!*data.done.lock());
        data.done();
        assert!(*data.done.lock());
    }

    #[test]
    fn last_active_advances_on_send() {
        let data = UdpStreamData::new();
        let t1 = data.last_active();
        std::thread::sleep(std::time::Duration::from_millis(10));
        data.send_packet(vec![1]);
        assert!(data.last_active() > t1);
    }

    #[test]
    fn send_packet_no_waker_no_panic() {
        let data = UdpStreamData::new();
        data.send_packet(vec![1]);
    }
}
