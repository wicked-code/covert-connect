//! ICMPv4 Header Format (RFC 792)
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
//! Echo Request/Reply (Type 8/0):
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |     Type      |     Code      |          Checksum             |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |           Identifier          |       Sequence Number         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                             Data                              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::checksum;
use core::fmt;

/// Minimum ICMPv4 header length in bytes.
pub const ICMPV4_HEADER_LEN: usize = 8;

// Common ICMPv4 types.
pub const ICMP_ECHO_REPLY: u8 = 0;
pub const ICMP_DEST_UNREACHABLE: u8 = 3;
pub const ICMP_REDIRECT: u8 = 5;
pub const ICMP_ECHO_REQUEST: u8 = 8;
pub const ICMP_TIME_EXCEEDED: u8 = 11;

/// Zero-copy ICMPv4 header backed by a mutable byte slice.
pub struct Icmpv4Header<'a> {
    buf: &'a mut [u8],
}

impl<'a> Icmpv4Header<'a> {
    /// Creates a new `Icmpv4Header`. Requires at least 8 bytes.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self, &'static str> {
        if buf.len() < ICMPV4_HEADER_LEN {
            return Err("Slice too short for ICMPv4 header.");
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

    /// Returns the fixed header length (always 8).
    #[inline]
    pub const fn header_len(&self) -> usize {
        ICMPV4_HEADER_LEN
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..ICMPV4_HEADER_LEN]
    }

    /// Returns the payload/data bytes after the 8-byte header.
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[ICMPV4_HEADER_LEN..]
    }

    /// Verifies the ICMP checksum over the entire message.
    #[inline]
    pub fn verify_checksum(&self, message_len: usize) -> bool {
        checksum::internet_checksum(&self.buf[..message_len], 0) == 0
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

    /// Computes and sets the ICMP checksum over the entire ICMP message.
    #[inline]
    pub fn compute_checksum(&mut self, message_len: usize) {
        self.set_checksum(0);
        let cksum = checksum::internet_checksum(&self.buf[..message_len], 0);
        self.set_checksum(cksum);
    }

    /// Consumes the packet and splits it into the fixed ICMPv4 header and payload.
    ///
    /// The returned `Icmpv4Header` stores only the header slice, allowing the payload
    /// slice to be used independently for parsing upper-layer packets.
    #[inline]
    pub(crate) fn split(self) -> (Self, &'a mut [u8]) {
        let (header, payload) = self.buf.split_at_mut(self.header_len());
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for Icmpv4Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Icmpv4Header")
            .field("type", &self.icmp_type())
            .field("code", &self.code())
            .field("checksum", &self.checksum())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_fields() {
        let mut buf = [0u8; 16];
        {
            let mut pkt = Icmpv4Header::new(&mut buf[..]).unwrap();
            pkt.set_type(ICMP_ECHO_REQUEST);
            pkt.set_code(0);
            pkt.set_identifier(0x1234);
            pkt.set_sequence_number(1);
            pkt.compute_checksum(16);
        }

        let pkt = Icmpv4Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.icmp_type(), ICMP_ECHO_REQUEST);
        assert_eq!(pkt.code(), 0);
        assert_eq!(pkt.identifier(), 0x1234);
        assert_eq!(pkt.sequence_number(), 1);
        assert!(pkt.verify_checksum(16));
    }

    #[test]
    fn echo_reply() {
        let mut buf = [0u8; 8];
        {
            let mut pkt = Icmpv4Header::new(&mut buf[..]).unwrap();
            pkt.set_type(ICMP_ECHO_REPLY);
            pkt.set_code(0);
            pkt.set_identifier(0xABCD);
            pkt.set_sequence_number(42);
            pkt.compute_checksum(8);
        }

        let pkt = Icmpv4Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.icmp_type(), ICMP_ECHO_REPLY);
        assert_eq!(pkt.identifier(), 0xABCD);
        assert_eq!(pkt.sequence_number(), 42);
        assert!(pkt.verify_checksum(8));
    }

    #[test]
    fn too_short() {
        let mut buf = [0u8; 4];
        assert!(Icmpv4Header::new(&mut buf[..]).is_err());
    }
}
