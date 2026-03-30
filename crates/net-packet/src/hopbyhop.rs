//! IPv6 Hop-by-Hop Options Header (RFC 8200, Section 4.3)
//!
//! ```text
//!  0                   1                   2                   3
//!  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |  Next Header  |  Hdr Ext Len  |                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+                               +
//! |                                                               |
//! .                            Options                            .
//! |                                                               |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```
//!
//! Hdr Ext Len is the length of the header in 8-octet units, not
//! including the first 8 octets.

use core::fmt;
use anyhow::{Result, bail};

/// Minimum Hop-by-Hop Options header length in bytes (Next Header + Hdr Ext Len + 6 padding).
pub const HOPBYHOP_MIN_HEADER_LEN: usize = 8;

/// Zero-copy IPv6 Hop-by-Hop Options header backed by a mutable byte slice.
pub struct HopByHopHeader<'a> {
    buf: &'a mut [u8],
}

impl<'a> HopByHopHeader<'a> {
    /// Creates a new `HopByHopHeader`.
    ///
    /// Validates that the buffer is large enough for the header length
    /// indicated by the Hdr Ext Len field.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < HOPBYHOP_MIN_HEADER_LEN {
            bail!("Slice too short for Hop-by-Hop header.");
        }
        let total = Self::total_len_from_buf(buf);
        if buf.len() < total {
            bail!(
                "Slice too short for Hop-by-Hop header: need {} bytes, got {}.",
                total,
                buf.len()
            );
        }
        Ok(Self { buf })
    }

    /// Computes total header length from the raw buffer (before constructing Self).
    #[inline]
    fn total_len_from_buf(buf: &[u8]) -> usize {
        (buf[1] as usize + 1) * 8
    }

    /// Returns the Next Header field (protocol number of the following header).
    #[inline]
    pub fn next_header(&self) -> u8 {
        self.buf[0]
    }

    /// Returns the Hdr Ext Len field (in 8-octet units, excluding the first 8 octets).
    #[inline]
    pub fn hdr_ext_len(&self) -> u8 {
        self.buf[1]
    }

    /// Returns the total header length in bytes: `(hdr_ext_len + 1) * 8`.
    #[inline]
    pub fn header_len(&self) -> usize {
        (self.buf[1] as usize + 1) * 8
    }

    /// Returns the header bytes.
    #[inline]
    pub fn header(&self) -> &[u8] {
        &self.buf[..self.header_len()]
    }

    /// Returns the raw options bytes (everything after Next Header and Hdr Ext Len).
    #[inline]
    pub fn options(&self) -> &[u8] {
        &self.buf[2..self.header_len()]
    }

    /// Sets the Next Header field.
    #[inline]
    pub fn set_next_header(&mut self, nh: u8) {
        self.buf[0] = nh;
    }

    /// Consumes the header and splits the buffer into the Hop-by-Hop header
    /// slice and the remaining payload.
    #[inline]
    pub fn split(self) -> (Self, &'a mut [u8]) {
        let len = self.header_len();
        let (header, payload) = self.buf.split_at_mut(len);
        (Self { buf: header }, payload)
    }
}

impl fmt::Debug for HopByHopHeader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HopByHopHeader")
            .field("next_header", &self.next_header())
            .field("hdr_ext_len", &self.hdr_ext_len())
            .field("header_len", &self.header_len())
            .finish()
    }
}
