use anyhow::Result;
use tokio::process::Command;

pub async fn flush_system_dns_cache() -> Result<()> {
    match Command::new("resolvectl")
        .args(["flush-caches"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            tracing::debug!("DNS cache flushed successfully via resolvectl");
        }
        Ok(_) => {
            tracing::warn!("resolvectl flush-caches failed");
        }
        Err(e) => {
            tracing::warn!("Failed to execute resolvectl: {}", e);
        }
    }
    
    match Command::new("systemctl")
        .args(["restart", "systemd-resolved"])
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            tracing::debug!("systemd-resolved restarted successfully");
        }
        Ok(_) => {
            tracing::warn!("systemctl restart systemd-resolved failed");
        }
        Err(e) => {
            tracing::warn!("Failed to execute systemctl: {}", e);
        }
    }
    Ok(())
}
