use anyhow::Result;
use std::net::IpAddr;

pub async fn flush_system_dns_cache() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        return flush_dns_windows().await;
    }

    #[cfg(target_os = "macos")]
    {
        return flush_dns_macos().await;
    }

    #[cfg(target_os = "linux")]
    {
        return flush_dns_linux().await;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        tracing::warn!("DNS cache flush not implemented for this platform");
        Ok(())
    }
}

pub async fn get_dns_by_if_addr(if_addr: IpAddr) -> Result<Vec<IpAddr>> {
    #[cfg(target_os = "windows")]
    {
        return get_dns_windows(if_addr).await;
    }

    #[cfg(target_os = "macos")]
    {
        return get_dns_macos(if_addr).await;
    }

    #[cfg(target_os = "linux")]
    {
        return get_dns_linux(if_addr).await;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        tracing::warn!("DNS lookup not implemented for this platform");
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
async fn flush_dns_windows() -> Result<()> {
    let output = tokio::process::Command::new("ipconfig")
        .args(["/flushdns"])
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            tracing::debug!("DNS cache flushed successfully via ipconfig");
        }
        Ok(_) => {
            tracing::warn!("ipconfig /flushdns failed");
        }
        Err(e) => {
            tracing::warn!("Failed to execute ipconfig: {}", e);
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
async fn get_dns_windows(if_addr: IpAddr) -> Result<Vec<IpAddr>> {
    let ps = format!(
        "(Get-NetIPAddress -IPAddress '{}' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty InterfaceIndex)",
        if_addr
    );

    let idx_out = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .output()
        .await;

    let interface_index = match idx_out {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).trim().parse::<u32>().ok(),
        _ => None,
    };

    let Some(interface_index) = interface_index else {
        return Ok(Vec::new());
    };

    let ps = format!(
        "Get-DnsClientServerAddress -InterfaceIndex {} -AddressFamily IPv4 | Select-Object -ExpandProperty ServerAddresses",
        interface_index
    );

    let output = tokio::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .output()
        .await;

    let mut dns_servers = Vec::new();

    if let Ok(output) = output {
        if output.status.success() {
            for token in String::from_utf8_lossy(&output.stdout).lines().map(str::trim) {
                if let Ok(ip) = token.parse::<IpAddr>() {
                    if !dns_servers.contains(&ip) {
                        dns_servers.push(ip);
                    }
                }
            }
        }
    }

    Ok(dns_servers)
}

#[cfg(target_os = "macos")]
async fn flush_dns_macos() -> Result<()> {
    let output = tokio::process::Command::new("dscacheutil")
        .args(["-flushcache"])
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            tracing::debug!("DNS cache flushed successfully via dscacheutil");
        }
        _ => {
            let _ = tokio::process::Command::new("killall")
                .args(["-HUP", "mDNSResponder"])
                .output()
                .await;
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
async fn get_dns_macos(_if_addr: IpAddr) -> Result<Vec<IpAddr>> {
    parse_resolv_conf().or_else(|_| Ok(Vec::new()))
}

#[cfg(target_os = "linux")]
async fn flush_dns_linux() -> Result<()> {
    let _ = tokio::process::Command::new("resolvectl")
        .args(["flush-caches"])
        .output()
        .await;
    let _ = tokio::process::Command::new("systemctl")
        .args(["restart", "systemd-resolved"])
        .output()
        .await;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn get_dns_linux(if_addr: IpAddr) -> Result<Vec<IpAddr>> {
    let output = tokio::process::Command::new("ip")
        .args(["-o", "addr", "show"])
        .output()
        .await;

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let if_addr_s = if_addr.to_string();
            let mut if_name: Option<String> = None;

            for line in stdout.lines() {
                if line.contains(&if_addr_s) {
                    let mut parts = line.split_whitespace();
                    let _idx = parts.next();
                    if let Some(name) = parts.next() {
                        if_name = Some(name.trim_end_matches(':').to_string());
                        break;
                    }
                }
            }

            if let Some(if_name) = if_name {
                let output = tokio::process::Command::new("resolvectl")
                    .args(["dns", &if_name])
                    .output()
                    .await;

                if let Ok(output) = output {
                    if output.status.success() {
                        let mut dns_servers = Vec::new();
                        for token in String::from_utf8_lossy(&output.stdout).split_whitespace() {
                            if let Ok(ip) = token.parse::<IpAddr>() {
                                if !dns_servers.contains(&ip) {
                                    dns_servers.push(ip);
                                }
                            }
                        }
                        if !dns_servers.is_empty() {
                            return Ok(dns_servers);
                        }
                    }
                }
            }
        }
    }

    parse_resolv_conf().or_else(|_| Ok(Vec::new()))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn parse_resolv_conf() -> Result<Vec<IpAddr>> {
    let contents = std::fs::read_to_string("/etc/resolv.conf")?;
    let mut dns_servers = Vec::new();

    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("nameserver") {
            if let Some(token) = rest.split_whitespace().next() {
                if let Ok(ip) = token.parse::<IpAddr>() {
                    dns_servers.push(ip);
                }
            }
        }
    }

    Ok(dns_servers)
}
