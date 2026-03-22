//! UDP Header Format (RFC 768)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |          Source Port          |       Destination Port        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |            Length             |           Checksum            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                             Data                              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::checksum;
use core::fmt;
use core::net::{Ipv4Addr, Ipv6Addr};
use anyhow::{Result, bail};

/// Fixed UDP header length in bytes.
pub const UDP_HEADER_LEN: usize = 8;

/// Zero-copy UDP header backed by a mutable byte slice.
pub struct UdpHeader<'a> {
    buf: &'a mut [u8],
}

impl<'a> UdpHeader<'a> {
    /// Creates a new `UdpHeader`. Requires at least 8 bytes.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < UDP_HEADER_LEN {
            bail!("Slice too short for UDP header.");
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

    /// Returns the length field.
    #[inline]
    pub fn length(&self) -> u16 {
        ((self.buf[4] as u16) << 8) | (self.buf[5] as u16)
    }

    /// Returns the checksum field.
    #[inline]
    pub fn checksum(&self) -> u16 {
        ((self.buf[6] as u16) << 8) | (self.buf[7] as u16)
    }

    /// Returns the fixed header length (always 8).
    #[inline]
    pub const fn header_len(&self) -> usize {
        UDP_HEADER_LEN
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..UDP_HEADER_LEN]
    }

    /// Verifies the UDP checksum against an IPv4 pseudo-header.
    ///
    /// A checksum of 0 means the checksum was not computed (valid for IPv4 UDP).
    #[inline]
    pub fn verify_checksum_v4(&self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr, datagram_len: u16) -> bool {
        if self.checksum() == 0 {
            return true; // No checksum computed (valid for IPv4).
        }
        let sum = checksum::ipv4_pseudo_header_sum(src_ip, dst_ip, 17, datagram_len);
        checksum::internet_checksum(&self.buf[..datagram_len as usize], sum) == 0
    }

    /// Verifies the UDP checksum against an IPv6 pseudo-header.
    #[inline]
    pub fn verify_checksum_v6(&self, src_ip: Ipv6Addr, dst_ip: Ipv6Addr, datagram_len: u32) -> bool {
        let sum = checksum::ipv6_pseudo_header_sum(src_ip, dst_ip, 17, datagram_len);
        checksum::internet_checksum(&self.buf[..datagram_len as usize], sum) == 0
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

    /// Sets the length field.
    #[inline]
    pub fn set_length(&mut self, length: u16) {
        self.buf[4] = (length >> 8) as u8;
        self.buf[5] = (length & 0xFF) as u8;
    }

    /// Sets the checksum field.
    #[inline]
    pub fn set_checksum(&mut self, cksum: u16) {
        self.buf[6] = (cksum >> 8) as u8;
        self.buf[7] = (cksum & 0xFF) as u8;
    }

    /// Computes and sets the UDP checksum using an IPv4 pseudo-header.
    #[inline]
    pub fn compute_checksum_v4(&mut self, src_ip: Ipv4Addr, dst_ip: Ipv4Addr, datagram_len: u16) {
        self.set_checksum(0);
        let sum = checksum::ipv4_pseudo_header_sum(src_ip, dst_ip, 17, datagram_len);
        let result = checksum::internet_checksum(&self.buf[..datagram_len as usize], sum);
        // UDP uses 0xFFFF instead of 0x0000 for the checksum.
        self.set_checksum(if result == 0 { 0xFFFF } else { result });
    }

    /// Computes and sets the UDP checksum using an IPv6 pseudo-header.
    #[inline]
    pub fn compute_checksum_v6(&mut self, src_ip: Ipv6Addr, dst_ip: Ipv6Addr, datagram_len: u32) {
        self.set_checksum(0);
        let sum = checksum::ipv6_pseudo_header_sum(src_ip, dst_ip, 17, datagram_len);
        let result = checksum::internet_checksum(&self.buf[..datagram_len as usize], sum);
        self.set_checksum(if result == 0 { 0xFFFF } else { result });
    }

    /// Consumes the packet and splits it into the fixed UDP header and payload.
    ///
    /// The returned `UdpHeader` stores only the header slice, allowing the payload
    /// slice to be used independently for parsing upper-layer packets.
    #[inline]
    pub(crate) fn split(self) -> (Self, &'a mut [u8]) {
        let (header, payload) = self.buf.split_at_mut(self.header_len());
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for UdpHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UdpHeader")
            .field("src_port", &self.src_port())
            .field("dst_port", &self.dst_port())
            .field("length", &self.length())
            .field("checksum", &self.checksum())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_fields() {
        let mut buf = [0u8; 20];
        {
            let mut pkt = UdpHeader::new(&mut buf[..]).unwrap();
            pkt.set_src_port(53);
            pkt.set_dst_port(12345);
            pkt.set_length(20);
        }

        let pkt = UdpHeader::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.src_port(), 53);
        assert_eq!(pkt.dst_port(), 12345);
        assert_eq!(pkt.length(), 20);
        assert_eq!(pkt.header_len(), 8);
    }

    #[test]
    fn udp_checksum_v4() {
        let src_ip = Ipv4Addr::new(10, 0, 0, 1);
        let dst_ip = Ipv4Addr::new(10, 0, 0, 2);
        let datagram_len: u16 = 12; // 8 header + 4 payload

        let mut buf = [0u8; 12];
        {
            let mut pkt = UdpHeader::new(&mut buf[..]).unwrap();
            pkt.set_src_port(1234);
            pkt.set_dst_port(5678);
            pkt.set_length(datagram_len);
            pkt.compute_checksum_v4(src_ip, dst_ip, datagram_len);
        }

        let pkt = UdpHeader::new(&mut buf[..]).unwrap();
        assert!(pkt.verify_checksum_v4(src_ip, dst_ip, datagram_len));
    }

    #[test]
    fn udp_zero_checksum_v4() {
        let mut buf = [0u8; 8];
        {
            let mut pkt = UdpHeader::new(&mut buf[..]).unwrap();
            pkt.set_src_port(1000);
            pkt.set_dst_port(2000);
            pkt.set_length(8);
            pkt.set_checksum(0);
        }

        let pkt = UdpHeader::new(&mut buf[..]).unwrap();
        assert!(pkt.verify_checksum_v4(Ipv4Addr::UNSPECIFIED, Ipv4Addr::UNSPECIFIED, 8));
    }

    #[test]
    fn too_short() {
        let mut buf = [0u8; 4];
        assert!(UdpHeader::new(&mut buf[..]).is_err());
    }
}
