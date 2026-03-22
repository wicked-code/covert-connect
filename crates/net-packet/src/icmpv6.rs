//! ICMPv6 Header Format (RFC 4443)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |     Type      |     Code      |          Checksum             |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                     Rest of Header                            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//!
//! Echo Request/Reply (Type 128/129):
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |     Type      |     Code      |          Checksum             |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |           Identifier          |       Sequence Number         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                             Data                              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//!
//! Packet Too Big (Type 2):
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |     Type      |     Code      |          Checksum             |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                             MTU                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```
//!
//! Note: ICMPv6 checksum includes the IPv6 pseudo-header
//! (src addr, dst addr, upper-layer length, next header = 58).

use crate::checksum;
use core::fmt;
use core::net::Ipv6Addr;
use anyhow::{Result, bail};

/// Minimum ICMPv6 header length in bytes.
pub const ICMPV6_HEADER_LEN: usize = 8;

// ICMPv6 error message types.
pub const ICMPV6_DEST_UNREACHABLE: u8 = 1;
pub const ICMPV6_PACKET_TOO_BIG: u8 = 2;
pub const ICMPV6_TIME_EXCEEDED: u8 = 3;
pub const ICMPV6_PARAMETER_PROBLEM: u8 = 4;

// ICMPv6 informational message types.
pub const ICMPV6_ECHO_REQUEST: u8 = 128;
pub const ICMPV6_ECHO_REPLY: u8 = 129;

// Neighbor Discovery types.
pub const ICMPV6_ROUTER_SOLICITATION: u8 = 133;
pub const ICMPV6_ROUTER_ADVERTISEMENT: u8 = 134;
pub const ICMPV6_NEIGHBOR_SOLICITATION: u8 = 135;
pub const ICMPV6_NEIGHBOR_ADVERTISEMENT: u8 = 136;
pub const ICMPV6_REDIRECT: u8 = 137;

/// Zero-copy ICMPv6 header backed by a mutable byte slice.
pub struct Icmpv6Header<'a> {
    buf: &'a mut [u8],
}

impl<'a> Icmpv6Header<'a> {
    /// Creates a new `Icmpv6Header`. Requires at least 8 bytes.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < ICMPV6_HEADER_LEN {
            return bail!("Slice too short for ICMPv6 header.");
        }
        Ok(Self { buf })
    }

    /// Returns the type field.
    #[inline]
    pub fn icmp_type(&self) -> u8 {
        self.buf[0]
    }

    /// Returns the code field.
    #[inline]
    pub fn code(&self) -> u8 {
        self.buf[1]
    }

    /// Returns the checksum field.
    #[inline]
    pub fn checksum(&self) -> u16 {
        ((self.buf[2] as u16) << 8) | (self.buf[3] as u16)
    }

    /// Returns the rest-of-header as a u32.
    #[inline]
    pub fn rest_of_header(&self) -> u32 {
        ((self.buf[4] as u32) << 24) | ((self.buf[5] as u32) << 16) | ((self.buf[6] as u32) << 8) | (self.buf[7] as u32)
    }

    /// Returns the identifier for Echo Request/Reply.
    #[inline]
    pub fn identifier(&self) -> u16 {
        ((self.buf[4] as u16) << 8) | (self.buf[5] as u16)
    }

    /// Returns the sequence number for Echo Request/Reply.
    #[inline]
    pub fn sequence_number(&self) -> u16 {
        ((self.buf[6] as u16) << 8) | (self.buf[7] as u16)
    }

    /// Returns the MTU field for Packet Too Big messages.
    #[inline]
    pub fn mtu(&self) -> u32 {
        self.rest_of_header()
    }

    /// Returns the fixed header length (always 8).
    #[inline]
    pub const fn header_len(&self) -> usize {
        ICMPV6_HEADER_LEN
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..ICMPV6_HEADER_LEN]
    }

    /// Returns the payload/data bytes after the 8-byte header.
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[ICMPV6_HEADER_LEN..]
    }

    /// Verifies the ICMPv6 checksum using the IPv6 pseudo-header.
    ///
    /// ICMPv6 checksums are computed over a pseudo-header (source addr,
    /// destination addr, ICMPv6 length, next header = 58) plus the entire
    /// ICMPv6 message.
    #[inline]
    pub fn verify_checksum(&self, src: Ipv6Addr, dst: Ipv6Addr, message_len: usize) -> bool {
        let sum = checksum::ipv6_pseudo_header_sum(src, dst, 58, message_len as u32);
        checksum::internet_checksum(&self.buf[..message_len], sum) == 0
    }

    /// Sets the type field.
    #[inline]
    pub fn set_type(&mut self, icmp_type: u8) {
        self.buf[0] = icmp_type;
    }

    /// Sets the code field.
    #[inline]
    pub fn set_code(&mut self, code: u8) {
        self.buf[1] = code;
    }

    /// Sets the checksum field.
    #[inline]
    pub fn set_checksum(&mut self, cksum: u16) {
        self.buf[2] = (cksum >> 8) as u8;
        self.buf[3] = (cksum & 0xFF) as u8;
    }

    /// Sets the rest-of-header field.
    #[inline]
    pub fn set_rest_of_header(&mut self, value: u32) {
        self.buf[4] = (value >> 24) as u8;
        self.buf[5] = (value >> 16) as u8;
        self.buf[6] = (value >> 8) as u8;
        self.buf[7] = value as u8;
    }

    /// Sets the identifier field for Echo Request/Reply.
    #[inline]
    pub fn set_identifier(&mut self, id: u16) {
        self.buf[4] = (id >> 8) as u8;
        self.buf[5] = (id & 0xFF) as u8;
    }

    /// Sets the sequence number field for Echo Request/Reply.
    #[inline]
    pub fn set_sequence_number(&mut self, seq: u16) {
        self.buf[6] = (seq >> 8) as u8;
        self.buf[7] = (seq & 0xFF) as u8;
    }

    /// Sets the MTU field for Packet Too Big messages.
    #[inline]
    pub fn set_mtu(&mut self, mtu: u32) {
        self.set_rest_of_header(mtu);
    }

    /// Computes and sets the ICMPv6 checksum using the IPv6 pseudo-header.
    #[inline]
    pub fn compute_checksum(&mut self, src: Ipv6Addr, dst: Ipv6Addr, message_len: usize) {
        self.set_checksum(0);
        let sum = checksum::ipv6_pseudo_header_sum(src, dst, 58, message_len as u32);
        self.set_checksum(checksum::internet_checksum(&self.buf[..message_len], sum));
    }

    /// Consumes the packet and splits it into the fixed ICMPv6 header and payload.
    ///
    /// The returned `Icmpv6Header` stores only the header slice, allowing the payload
    /// slice to be used independently for parsing upper-layer packets.
    #[inline]
    pub(crate) fn split(self) -> (Self, &'a mut [u8]) {
        let (header, payload) = self.buf.split_at_mut(self.header_len());
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for Icmpv6Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Icmpv6Header")
            .field("type", &self.icmp_type())
            .field("code", &self.code())
            .field("checksum", &self.checksum())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: Ipv6Addr = Ipv6Addr::new(0xFE80, 0, 0, 0, 0, 0, 0, 1);
    const DST: Ipv6Addr = Ipv6Addr::new(0xFE80, 0, 0, 0, 0, 0, 0, 2);

    #[test]
    fn read_write_fields() {
        let mut buf = [0u8; 16];
        {
            let mut pkt = Icmpv6Header::new(&mut buf[..]).unwrap();
            pkt.set_type(ICMPV6_ECHO_REQUEST);
            pkt.set_code(0);
            pkt.set_identifier(0x1234);
            pkt.set_sequence_number(1);
            pkt.compute_checksum(SRC, DST, 16);
        }

        let pkt = Icmpv6Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.icmp_type(), ICMPV6_ECHO_REQUEST);
        assert_eq!(pkt.code(), 0);
        assert_eq!(pkt.identifier(), 0x1234);
        assert_eq!(pkt.sequence_number(), 1);
        assert!(pkt.verify_checksum(SRC, DST, 16));
    }

    #[test]
    fn echo_reply() {
        let mut buf = [0u8; 8];
        {
            let mut pkt = Icmpv6Header::new(&mut buf[..]).unwrap();
            pkt.set_type(ICMPV6_ECHO_REPLY);
            pkt.set_code(0);
            pkt.set_identifier(0xABCD);
            pkt.set_sequence_number(42);
            pkt.compute_checksum(SRC, DST, 8);
        }

        let pkt = Icmpv6Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.icmp_type(), ICMPV6_ECHO_REPLY);
        assert_eq!(pkt.identifier(), 0xABCD);
        assert_eq!(pkt.sequence_number(), 42);
        assert!(pkt.verify_checksum(SRC, DST, 8));
    }

    #[test]
    fn packet_too_big() {
        let mut buf = [0u8; 8];
        {
            let mut pkt = Icmpv6Header::new(&mut buf[..]).unwrap();
            pkt.set_type(ICMPV6_PACKET_TOO_BIG);
            pkt.set_code(0);
            pkt.set_mtu(1280);
            pkt.compute_checksum(SRC, DST, 8);
        }

        let pkt = Icmpv6Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.icmp_type(), ICMPV6_PACKET_TOO_BIG);
        assert_eq!(pkt.mtu(), 1280);
        assert!(pkt.verify_checksum(SRC, DST, 8));
    }

    #[test]
    fn too_short() {
        let mut buf = [0u8; 4];
        assert!(Icmpv6Header::new(&mut buf[..]).is_err());
    }
}
