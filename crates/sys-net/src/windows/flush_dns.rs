use anyhow::Result;

pub async fn flush_system_dns_cache() -> Result<()> {
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
