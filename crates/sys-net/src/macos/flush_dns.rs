use anyhow::Result;

pub async fn flush_system_dns_cache() -> Result<()> {
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
