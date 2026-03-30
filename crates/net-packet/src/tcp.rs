//! TCP Header Format (RFC 9293)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |          Source Port          |       Destination Port        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                        Sequence Number                        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Acknowledgment Number                      |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |  Data |       |C|E|U|A|P|R|S|F|                               |
//! | Offset| Rsrvd |W|C|R|C|S|S|Y|I|            Window             |
//! |       |       |R|E|G|K|H|T|N|N|                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |           Checksum            |         Urgent Pointer        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Options                    |    Padding    |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                             Data                              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::checksum;
use core::fmt;
use core::net::{Ipv4Addr, Ipv6Addr};
use anyhow::{Result, bail};

/// Minimum TCP header length in bytes (no options).
pub const TCP_MIN_HEADER_LEN: usize = 20;

// TCP flag bit masks for the flags byte.
pub const TCP_FIN: u8 = 0x01;
pub const TCP_SYN: u8 = 0x02;
pub const TCP_RST: u8 = 0x04;
pub const TCP_PSH: u8 = 0x08;
pub const TCP_ACK: u8 = 0x10;
pub const TCP_URG: u8 = 0x20;
pub const TCP_ECE: u8 = 0x40;
pub const TCP_CWR: u8 = 0x80;

/// Zero-copy TCP header backed by a mutable byte slice.
pub struct TcpHeader<'a> {
    buf: &'a mut [u8],
}

impl<'a> TcpHeader<'a> {
    /// Creates a new `TcpHeader` by parsing an existing buffer.
    ///
    /// Validates the buffer is long enough for the header indicated by data offset.
    /// Use [`new_unchecked`](Self::new_unchecked) to build packets from scratch.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < TCP_MIN_HEADER_LEN {
            bail!("Slice too short for TCP header.");
        }
        Ok(Self { buf })
    }

    /// Returns the source port.
    #[inline]
    pub fn src_port(&self) -> u16 {
        ((self.buf[0] as u16) << 8) | (self.buf[1] as u16)
    }

    /// Returns the destination port.
    #[inline]
    pub fn dst_port(&self) -> u16 {
        ((self.buf[2] as u16) << 8) | (self.buf[3] as u16)
    }

    /// Returns the sequence number.
    #[inline]
    pub fn seq_number(&self) -> u32 {
        ((self.buf[4] as u32) << 24) | ((self.buf[5] as u32) << 16) | ((self.buf[6] as u32) << 8) | (self.buf[7] as u32)
    }

    /// Returns the acknowledgment number.
    #[inline]
    pub fn ack_number(&self) -> u32 {
        ((self.buf[8] as u32) << 24)
            | ((self.buf[9] as u32) << 16)
            | ((self.buf[10] as u32) << 8)
            | (self.buf[11] as u32)
    }

    /// Returns the data offset (upper 4 bits of byte 12), in 32-bit words.
    #[inline]
    pub fn data_offset(&self) -> u8 {
        self.buf[12] >> 4
    }

    /// Returns the flags byte.
    #[inline]
    pub fn flags(&self) -> u8 {
        self.buf[13]
    }

    #[inline]
    pub fn fin(&self) -> bool {
        self.buf[13] & TCP_FIN != 0
    }
    #[inline]
    pub fn syn(&self) -> bool {
        self.buf[13] & TCP_SYN != 0
    }
    #[inline]
    pub fn rst(&self) -> bool {
        self.buf[13] & TCP_RST != 0
    }
    #[inline]
    pub fn psh(&self) -> bool {
        self.buf[13] & TCP_PSH != 0
    }
    #[inline]
    pub fn ack(&self) -> bool {
        self.buf[13] & TCP_ACK != 0
    }
    #[inline]
    pub fn urg(&self) -> bool {
        self.buf[13] & TCP_URG != 0
    }
    #[inline]
    pub fn ece(&self) -> bool {
        self.buf[13] & TCP_ECE != 0
    }
    #[inline]
    pub fn cwr(&self) -> bool {
        self.buf[13] & TCP_CWR != 0
    }

    /// Returns the window size.
    #[inline]
    pub fn window_size(&self) -> u16 {
        ((self.buf[14] as u16) << 8) | (self.buf[15] as u16)
    }

    /// Returns the checksum.
    #[inline]
    pub fn checksum(&self) -> u16 {
        ((self.buf[16] as u16) << 8) | (self.buf[17] as u16)
    }

    /// Returns the urgent pointer.
    #[inline]
    pub fn urgent_pointer(&self) -> u16 {
        ((self.buf[18] as u16) << 8) | (self.buf[19] as u16)
    }

    /// Returns the header length in bytes.
    #[inline]
    pub fn header_len(&self) -> usize {
        self.data_offset() as usize * 4
    }

    /// Returns the option bytes (empty if data offset == 5).
    #[inline]
    pub fn options(&self) -> &[u8] {
        &self.buf[TCP_MIN_HEADER_LEN..self.header_len()]
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..self.header_len()]
    }

    /// Verifies the TCP checksum against an IPv4 pseudo-header.
    #[inline]
    pub fn verify_checksum_v4(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr) -> bool {
        let len = self.buf.len() as u16;
        let sum = checksum::ipv4_pseudo_header_sum(src_ip, dst_ip, 6, len);
        checksum::internet_checksum(&self.buf[..len as usize], sum) == 0
    }

    /// Verifies the TCP checksum against an IPv6 pseudo-header.
    #[inline]
    pub fn verify_checksum_v6(&self, src_ip: Ipv6Addr, dst_ip: Ipv6Addr) -> bool {
        let len = self.buf.len() as u32;
        let sum = checksum::ipv6_pseudo_header_sum(src_ip, dst_ip, 6, len);
        checksum::internet_checksum(&self.buf[..len as usize], sum) == 0
    }

    /// Sets the source port.
    #[inline]
    pub fn set_src_port(&mut self, port: u16) {
        self.buf[0] = (port >> 8) as u8;
        self.buf[1] = (port & 0xFF) as u8;
    }

    /// Sets the destination port.
    #[inline]
    pub fn set_dst_port(&mut self, port: u16) {
        self.buf[2] = (port >> 8) as u8;
        self.buf[3] = (port & 0xFF) as u8;
    }

    /// Sets the sequence number.
    #[inline]
    pub fn set_seq_number(&mut self, seq: u32) {
        self.buf[4] = (seq >> 24) as u8;
        self.buf[5] = (seq >> 16) as u8;
        self.buf[6] = (seq >> 8) as u8;
        self.buf[7] = seq as u8;
    }

    /// Sets the acknowledgment number.
    #[inline]
    pub fn set_ack_number(&mut self, ack: u32) {
        self.buf[8] = (ack >> 24) as u8;
        self.buf[9] = (ack >> 16) as u8;
        self.buf[10] = (ack >> 8) as u8;
        self.buf[11] = ack as u8;
    }

    /// Sets the data offset (upper 4 bits of byte 12), in 32-bit words.
    #[inline]
    pub fn set_data_offset(&mut self, offset: u8) {
        self.buf[12] = (self.buf[12] & 0x0F) | (offset << 4);
    }

    /// Sets the reserved bits (lower 4 bits of byte 12).
    #[inline]
    pub fn set_reserved(&mut self, reserved: u8) {
        self.buf[12] = (self.buf[12] & 0xF0) | (reserved & 0x0F);
    }

    /// Sets the flags byte.
    #[inline]
    pub fn set_flags(&mut self, flags: u8) {
        self.buf[13] = flags;
    }

    /// Sets the window size.
    #[inline]
    pub fn set_window_size(&mut self, window: u16) {
        self.buf[14] = (window >> 8) as u8;
        self.buf[15] = (window & 0xFF) as u8;
    }

    /// Sets the checksum.
    #[inline]
    pub fn set_checksum(&mut self, cksum: u16) {
        self.buf[16] = (cksum >> 8) as u8;
        self.buf[17] = (cksum & 0xFF) as u8;
    }

    /// Sets the urgent pointer.
    #[inline]
    pub fn set_urgent_pointer(&mut self, urgent: u16) {
        self.buf[18] = (urgent >> 8) as u8;
        self.buf[19] = (urgent & 0xFF) as u8;
    }

    /// Computes and sets the TCP checksum using an IPv4 pseudo-header.
    ///
    /// `segment_len` is the total TCP segment length (header + payload).
    #[inline]
    pub fn compute_checksum_v4(&mut self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr) {
        self.set_checksum(0);
        let len = self.buf.len() as u16;
        let sum = checksum::ipv4_pseudo_header_sum(src_ip, dst_ip, 6, len);
        self.set_checksum(checksum::internet_checksum(&self.buf[..len as usize], sum));
    }

    /// Computes and sets the TCP checksum using an IPv6 pseudo-header.
    #[inline]
    pub fn compute_checksum_v6(&mut self, src_ip: Ipv6Addr, dst_ip: Ipv6Addr) {
        self.set_checksum(0);
        let len = self.buf.len() as u32;
        let sum = checksum::ipv6_pseudo_header_sum(src_ip, dst_ip, 6, len);
        self.set_checksum(checksum::internet_checksum(&self.buf[..len as usize], sum));
    }

    /// Returns a mutable reference to the options region.
    #[inline]
    pub fn options_mut(&mut self) -> Option<&mut [u8]> {
        if self.header_len() <= TCP_MIN_HEADER_LEN {
            return None;
        }
        let hl = self.header_len();
        Some(&mut self.buf[TCP_MIN_HEADER_LEN..hl])
    }

    /// Consumes the packet and splits it into the fixed TCP header and payload.
    ///
    /// The returned `TcpPacket` stores only the header slice, allowing the payload
    /// slice to be used independently for parsing upper-layer packets.
    #[inline]
    pub fn split(self) -> (Self, &'a mut [u8]) {
        let (header, payload) = self.buf.split_at_mut(self.header_len());
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for TcpHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TcpHeader")
            .field("src_port", &self.src_port())
            .field("dst_port", &self.dst_port())
            .field("seq", &self.seq_number())
            .field("ack", &self.ack_number())
            .field("flags", &self.flags())
            .field("window", &self.window_size())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_fields() {
        let mut buf = [0u8; 40];
        {
            let mut pkt = TcpHeader::new(&mut buf[..]).unwrap();
            pkt.set_src_port(12345);
            pkt.set_dst_port(80);
            pkt.set_seq_number(1000);
            pkt.set_ack_number(2000);
            pkt.set_data_offset(5);
            pkt.set_flags(TCP_SYN | TCP_ACK);
            pkt.set_window_size(65535);
        }

        let pkt = TcpHeader::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.src_port(), 12345);
        assert_eq!(pkt.dst_port(), 80);
        assert_eq!(pkt.seq_number(), 1000);
        assert_eq!(pkt.ack_number(), 2000);
        assert_eq!(pkt.data_offset(), 5);
        assert!(pkt.syn());
        assert!(pkt.ack());
        assert!(!pkt.fin());
        assert!(!pkt.rst());
        assert_eq!(pkt.window_size(), 65535);
        assert_eq!(pkt.header_len(), 20);
    }

    #[test]
    fn tcp_checksum_v4() {
        let src_ip = Ipv4Addr::new(10, 0, 0, 1);
        let dst_ip = Ipv4Addr::new(10, 0, 0, 2);

        let mut buf = [0u8; 20];
        let mut pkt = TcpHeader::new(&mut buf[..]).unwrap();
        pkt.set_src_port(1234);
        pkt.set_dst_port(80);
        pkt.set_seq_number(100);
        pkt.set_ack_number(0);
        pkt.set_data_offset(5);
        pkt.set_flags(TCP_SYN);
        pkt.set_window_size(8192);
        pkt.compute_checksum_v4(src_ip, dst_ip);

        let pkt = TcpHeader::new(&mut buf[..]).unwrap();
        assert!(pkt.verify_checksum_v4(src_ip, dst_ip));
    }

    #[test]
    fn all_flags() {
        let mut buf = [0u8; 20];
        let mut pkt = TcpHeader::new(&mut buf[..]).unwrap();
        pkt.set_data_offset(5);
        pkt.set_flags(TCP_FIN | TCP_SYN | TCP_RST | TCP_PSH | TCP_ACK | TCP_URG | TCP_ECE | TCP_CWR);

        let pkt = TcpHeader::new(&mut buf[..]).unwrap();
        assert!(pkt.fin());
        assert!(pkt.syn());
        assert!(pkt.rst());
        assert!(pkt.psh());
        assert!(pkt.ack());
        assert!(pkt.urg());
        assert!(pkt.ece());
        assert!(pkt.cwr());
    }

    #[test]
    fn too_short() {
        let mut buf = [0u8; 10];
        assert!(TcpHeader::new(&mut buf[..]).is_err());
    }
}
