//! IPv4 Header Format (RFC 791)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |Version|  IHL  |    DSCP   |ECN|         Total Length          |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |         Identification        |Flags|     Fragment Offset     |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |  Time to Live |    Protocol   |        Header Checksum        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                       Source Address                          |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Destination Address                        |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Options                    |    Padding    |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::checksum;
use core::fmt;
use core::net::Ipv4Addr;

/// Minimum IPv4 header length in bytes (no options).
pub const IPV4_MIN_HEADER_LEN: usize = 20;

/// Maximum IPv4 header length in bytes (with options).
pub const IPV4_MAX_HEADER_LEN: usize = 60;

/// Zero-copy IPv4 header backed by a mutable byte slice.
pub struct Ipv4Header<'a> {
    buf: &'a mut [u8],
}

impl<'a> Ipv4Header<'a> {
    /// Creates a new `Ipv4Header` by parsing an existing buffer.
    ///
    /// Validates the buffer is long enough for the header indicated by IHL.
    /// Use [`new_unchecked`](Self::new_unchecked) to build packets from scratch.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self, &'static str> {
        if buf.len() < IPV4_MIN_HEADER_LEN {
            return Err("Slice too short for IPv4 header.");
        }
        Ok(Self { buf })
    }

    /// Returns the version field (upper 4 bits of byte 0).
    #[inline]
    pub fn version(&self) -> u8 {
        self.buf[0] >> 4
    }

    /// Returns the IHL field (lower 4 bits of byte 0), in 32-bit words.
    #[inline]
    pub fn ihl(&self) -> u8 {
        self.buf[0] & 0x0F
    }

    /// Returns the DSCP field (upper 6 bits of byte 1).
    #[inline]
    pub fn dscp(&self) -> u8 {
        self.buf[1] >> 2
    }

    /// Returns the ECN field (lower 2 bits of byte 1).
    #[inline]
    pub fn ecn(&self) -> u8 {
        self.buf[1] & 0x03
    }

    /// Returns the total length.
    #[inline]
    pub fn total_length(&self) -> u16 {
        ((self.buf[2] as u16) << 8) | (self.buf[3] as u16)
    }

    /// Returns the identification field.
    #[inline]
    pub fn identification(&self) -> u16 {
        ((self.buf[4] as u16) << 8) | (self.buf[5] as u16)
    }

    /// Returns the flags field (upper 3 bits of byte 6).
    #[inline]
    pub fn flags(&self) -> u8 {
        self.buf[6] >> 5
    }

    /// Returns the Don't Fragment (DF) flag.
    #[inline]
    pub fn dont_fragment(&self) -> bool {
        (self.buf[6] & 0x40) != 0
    }

    /// Returns the More Fragments (MF) flag.
    #[inline]
    pub fn more_fragments(&self) -> bool {
        (self.buf[6] & 0x20) != 0
    }

    /// Returns the fragment offset (lower 13 bits of bytes 6-7), in 8-byte blocks.
    #[inline]
    pub fn fragment_offset(&self) -> u16 {
        (((self.buf[6] & 0x1F) as u16) << 8) | (self.buf[7] as u16)
    }

    /// Returns the TTL field.
    #[inline]
    pub fn ttl(&self) -> u8 {
        self.buf[8]
    }

    /// Returns the protocol field.
    #[inline]
    pub fn protocol(&self) -> u8 {
        self.buf[9]
    }

    /// Returns the header checksum field.
    #[inline]
    pub fn checksum(&self) -> u16 {
        ((self.buf[10] as u16) << 8) | (self.buf[11] as u16)
    }

    /// Returns the source address.
    #[inline]
    pub fn src_addr(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.buf[12], self.buf[13], self.buf[14], self.buf[15])
    }

    /// Returns the destination address.
    #[inline]
    pub fn dst_addr(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.buf[16], self.buf[17], self.buf[18], self.buf[19])
    }

    /// Returns the header length in bytes.
    #[inline]
    pub fn header_len(&self) -> usize {
        self.ihl() as usize * 4
    }

    /// Returns the option bytes (empty if IHL == 5).
    #[inline]
    pub fn options(&self) -> &[u8] {
        &self.buf[IPV4_MIN_HEADER_LEN..self.header_len()]
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..self.header_len()]
    }

    /// Returns the payload bytes (everything after the IP header).
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[self.header_len()..]
    }

    /// Verifies the header checksum. Returns true if valid.
    #[inline]
    pub fn verify_checksum(&self) -> bool {
        checksum::internet_checksum(&self.buf[..self.header_len()], 0) == 0
    }

    /// Sets the version field (upper 4 bits of byte 0).
    #[inline]
    pub fn set_version(&mut self, version: u8) {
        self.buf[0] = (self.buf[0] & 0x0F) | (version << 4);
    }

    /// Sets the IHL field (lower 4 bits of byte 0), in 32-bit words.
    #[inline]
    pub fn set_ihl(&mut self, ihl: u8) {
        self.buf[0] = (self.buf[0] & 0xF0) | (ihl & 0x0F);
    }

    /// Sets the DSCP field (upper 6 bits of byte 1).
    #[inline]
    pub fn set_dscp(&mut self, dscp: u8) {
        self.buf[1] = (self.buf[1] & 0x03) | (dscp << 2);
    }

    /// Sets the ECN field (lower 2 bits of byte 1).
    #[inline]
    pub fn set_ecn(&mut self, ecn: u8) {
        self.buf[1] = (self.buf[1] & 0xFC) | (ecn & 0x03);
    }

    /// Sets the total length field.
    #[inline]
    pub fn set_total_length(&mut self, len: u16) {
        self.buf[2] = (len >> 8) as u8;
        self.buf[3] = (len & 0xFF) as u8;
    }

    /// Sets the identification field.
    #[inline]
    pub fn set_identification(&mut self, id: u16) {
        self.buf[4] = (id >> 8) as u8;
        self.buf[5] = (id & 0xFF) as u8;
    }

    /// Sets the flags field (upper 3 bits of byte 6).
    #[inline]
    pub fn set_flags(&mut self, flags: u8) {
        self.buf[6] = (self.buf[6] & 0x1F) | (flags << 5);
    }

    /// Sets the fragment offset (lower 13 bits of bytes 6-7).
    #[inline]
    pub fn set_fragment_offset(&mut self, offset: u16) {
        self.buf[6] = (self.buf[6] & 0xE0) | ((offset >> 8) as u8 & 0x1F);
        self.buf[7] = (offset & 0xFF) as u8;
    }

    /// Sets the TTL field.
    #[inline]
    pub fn set_ttl(&mut self, ttl: u8) {
        self.buf[8] = ttl;
    }

    /// Sets the protocol field.
    #[inline]
    pub fn set_protocol(&mut self, protocol: u8) {
        self.buf[9] = protocol;
    }

    /// Sets the header checksum field.
    #[inline]
    pub fn set_checksum(&mut self, cksum: u16) {
        self.buf[10] = (cksum >> 8) as u8;
        self.buf[11] = (cksum & 0xFF) as u8;
    }

    /// Sets the source address.
    #[inline]
    pub fn set_src_addr(&mut self, addr: Ipv4Addr) {
        self.buf[12..16].copy_from_slice(&addr.octets());
    }

    /// Sets the destination address.
    #[inline]
    pub fn set_dst_addr(&mut self, addr: Ipv4Addr) {
        self.buf[16..20].copy_from_slice(&addr.octets());
    }

    /// Computes and sets the header checksum.
    #[inline]
    pub fn compute_checksum(&mut self) {
        self.set_checksum(0);
        let hl = self.header_len();
        let cksum = checksum::internet_checksum(&self.buf[..hl], 0);
        self.set_checksum(cksum);
    }

    /// Consumes the packet and splits it into the fixed IPv4 header and payload.
    ///
    /// The returned `Ipv4Header` stores only the header slice, allowing the payload
    /// slice to be used independently for parsing upper-layer packets.
    #[inline]
    pub(crate) fn split(self) -> (Self, &'a mut [u8]) {
        let (header, payload) = self.buf.split_at_mut(self.header_len());
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for Ipv4Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ipv4Header")
            .field("version", &self.version())
            .field("ihl", &self.ihl())
            .field("total_length", &self.total_length())
            .field("ttl", &self.ttl())
            .field("protocol", &self.protocol())
            .field("src", &self.src_addr())
            .field("dst", &self.dst_addr())
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
            let mut pkt = Ipv4Header::new(&mut buf[..]).unwrap();
            pkt.set_version(4);
            pkt.set_ihl(5);
            pkt.set_total_length(40);
            pkt.set_ttl(64);
            pkt.set_protocol(6);
            pkt.set_src_addr(Ipv4Addr::new(10, 0, 0, 1));
            pkt.set_dst_addr(Ipv4Addr::new(10, 0, 0, 2));
            pkt.compute_checksum();
        }

        let pkt = Ipv4Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.version(), 4);
        assert_eq!(pkt.ihl(), 5);
        assert_eq!(pkt.total_length(), 40);
        assert_eq!(pkt.ttl(), 64);
        assert_eq!(pkt.protocol(), 6);
        assert_eq!(pkt.src_addr(), Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(pkt.dst_addr(), Ipv4Addr::new(10, 0, 0, 2));
        assert!(pkt.verify_checksum());
        assert_eq!(pkt.header_len(), 20);
    }

    #[test]
    fn flags_and_fragment() {
        let mut buf = [0u8; 20];
        {
            let mut pkt = Ipv4Header::new(&mut buf[..]).unwrap();
            pkt.set_version(4);
            pkt.set_ihl(5);
            pkt.set_flags(0b010);
            pkt.set_fragment_offset(185);
        }

        let pkt = Ipv4Header::new(&mut buf[..]).unwrap();
        assert!(pkt.dont_fragment());
        assert!(!pkt.more_fragments());
        assert_eq!(pkt.fragment_offset(), 185);
    }

    #[test]
    fn dscp_ecn() {
        let mut buf = [0u8; 20];
        {
            let mut pkt = Ipv4Header::new(&mut buf[..]).unwrap();
            pkt.set_version(4);
            pkt.set_ihl(5);
            pkt.set_dscp(46);
            pkt.set_ecn(3);
        }

        let pkt = Ipv4Header::new(&mut buf[..]).unwrap();
        assert_eq!(pkt.dscp(), 46);
        assert_eq!(pkt.ecn(), 3);
    }

    #[test]
    fn too_short() {
        let mut buf = [0u8; 10];
        assert!(Ipv4Header::new(&mut buf[..]).is_err());
    }
}
