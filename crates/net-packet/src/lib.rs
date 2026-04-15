pub mod checksum;
pub mod hopbyhop;
pub mod icmpv4;
pub mod icmpv6;
pub mod igmp;
pub mod ip;
pub mod ipv4;
pub mod ipv6;
pub mod tcp;
pub mod udp;

pub const MAX_PACKET_SIZE: usize = 0xFFFF; // max IP packet size
pub const MTU_DEFAULT: usize = 1500; // default MTU for Ethernet
pub const IP_BUFFER_SIZE: usize = 2000;

/// Well-known IP protocol numbers.
pub mod ip_protocols {
    pub const HOPBYHOP: u8 = 0;
    pub const ICMP: u8 = 1;
    pub const IGMP: u8 = 2;
    pub const TCP: u8 = 6;
    pub const UDP: u8 = 17;
    pub const ICMPV6: u8 = 58;
}
