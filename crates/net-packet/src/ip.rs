use std::net::{IpAddr, SocketAddr};

use crate::{
    hopbyhop::HopByHopHeader,
    icmpv4::Icmpv4Header,
    icmpv6::Icmpv6Header,
    igmp::IgmpHeader,
    ip_protocols,
    ipv4::{IPV4_MIN_HEADER_LEN, Ipv4Header},
    ipv6::{IPV6_HEADER_LEN, Ipv6Header},
    tcp::TcpHeader,
    udp::{UdpHeader, UDP_HEADER_LEN},
};
use anyhow::{Result, bail};

const DEFAULT_TTL: u8 = 64;

pub enum IpHeader<'a> {
    V4(Ipv4Header<'a>),
    V6(Ipv6Header<'a>),
}

pub enum NextHeader<'a> {
    Tcp(TcpHeader<'a>),
    Udp(UdpHeader<'a>),
    Icmpv4(Icmpv4Header<'a>),
    Icmpv6(Icmpv6Header<'a>),
    Igmp(IgmpHeader<'a>),
}

pub struct IpPacket<'a> {
    pub header: IpHeader<'a>,
    pub next_header: NextHeader<'a>,
}

/// Parses transport-layer header from payload given the IP protocol number.
fn parse_transport<'a>(protocol: u8, payload: &'a mut [u8]) -> Result<NextHeader<'a>> {
    Ok(match protocol {
        ip_protocols::TCP => NextHeader::Tcp(TcpHeader::new(payload)?),
        ip_protocols::UDP => NextHeader::Udp(UdpHeader::new(payload)?),
        ip_protocols::ICMP => NextHeader::Icmpv4(Icmpv4Header::new(payload)?),
        ip_protocols::ICMPV6 => NextHeader::Icmpv6(Icmpv6Header::new(payload)?),
        ip_protocols::IGMP => NextHeader::Igmp(IgmpHeader::new(payload)?),
        ip_protocols::HOPBYHOP => {
            // TODO: just skip it for now
            let (header, payload) = HopByHopHeader::new(payload)?.split();
            return parse_transport(header.next_header(), payload);
        }
        _ => bail!("Unsupported IP protocol {}", protocol),
    })
}

impl<'a> IpPacket<'a> {
    /// Builds a new IP packet and returns the completed buffer.
    /// Use `IpPacket::from(&mut buf)` afterwards if you need to modify fields.
    #[inline]
    pub fn build(protocol: u8, src_addr: SocketAddr, dst_addr: SocketAddr, payload: &[u8]) -> Result<Vec<u8>> {
        if src_addr.is_ipv4() != dst_addr.is_ipv4() {
            bail!("Source and destination IP version mismatch.");
        }

        let mut packet = Vec::with_capacity(payload.len() + 48);
        let is_v4 = src_addr.is_ipv4();

        // Write IP header
        if is_v4 {
            packet.resize(IPV4_MIN_HEADER_LEN, 0);
            let mut header = Ipv4Header::new(&mut packet)?;
            header.set_version(4);
            header.set_ihl((IPV4_MIN_HEADER_LEN / 4) as u8);
            if let IpAddr::V4(addr) = src_addr.ip() { header.set_src_addr(addr); }
            if let IpAddr::V4(addr) = dst_addr.ip() { header.set_dst_addr(addr); }
            header.set_ttl(DEFAULT_TTL);
            header.set_protocol(protocol);
        } else {
            packet.resize(IPV6_HEADER_LEN, 0);
            let mut header = Ipv6Header::new(&mut packet)?;
            header.set_version(6);
            header.set_hop_limit(DEFAULT_TTL);
            if let IpAddr::V6(addr) = src_addr.ip() { header.set_src_addr(addr); }
            if let IpAddr::V6(addr) = dst_addr.ip() { header.set_dst_addr(addr); }
            header.set_next_header(protocol);
        }

        // Write transport header + payload
        let ip_len = packet.len();
        match protocol {
            ip_protocols::UDP => {
                packet.resize(ip_len + UDP_HEADER_LEN, 0);
                packet.extend_from_slice(payload);
                let datagram_len = (UDP_HEADER_LEN + payload.len()) as u16;
                let mut udp = UdpHeader::new(&mut packet[ip_len..])?;
                udp.set_src_port(src_addr.port());
                udp.set_dst_port(dst_addr.port());
                udp.set_length(datagram_len);
                if is_v4 {
                    if let (IpAddr::V4(src), IpAddr::V4(dst)) = (src_addr.ip(), dst_addr.ip()) {
                        udp.compute_checksum_v4(src, dst, datagram_len);
                    }
                } else {
                    if let (IpAddr::V6(src), IpAddr::V6(dst)) = (src_addr.ip(), dst_addr.ip()) {
                        udp.compute_checksum_v6(src, dst, datagram_len as u32);
                    }
                }
            }
            _ => bail!("Unsupported IP protocol to create {}", protocol),
        }

        // Finalize IP header (total length + checksum)
        let total_len = packet.len();
        if is_v4 {
            let mut header = Ipv4Header::new(&mut packet)?;
            header.set_total_length(total_len as u16);
            header.compute_checksum();
        } else {
            let mut header = Ipv6Header::new(&mut packet)?;
            header.set_payload_length((total_len - IPV6_HEADER_LEN) as u16);
        }

        Ok(packet)
    }

    /// Creates a new `IpPacket` by parsing an existing buffer.
    #[inline]
    pub fn try_from(buf: &'a mut [u8]) -> Result<Self> {
        if buf.len() < IPV4_MIN_HEADER_LEN {
            bail!("Slice too short for IP header.");
        }
        let version = buf[0] >> 4;
        let (protocol, header, payload) = match version {
            4 => {
                let (header, payload) = Ipv4Header::new(buf)?.split();
                (header.protocol(), IpHeader::V4(header), payload)
            }
            6 => {
                let (header, payload) = Ipv6Header::new(buf)?.split();
                (header.next_header(), IpHeader::V6(header), payload)
            }
            _ => bail!("Unsupported IP version."),
        };
        Ok(Self {
            header,
            next_header: parse_transport(protocol, payload)?,
        })
    }
}
