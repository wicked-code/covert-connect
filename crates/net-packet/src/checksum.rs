/// Computes the Internet checksum (RFC 1071) over the given bytes.
///
/// Used by IPv4, TCP, UDP, and ICMP for header/data integrity.
/// `initial_sum` allows pre-seeding with a pseudo-header contribution
#[inline]
pub fn internet_checksum(data: &[u8], initial_sum: u32) -> u16 {
    let mut sum = initial_sum;
    let mut i = 0;
    let len = data.len();

    while i + 1 < len {
        sum += ((data[i] as u32) << 8) | (data[i + 1] as u32);
        i += 2;
    }

    // If odd number of bytes, pad with zero.
    if i < len {
        sum += (data[i] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }

    !sum as u16
}

use core::net::{Ipv4Addr, Ipv6Addr};

/// Computes the IPv4 pseudo-header checksum contribution for TCP/UDP.
///
/// The pseudo-header consists of:
/// - Source IP (4 bytes)
/// - Destination IP (4 bytes)
/// - Zero byte + Protocol (2 bytes)
/// - Transport length (2 bytes)
#[inline]
pub fn ipv4_pseudo_header_sum(src: Ipv4Addr, dst: Ipv4Addr, protocol: u8, transport_len: u16) -> u32 {
    let s = src.octets();
    let d = dst.octets();
    let mut sum: u32 = 0;
    sum += ((s[0] as u32) << 8) | (s[1] as u32);
    sum += ((s[2] as u32) << 8) | (s[3] as u32);
    sum += ((d[0] as u32) << 8) | (d[1] as u32);
    sum += ((d[2] as u32) << 8) | (d[3] as u32);
    sum += protocol as u32;
    sum += transport_len as u32;
    sum
}

/// Computes the IPv6 pseudo-header checksum contribution for TCP/UDP.
///
/// The pseudo-header consists of:
/// - Source IP (16 bytes)
/// - Destination IP (16 bytes)
/// - Upper-layer packet length (4 bytes)
/// - Zero (3 bytes) + Next Header (1 byte)
#[inline]
pub fn ipv6_pseudo_header_sum(src: Ipv6Addr, dst: Ipv6Addr, next_header: u8, transport_len: u32) -> u32 {
    let s = src.octets();
    let d = dst.octets();
    let mut sum: u32 = 0;

    let mut i = 0;
    while i < 16 {
        sum += ((s[i] as u32) << 8) | (s[i + 1] as u32);
        i += 2;
    }
    i = 0;
    while i < 16 {
        sum += ((d[i] as u32) << 8) | (d[i + 1] as u32);
        i += 2;
    }

    sum += (transport_len >> 16) & 0xFFFF;
    sum += transport_len & 0xFFFF;
    sum += next_header as u32;

    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_internet_checksum_simple() {
        // Example from RFC 1071: the 16-bit words 0x0001, 0xF203, 0xF4F5, 0xF6F7
        let data = [0x00, 0x01, 0xF2, 0x03, 0xF4, 0xF5, 0xF6, 0xF7];
        let cksum = internet_checksum(&data, 0);
        assert_eq!(cksum, 0x220D);
    }

    #[test]
    fn test_internet_checksum_odd_length() {
        let data = [0x00, 0x01, 0x02];
        let cksum = internet_checksum(&data, 0);
        // Sum: 0x0001 + 0x0200 = 0x0201, complement = 0xFDFE
        assert_eq!(cksum, 0xFDFE);
    }

    #[test]
    fn test_checksum_zero_data() {
        let data = [0u8; 20];
        let cksum = internet_checksum(&data, 0);
        assert_eq!(cksum, 0xFFFF);
    }
}
