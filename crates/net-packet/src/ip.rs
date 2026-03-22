use anyhow::{Result, bail};
use crate::{
    icmpv4::Icmpv4Header,
    icmpv6::Icmpv6Header,
    ip_protocols,
    ipv4::{IPV4_MIN_HEADER_LEN, Ipv4Header},
    ipv6::Ipv6Header,
    tcp::TcpHeader,
    udp::UdpHeader,
};

pub enum IpHeader<'a> {
    V4(Ipv4Header<'a>),
    V6(Ipv6Header<'a>),
}

pub enum NextHeader<'a> {
    Tcp(TcpHeader<'a>),
    Udp(UdpHeader<'a>),
    Icmpv4(Icmpv4Header<'a>),
    Icmpv6(Icmpv6Header<'a>),
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
        _ => bail!("Unsupported IP protocol {}", protocol),
    })
}

impl<'a> IpPacket<'a> {
    /// Creates a new `IpPacket` by parsing an existing buffer.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> Result<Self> {
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
