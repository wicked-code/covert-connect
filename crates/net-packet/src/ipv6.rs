//! IPv6 Header Format (RFC 8200)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |Version| Traffic Class |           Flow Label                  |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |         Payload Length        |  Next Header  |   Hop Limit   |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                                                               |
//! +                                                               +
//! |                                                               |
//! +                         Source Address                        +
//! |                                                               |
//! +                                                               +
//! |                                                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                                                               |
//! +                                                               +
//! |                                                               |
//! +                      Destination Address                      +
//! |                                                               |
//! +                                                               +
//! |                                                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use anyhow::{Result, bail};
use core::fmt;
use core::net::Ipv6Addr;

/// Fixed IPv6 header length in bytes.
pub const IPV6_HEADER_LEN: usize = 40;

/// Zero-copy IPv6 header backed by a mutable byte slice.
pub struct Ipv6Header<'a> {
    buf: &'a mut [u8],
}

impl<'a> Ipv6Header<'a> {
    /// Creates a new `Ipv6Header`. Requires at least 40 bytes.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < IPV6_HEADER_LEN {
            bail!("Slice too short for IPv6 header.");
        }
        Ok(Self { buf })
    }

    /// Returns the version field (upper 4 bits of byte 0).
    #[inline]
    pub fn version(&self) -> u8 {
        self.buf[0] >> 4
    }

    /// Returns the traffic class (8 bits across bytes 0-1).
    #[inline]
    pub fn traffic_class(&self) -> u8 {
        ((self.buf[0] & 0x0F) << 4) | (self.buf[1] >> 4)
    }

    /// Returns the flow label (20 bits across bytes 1-3).
    #[inline]
    pub fn flow_label(&self) -> u32 {
        ((self.buf[1] as u32 & 0x0F) << 16) | ((self.buf[2] as u32) << 8) | (self.buf[3] as u32)
    }

    /// Returns the payload length.
    #[inline]
    pub fn payload_length(&self) -> u16 {
        ((self.buf[4] as u16) << 8) | (self.buf[5] as u16)
    }

    /// Returns the next header field.
    #[inline]
    pub fn next_header(&self) -> u8 {
        self.buf[6]
    }

    /// Returns the hop limit.
    #[inline]
    pub fn hop_limit(&self) -> u8 {
        self.buf[7]
    }

    /// Returns the source address.
    #[inline]
    pub fn src_addr(&self) -> Ipv6Addr {
        let mut addr = [0u8; 16];
        addr.copy_from_slice(&self.buf[8..24]);
        Ipv6Addr::from(addr)
    }

    /// Returns a reference to the source address bytes.
    #[inline]
    pub fn src_addr_ref(&self) -> &[u8] {
        &self.buf[8..24]
    }

    /// Returns the destination address.
    #[inline]
    pub fn dst_addr(&self) -> Ipv6Addr {
        let mut addr = [0u8; 16];
        addr.copy_from_slice(&self.buf[24..40]);
        Ipv6Addr::from(addr)
    }

    /// Returns a reference to the destination address bytes.
    #[inline]
    pub fn dst_addr_ref(&self) -> &[u8] {
        &self.buf[24..40]
    }

    /// Returns the fixed header length (always 40).
    #[inline]
    pub const fn header_len(&self) -> usize {
        IPV6_HEADER_LEN
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..IPV6_HEADER_LEN]
    }

    /// Returns the payload bytes.
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[IPV6_HEADER_LEN..]
    }

    /// Sets the version field (upper 4 bits of byte 0).
    #[inline]
    pub fn set_version(&mut self, version: u8) {
        self.buf[0] = (self.buf[0] & 0x0F) | (version << 4);
    }

    /// Sets the traffic class (8 bits across bytes 0-1).
    #[inline]
    pub fn set_traffic_class(&mut self, tc: u8) {
        self.buf[0] = (self.buf[0] & 0xF0) | (tc >> 4);
        self.buf[1] = (self.buf[1] & 0x0F) | (tc << 4);
    }

    /// Sets the flow label (20 bits across bytes 1-3).
    #[inline]
    pub fn set_flow_label(&mut self, label: u32) {
        self.buf[1] = (self.buf[1] & 0xF0) | ((label >> 16) as u8 & 0x0F);
        self.buf[2] = (label >> 8) as u8;
        self.buf[3] = label as u8;
    }

    /// Sets the payload length.
    #[inline]
    pub fn set_payload_length(&mut self, len: u16) {
        self.buf[4] = (len >> 8) as u8;
        self.buf[5] = (len & 0xFF) as u8;
    }

    /// Sets the next header field.
    #[inline]
    pub fn set_next_header(&mut self, nh: u8) {
        self.buf[6] = nh;
    }

    /// Sets the hop limit.
    #[inline]
    pub fn set_hop_limit(&mut self, limit: u8) {
        self.buf[7] = limit;
    }

    /// Sets the source address.
    #[inline]
    pub fn set_src_addr(&mut self, addr: Ipv6Addr) {
        self.buf[8..24].copy_from_slice(&addr.octets());
    }

    /// Sets the destination address.
    #[inline]
    pub fn set_dst_addr(&mut self, addr: Ipv6Addr) {
        self.buf[24..40].copy_from_slice(&addr.octets());
    }

    /// Consumes the packet and splits it into the fixed IPv6 header and payload.
    ///
    /// The returned `Ipv6Packet` stores only the header slice, allowing the payload
    /// slice to be used independently for parsing upper-layer packets.
    #[inline]
    pub fn split(self) -> (Self, &'a mut [u8]) {
        let (header, payload) = self.buf.split_at_mut(self.header_len());
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for Ipv6Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ipv6Header")
            .field("version", &self.version())
            .field("traffic_class", &self.traffic_class())
            .field("flow_label", &self.flow_label())
            .field("payload_length", &self.payload_length())
            .field("next_header", &self.next_header())
            .field("hop_limit", &self.hop_limit())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_fields() {
        let mut buf = [0u8; 60];
        let src = Ipv6Addr::new(0xFE80, 0, 0, 0, 0, 0, 0, 1);
        let dst = Ipv6Addr::new(0xFE80, 0, 0, 0, 0, 0, 0, 2);

        {
            let mut pkt = Ipv6Header::new(&mut buf[..]).unwrap();
            pkt.set_version(6);
            pkt.set_traffic_class(0xAB);
            pkt.set_flow_label(0x12345);
            pkt.set_payload_length(20);
            pkt.set_next_header(6);
            pkt.set_hop_limit(64);
            pkt.set_src_addr(src);
            pkt.set_dst_addr(dst);
        }

        let pkt = Ipv6Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.version(), 6);
        assert_eq!(pkt.traffic_class(), 0xAB);
        assert_eq!(pkt.flow_label(), 0x12345);
        assert_eq!(pkt.payload_length(), 20);
        assert_eq!(pkt.next_header(), 6);
        assert_eq!(pkt.hop_limit(), 64);
        assert_eq!(pkt.src_addr(), src);
        assert_eq!(pkt.dst_addr(), dst);
        assert_eq!(pkt.header_len(), 40);
    }

    #[test]
    fn too_short() {
        let mut buf = [0u8; 30];
        assert!(Ipv6Header::new(&mut buf[..]).is_err());
    }
}
