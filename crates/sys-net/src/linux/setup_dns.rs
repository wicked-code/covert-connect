use anyhow::{Context, Result, bail};
use tokio::process::Command;

use std::net::IpAddr;

pub async fn setup_dns(interface: &str, dns_ip: IpAddr) -> Result<()> {
    if !Command::new("resolvectl")
        .args(&["dns", interface, &dns_ip.to_string()])
        .status()
        .await
        .with_context(|| "tun setup dns")?
        .success()
    {
        bail!("tun setup dns, resolvectl dns command failed");
    }

    // Route all traffic (~.) through this interface for DNS
    if !Command::new("resolvectl")
        .args(&["domain", interface, "~."])
        .status()
        .await
        .with_context(|| "tun setup dns")?
        .success()
    {
        bail!("tun setup dns, resolvectl domain command failed");
    }

    // not so important if it fails
    match Command::new("resolvectl").args(&["flush-caches"]).status().await {
        Ok(status) if status.success() => {}
        Ok(_) => {
            tracing::warn!("resolvectl flush-caches failed");
        }
        Err(e) => {
            tracing::warn!("Failed to execute resolvectl: {}", e);
        }
    }

    Ok(())
}
