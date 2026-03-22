pub mod checksum;
pub mod icmpv4;
pub mod icmpv6;
pub mod ip;
pub mod ipv4;
pub mod ipv6;
pub mod tcp;
pub mod udp;

/// Well-known IP protocol numbers.
pub mod ip_protocols {
    pub const ICMP: u8 = 1;
    pub const TCP: u8 = 6;
    pub const UDP: u8 = 17;
    pub const ICMPV6: u8 = 58;
}
