use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use anyhow::Result;

use crate::DefaultIf;

pub fn find_default_if() -> Result<DefaultIf> {
    Ok(DefaultIf {
        ipv4: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        ipv6: IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        dns: Vec::new(),
    })
}

pub async fn flush_system_dns_cache() -> Result<()> {
    Ok(())
}

pub async fn setup_dns(utun_name: &str, dns_ip: IpAddr) -> Result<()> {
    Ok(())
}