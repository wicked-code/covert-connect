use anyhow::Result;
use tokio::process::Command;

pub async fn flush_system_dns_cache() -> Result<()> {
    let output = Command::new("dscacheutil").args(["-flushcache"]).output().await;

    match output {
        Ok(output) if output.status.success() => {
            tracing::debug!("DNS cache flushed successfully via dscacheutil");
        }
        _ => {
            let _ = Command::new("killall").args(["-HUP", "mDNSResponder"]).output().await;
        }
    }

    Ok(())
}
