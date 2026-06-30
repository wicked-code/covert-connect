use std::net::{IpAddr, Ipv6Addr};
use std::process::Command;

use anyhow::Result;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};

use crate::{DefaultIf, NoInterfaceFoundError, is_ipv6_global};

pub fn find_default_if() -> Result<DefaultIf> {
    let hardware_ports_output = Command::new("networksetup").args(["-listallhardwareports"]).output()?;
    let hardware_ports_str = String::from_utf8_lossy(&hardware_ports_output.stdout);

    let mut physical_devices = Vec::new();
    for line in hardware_ports_str.lines() {
        if line.contains("Device:") {
            if let Some(device) = line.split_whitespace().last() {
                physical_devices.push(device.to_string());
            }
        }
    }

    let service_order_output = Command::new("networksetup")
        .args(["-listnetworkserviceorder"])
        .output()?;
    let service_order_str = String::from_utf8_lossy(&service_order_output.stdout);

    let mut ordered_physical_devices = Vec::new();
    for line in service_order_str.lines() {
        if line.contains("Device:") {
            if let Some(device_part) = line.split("Device:").last() {
                let device = device_part.trim_matches(|c| c == ' ' || c == ')').to_string();
                if physical_devices.contains(&device) {
                    ordered_physical_devices.push(device);
                }
            }
        }
    }

    let system_interfaces = NetworkInterface::show()?;
    for device_name in ordered_physical_devices {
        if let Some(itf) = system_interfaces.iter().find(|i| i.name == device_name) {
            let ipv4 = itf
                .addr
                .iter()
                .find_map(|a| if a.ip().is_ipv4() { Some(a.ip()) } else { None });
            match ipv4 {
                Some(ipv4) => {
                    return Ok(DefaultIf {
                        if_index: itf.index,
                        ipv4,
                        ipv6: itf
                            .addr
                            .iter()
                            .find_map(|a| match a.ip() {
                                IpAddr::V6(ipv6) if is_ipv6_global(ipv6) => Some(a.ip()),
                                _ => None,
                            })
                            .unwrap_or_else(|| IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
                        dns: get_dns_servers(&device_name).unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }
    }

    Err(NoInterfaceFoundError.into())
}

fn get_dns_servers(device_name: &str) -> Result<Vec<IpAddr>> {
    let output = Command::new("scutil").args(["--dns"]).output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);

    let scoped_header = "DNS configuration (for scoped queries)";
    let scoped_section = if let Some(idx) = output_str.find(scoped_header) {
        &output_str[idx + scoped_header.len()..]
    } else {
        &output_str
    };

    let mut dns_servers = Vec::new();
    let mut curr_dns: Option<IpAddr> = None;
    let mut curr_if_name: Option<String> = None;

    for line in scoped_section.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("resolver") {
            curr_dns = None;
            curr_if_name = None;
            continue;
        }

        if line.starts_with("if_index") {
            if let Some(start) = line.rfind('(') {
                if let Some(end) = line.rfind(')') {
                    if start < end {
                        let iface = line[start + 1..end].trim();
                        curr_if_name = Some(iface.to_string());
                    }
                }
            }
            continue;
        }

        if line.starts_with("nameserver") {
            if let Some((_, ip_str)) = line.split_once(':') {
                if let Ok(ip) = ip_str.trim().parse() {
                    curr_dns = Some(ip);
                }
            }
        }

        if let Some(dns) = curr_dns
            && curr_if_name.as_deref() == Some(device_name)
        {
            dns_servers.push(dns);

            curr_dns = None;
            curr_if_name = None;
        }
    }

    Ok(dns_servers)
}
