use anyhow::{Result, anyhow, bail};
use cc_server::udp::address_from_buf;
use net_packet::{
    ip::{IpHeader, IpPacket, NextHeader},
    ip_protocols,
};
use parking_lot::Mutex;
use std::{
    io,
    net::{IpAddr, SocketAddr},
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
    src_addr: SocketAddr,
    dst_addr: SocketAddr,
    data: Arc<UdpStreamData>,
    read_state: ReadState,
    write_data: vec::Vec<u8>,
    is_icmp: bool,
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

    pub fn send_packet(&self, mut packet: Vec<u8>, dst_addr: SocketAddr, is_icmp: bool) {
        // insert packet length in big-endian format
        let address_len = if is_icmp {
            0
        } else {
            if dst_addr.is_ipv4() { 7 } else { 19 }
        };
        let len = packet.len() + address_len;
        let mut prefix = Vec::with_capacity(address_len + 2);
        prefix.push(((len >> 8) & 0xff) as u8);
        prefix.push((len & 0xff) as u8);
        if !is_icmp {
            match dst_addr {
                SocketAddr::V4(addr) => {
                    prefix.push(4); // IPv4 flag
                    prefix.push((addr.port() >> 8) as u8);
                    prefix.push(addr.port() as u8);
                    prefix.extend_from_slice(&addr.ip().octets());
                }
                SocketAddr::V6(addr) => {
                    prefix.push(6); // IPv6 flag
                    prefix.push((addr.port() >> 8) as u8);
                    prefix.push(addr.port() as u8);
                    prefix.extend_from_slice(&addr.ip().octets());
                }
            }
        }
        packet.splice(0..0, prefix);
        *self.last_active.lock() = std::time::Instant::now();
        self.packets_to_send.lock().push(packet);
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
    pub fn new(writer: W, src_addr: SocketAddr, dst_addr: SocketAddr, is_icmp: bool) -> Self {
        Self {
            writer,
            src_addr,
            dst_addr,
            data: Arc::new(UdpStreamData::new()),
            read_state: ReadState::Wait,
            write_data: vec::Vec::new(),
            is_icmp,
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
                if *this.data.done.lock() {
                    return Poll::Ready(Ok(()));
                }

                let mut waker = this.data.waker.lock();
                if !waker.as_ref().is_some_and(|w| w.will_wake(cx.waker())) {
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

        data.extend_from_slice(buf);
        if data.len() < 2 {
            return Poll::Ready(Ok(buf.len()));
        }

        let packet_len = ((data[0] as usize) << 8) + data[1] as usize;
        let chunk_len = packet_len + 2;
        if data.len() < chunk_len {
            return Poll::Ready(Ok(buf.len()));
        }

        let packet = &mut data[2..chunk_len];
        if this.is_icmp {
            // in case of ICMP payload is full L3 packet, because of raw socket
            let mut packet = packet.to_vec();
            match IpPacket::try_from(&mut packet) {
                Ok(ip) => {
                    // swap src and dst, because it's NAT, and packet is from dst to src
                    match correct_icmp_packet(ip, this.dst_addr, this.src_addr) {
                        Ok(_) => Self::send_packet(this.writer.clone(), packet),
                        Err(e) => tracing::warn!("Failed to correct ICMP packet: {:?}", e),
                    }
                }
                Err(e) => tracing::warn!("Failed to parse ICMP packet: {:?}", e),
            }
        } else {
            let (dst_addr, packet) = match address_from_buf(packet) {
                Ok(result) => result,
                Err(err) => {
                    tracing::error!("Failed to parse address from packet: {:?}", err);
                    data.drain(..chunk_len);
                    return Poll::Ready(Ok(buf.len()));
                }
            };

            // swap src and dst, because it's NAT, and packet is from dst to src
            match IpPacket::build(ip_protocols::UDP, dst_addr, this.src_addr, packet) {
                Ok(packet) => Self::send_packet(this.writer.clone(), packet),
                Err(e) => tracing::warn!("Failed to build IP packet: {:?}", e),
            }
        }
        data.drain(..chunk_len);

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}

impl<W: AsyncWrite + Clone + Unpin + Send + 'static> UdpStream<W> {
    fn send_packet(writer: W, packet: Vec<u8>) {
        tokio::task::spawn({
            let mut writer = writer.clone();
            // dst, src because it's NAT, and packet is from dst to src
            async move {
                if let Err(e) = writer.write_all(&packet).await {
                    tracing::warn!("udp stream write error: {:?}", e);
                }
            }
        });
    }
}

fn correct_icmp_packet(ip: IpPacket, src_addr: SocketAddr, dst_addr: SocketAddr) -> Result<()> {
    match ip.header {
        IpHeader::V4(mut header) => {
            let IpAddr::V4(src_addr) = src_addr.ip() else {
                bail!("Correct icmp: src_addr is not IPv4");
            };
            let IpAddr::V4(dst_addr) = dst_addr.ip() else {
                bail!("Correct icmp: dst_addr is not IPv4");
            };
            header.set_src_addr(src_addr);
            header.set_dst_addr(dst_addr);
            header.compute_checksum();

            let message_len = (header.total_length() as usize)
                .checked_sub(header.header_len())
                .ok_or_else(|| anyhow!("Malformed IPv4: total_length < header_len"))?;
            match ip.next_header {
                NextHeader::Icmpv4(mut icmp_v4) => {
                    icmp_v4.compute_checksum(message_len);
                }
                _ => bail!("Unsupported protocol in ICMP packet"),
            }
        }
        IpHeader::V6(mut header) => {
            let IpAddr::V6(src_addr) = src_addr.ip() else {
                bail!("Correct icmp: src_addr is not IPv6");
            };
            let IpAddr::V6(dst_addr) = dst_addr.ip() else {
                bail!("Correct icmp: dst_addr is not IPv6");
            };
            header.set_src_addr(src_addr);
            header.set_dst_addr(dst_addr);

            let message_len = header.payload_length() as usize;
            match ip.next_header {
                NextHeader::Icmpv6(mut icmp_v6) => {
                    icmp_v6.compute_checksum(src_addr, dst_addr, message_len);
                }
                _ => bail!("Unsupported protocol in ICMP packet"),
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use net_packet::ipv4::IPV4_MIN_HEADER_LEN;
    use net_packet::udp::UDP_HEADER_LEN;
    use std::task::{RawWaker, RawWakerVTable};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, sleep};

    const IP_UDP_OVERHEAD: usize = IPV4_MIN_HEADER_LEN + UDP_HEADER_LEN; // 28

    /// Extract the UDP payload from a raw IP packet written by poll_write.
    fn extract_udp_payload(packet: &[u8]) -> &[u8] {
        &packet[IP_UDP_OVERHEAD..]
    }

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
        fn poll_write(self: Pin<&mut Self>, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize, io::Error>> {
            self.written.lock().push(buf.to_vec());
            Poll::Ready(Ok(buf.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
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

    fn test_addr() -> SocketAddr {
        "127.0.0.1:1234".parse().unwrap()
    }

    fn test_addr2() -> SocketAddr {
        "127.0.0.2:1235".parse().unwrap()
    }

    fn new_stream() -> UdpStream<MockWriter> {
        UdpStream::new(MockWriter::new(), test_addr(), test_addr2(), false)
    }

    fn new_stream_with_writer() -> (UdpStream<MockWriter>, Arc<Mutex<Vec<Vec<u8>>>>) {
        let w = MockWriter::new();
        let written = w.written.clone();
        (UdpStream::new(w, test_addr(), test_addr2(), false), written)
    }

    async fn wait_for_written_count(written: &Arc<Mutex<Vec<Vec<u8>>>>, expected: usize) {
        for _ in 0..50 {
            if written.lock().len() >= expected {
                return;
            }
            sleep(Duration::from_millis(5)).await;
        }
    }

    fn build_udp_stream_chunk(payload: &[u8], dst_addr: SocketAddr) -> Vec<u8> {
        let mut body = Vec::new();
        match dst_addr {
            SocketAddr::V4(addr) => {
                body.push(4);
                body.push((addr.port() >> 8) as u8);
                body.push(addr.port() as u8);
                body.extend_from_slice(&addr.ip().octets());
            }
            SocketAddr::V6(addr) => {
                body.push(6);
                body.push((addr.port() >> 8) as u8);
                body.push(addr.port() as u8);
                body.extend_from_slice(&addr.ip().octets());
            }
        }
        body.extend_from_slice(payload);

        let len = body.len();
        let mut chunk = Vec::with_capacity(len + 2);
        chunk.push(((len >> 8) & 0xff) as u8);
        chunk.push((len & 0xff) as u8);
        chunk.extend_from_slice(&body);
        chunk
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
            data.send_packet(vec![0xAA, 0xBB], test_addr(), false);
        });

        // This read will initially pend, then wake up when the packet arrives
        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        // send_packet prepends [len_hi, len_lo, addr_type, port_hi, port_lo, ip...]
        assert_eq!(n, 11);
        assert_eq!(
            &buf[..n],
            &[0x00, 0x09, 0x04, 0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01, 0xAA, 0xBB]
        );
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
                data.send_packet(vec![i as u8], test_addr(), false);
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
            // Each 1-byte payload becomes [header..., <byte>]
            assert_eq!(n, 10);
            received.push(buf[9]);
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
        stream.data.send_packet(vec![0x01, 0x02, 0x03], test_addr(), false);

        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        // 3-byte payload + 9-byte metadata prefix
        assert_eq!(n, 12);
        assert_eq!(
            &buf[..n],
            &[0x00, 0x0A, 0x04, 0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01, 0x01, 0x02, 0x03]
        );
    }

    // 3d. Packet larger than read buffer → partial read, then remainder
    #[tokio::test]
    async fn poll_read_partial_then_remainder() {
        let mut stream = new_stream();
        stream
            .data
            .send_packet(vec![0xAA, 0xBB, 0xCC, 0xDD], test_addr(), false);
        // 4-byte payload + 9-byte metadata prefix = 13 bytes

        // Read with 3-byte buffer
        let mut buf = vec![0u8; 3];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buf[..n], &[0x00, 0x0B, 0x04]);

        // Continue reading the rest
        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(&buf[..n], &[0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0xCC, 0xDD]);
    }

    // 3e. Multiple packets are consumed sequentially (LIFO from Vec::pop)
    #[tokio::test]
    async fn poll_read_multiple_packets_sequential() {
        let mut stream = new_stream();
        stream.data.send_packet(vec![0x11], test_addr(), false);
        stream.data.send_packet(vec![0x22], test_addr(), false);

        let mut buf = vec![0u8; 64];

        // pop() returns last pushed first
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buf[9], 0x22);

        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        assert_eq!(n, 10);
        assert_eq!(buf[9], 0x11);
    }

    // 3f. Packets are drained before signaling EOF
    #[tokio::test]
    async fn poll_read_drains_packets_before_eof() {
        let mut stream = new_stream();
        stream.data.send_packet(vec![0x42], test_addr(), false);
        stream.data.done();

        let mut buf = vec![0u8; 64];
        let n = Pin::new(&mut stream).read(&mut buf).await.unwrap();
        // 1-byte payload + 9-byte metadata prefix
        assert_eq!(n, 10);
        assert_eq!(&buf[..n], &[0x00, 0x08, 0x04, 0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01, 0x42]);

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

        let chunk = build_udp_stream_chunk(&[0xAA, 0xBB], test_addr());
        Pin::new(&mut stream).write_all(&chunk[..4]).await.unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());
    }

    // 4c. Complete chunk in one write → packet dispatched to writer
    #[tokio::test]
    async fn poll_write_complete_chunk() {
        let (mut stream, written) = new_stream_with_writer();

        let chunk = build_udp_stream_chunk(&[0xAA, 0xBB, 0xCC], test_addr());
        Pin::new(&mut stream).write_all(&chunk).await.unwrap();
        wait_for_written_count(&written, 1).await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(extract_udp_payload(&packets[0]), &[0xAA, 0xBB, 0xCC]);
    }

    // 4d. Chunk assembled across multiple writes
    #[tokio::test]
    async fn poll_write_chunk_across_multiple_writes() {
        let (mut stream, written) = new_stream_with_writer();
        let chunk = build_udp_stream_chunk(&[0x01, 0x02, 0x03], test_addr());

        // First write: partial header
        Pin::new(&mut stream).write_all(&chunk[..1]).await.unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());

        // Second write: rest of header + partial data
        Pin::new(&mut stream).write_all(&chunk[1..5]).await.unwrap();
        tokio::task::yield_now().await;
        assert!(written.lock().is_empty());

        // Third write: final bytes
        Pin::new(&mut stream).write_all(&chunk[5..]).await.unwrap();
        wait_for_written_count(&written, 1).await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(extract_udp_payload(&packets[0]), &[0x01, 0x02, 0x03]);
    }

    // 4e. Empty payload chunk (len=0) — dispatches IP+UDP packet with no payload
    #[tokio::test]
    async fn poll_write_zero_length_chunk() {
        let (mut stream, written) = new_stream_with_writer();

        let chunk = build_udp_stream_chunk(&[], test_addr());
        Pin::new(&mut stream).write_all(&chunk).await.unwrap();
        wait_for_written_count(&written, 1).await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].len(), IP_UDP_OVERHEAD);
        assert!(extract_udp_payload(&packets[0]).is_empty());
    }

    // 4f. Large packet (> 256 bytes)
    #[tokio::test]
    async fn poll_write_large_packet() {
        let (mut stream, written) = new_stream_with_writer();

        let payload = vec![0xAB; 500];
        let chunk = build_udp_stream_chunk(&payload, test_addr());

        Pin::new(&mut stream).write_all(&chunk).await.unwrap();
        wait_for_written_count(&written, 1).await;

        let packets = written.lock();
        assert_eq!(packets.len(), 1);
        assert_eq!(extract_udp_payload(&packets[0]), &payload[..]);
    }

    // ════════════════════════════════════════════════
    // UdpStreamData unit tests
    // ════════════════════════════════════════════════

    #[test]
    fn send_packet_prepends_big_endian_length() {
        let data = UdpStreamData::new();
        data.send_packet(vec![0xAA, 0xBB, 0xCC], test_addr(), false);
        let packets = data.packets_to_send.lock();
        assert_eq!(
            packets[0],
            vec![0x00, 0x0A, 0x04, 0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01, 0xAA, 0xBB, 0xCC]
        );
    }

    #[test]
    fn send_packet_large_length() {
        let data = UdpStreamData::new();
        let payload = vec![0x42; 300];
        data.send_packet(payload.clone(), test_addr(), false);
        let packets = data.packets_to_send.lock();
        // 300-byte payload with IPv4 prefix: len = 300 + 7 = 307 = 0x0133
        assert_eq!(
            &packets[0][..9],
            &[0x01, 0x33, 0x04, 0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01]
        );
        assert_eq!(&packets[0][9..], &payload[..]);
    }

    #[test]
    fn send_packet_empty_payload() {
        let data = UdpStreamData::new();
        data.send_packet(vec![], test_addr(), false);
        let packets = data.packets_to_send.lock();
        // empty payload with IPv4 prefix: len = 7
        assert_eq!(packets[0], vec![0x00, 0x07, 0x04, 0x04, 0xD2, 0x7F, 0x00, 0x00, 0x01]);
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
        data.send_packet(vec![1], test_addr(), false);
        assert!(data.last_active() > t1);
    }

    #[test]
    fn send_packet_no_waker_no_panic() {
        let data = UdpStreamData::new();
        data.send_packet(vec![1], test_addr(), false);
    }
}
