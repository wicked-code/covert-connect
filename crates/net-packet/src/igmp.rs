//! IGMP Header Format (RFC 2236 / RFC 3376)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |      Type     | Max Resp Time |           Checksum            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                         Group Address                         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use crate::checksum;
use anyhow::{Result, bail};
use core::fmt;
use core::net::Ipv4Addr;

/// Minimum IGMP header length in bytes.
pub const IGMP_HEADER_LEN: usize = 8;

// IGMP message types.
pub const IGMP_MEMBERSHIP_QUERY: u8 = 0x11;
pub const IGMP_V1_MEMBERSHIP_REPORT: u8 = 0x12;
pub const IGMP_V2_MEMBERSHIP_REPORT: u8 = 0x16;
pub const IGMP_V3_MEMBERSHIP_REPORT: u8 = 0x22;
pub const IGMP_LEAVE_GROUP: u8 = 0x17;

/// Zero-copy IGMP header backed by a mutable byte slice.
pub struct IgmpHeader<'a> {
    buf: &'a mut [u8],
}

impl<'a> IgmpHeader<'a> {
    /// Creates a new `IgmpHeader`. Requires at least 8 bytes.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < IGMP_HEADER_LEN {
            bail!("Slice too short for IGMP header.");
        }
        Ok(Self { buf })
    }

    /// Returns the IGMP message type.
    #[inline]
    pub fn igmp_type(&self) -> u8 {
        self.buf[0]
    }

    /// Returns the max response time.
    #[inline]
    pub fn max_resp_time(&self) -> u8 {
        self.buf[1]
    }

    /// Returns the checksum field.
    #[inline]
    pub fn checksum(&self) -> u16 {
        ((self.buf[2] as u16) << 8) | (self.buf[3] as u16)
    }

    /// Returns the group address.
    #[inline]
    pub fn group_addr(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.buf[4], self.buf[5], self.buf[6], self.buf[7])
    }

    /// Returns the fixed header length (always 8).
    #[inline]
    pub const fn header_len(&self) -> usize {
        IGMP_HEADER_LEN
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..IGMP_HEADER_LEN]
    }

    /// Returns the payload/data bytes after the 8-byte header.
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[IGMP_HEADER_LEN..]
    }

    /// Sets the IGMP message type.
    #[inline]
    pub fn set_type(&mut self, igmp_type: u8) {
        self.buf[0] = igmp_type;
    }

    /// Sets the max response time.
    #[inline]
    pub fn set_max_resp_time(&mut self, time: u8) {
        self.buf[1] = time;
    }

    /// Sets the checksum field.
    #[inline]
    pub fn set_checksum(&mut self, cksum: u16) {
        self.buf[2] = (cksum >> 8) as u8;
        self.buf[3] = (cksum & 0xFF) as u8;
    }

    /// Sets the group address.
    #[inline]
    pub fn set_group_addr(&mut self, addr: Ipv4Addr) {
        let octets = addr.octets();
        self.buf[4] = octets[0];
        self.buf[5] = octets[1];
        self.buf[6] = octets[2];
        self.buf[7] = octets[3];
    }

    /// Verifies the IGMP checksum over the entire message.
    #[inline]
    pub fn verify_checksum(&self, message_len: usize) -> bool {
        checksum::internet_checksum(&self.buf[..message_len], 0) == 0
    }

    /// Computes and sets the IGMP checksum over the entire message.
    #[inline]
    pub fn compute_checksum(&mut self, message_len: usize) {
        self.set_checksum(0);
        let cksum = checksum::internet_checksum(&self.buf[..message_len], 0);
        self.set_checksum(cksum);
    }
}

impl fmt::Debug for IgmpHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IgmpHeader")
            .field("type", &self.igmp_type())
            .field("max_resp_time", &self.max_resp_time())
            .field("checksum", &self.checksum())
            .field("group_addr", &self.group_addr())
            .finish()
    }
}
