use std::net::{IpAddr, Ipv6Addr};
use std::process::Command;

use anyhow::Result;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};

use crate::{DefaultIf, NoInterfaceFoundError, is_ipv6_global};

pub fn find_default_if() -> Result<DefaultIf> {
    let output = Command::new("ip").args(["route", "show", "default"]).output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);

    let mut default_if_name = None;
    for line in output_str.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 && parts[0] == "default" {
            if let Some(dev_idx) = parts.iter().position(|&p| p == "dev") {
                if dev_idx + 1 < parts.len() {
                    default_if_name = Some(parts[dev_idx + 1].to_string());
                    break;
                }
            }
        }
    }

    let default_if_name = default_if_name.ok_or_else(|| NoInterfaceFoundError)?;

    let system_interfaces = NetworkInterface::show()?;
    let itf = system_interfaces
        .into_iter()
        .find(|i| i.name == default_if_name)
        .ok_or_else(|| NoInterfaceFoundError)?;

    let ipv4 = itf
        .addr
        .iter()
        .find_map(|a| if a.ip().is_ipv4() { Some(a.ip()) } else { None })
        .ok_or_else(|| anyhow::anyhow!("No IPv4 address found on default interface"))?;

    let ipv6 = itf
        .addr
        .iter()
        .find_map(|a| match a.ip() {
            IpAddr::V6(ipv6) if is_ipv6_global(ipv6) => Some(a.ip()),
            _ => None,
        })
        .unwrap_or_else(|| IpAddr::V6(Ipv6Addr::UNSPECIFIED));

    let dns = get_dns_servers()?;

    Ok(DefaultIf {
        if_name: default_if_name,
        ipv4,
        ipv6,
        dns,
    })
}

fn get_dns_servers() -> Result<Vec<IpAddr>> {
    let content = std::fs::read_to_string("/etc/resolv.conf")?;
    let mut dns_servers = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("nameserver") {
            if let Some(ip_str) = line.split_whitespace().nth(1) {
                if let Ok(ip) = ip_str.parse::<IpAddr>() {
                    dns_servers.push(ip);
                }
            }
        }
    }
    Ok(dns_servers)
}
