use std::process::Command;

use anyhow::Result;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};

use crate::DefaultIf;

pub fn find_default_if() -> Result<DefaultIf> {
    let hardware_ports_output = Command::new("networksetup")
        .args(["-listallhardwareports"])
        .output()?;
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
            let ipv4 = itf.addr.iter().find_map(|a| if a.ip().is_ipv4() { Some(a.ip()) } else { None });
            match ipv4 {
                Some(ipv4) => {
                    return Ok(DefaultIf {
                        ipv4,
                        ipv6: itf
                            .addr
                            .iter()
                            .find_map(|a| if a.ip().is_ipv6() { Some(a.ip()) } else { None })
                            .unwrap_or_else(|| std::net::IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED)),
                        dns: get_dns_servers(&device_name).unwrap_or_default(),
                    });
                }
                _ => {}
            }
        }
    }

    anyhow::bail!("No default interface found")
}

fn get_dns_servers(device_name: &str) -> Result<Vec<std::net::IpAddr>> {
    let output = Command::new("networksetup")
        .args(["getoption", device_name, "domain_name_server"])
        .output()?;
    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut dns_servers = Vec::new();
    for line in output_str.lines() {
        if let Ok(ip) = line.trim().parse() {
            dns_servers.push(ip);
        }
    }
    Ok(dns_servers)
}
