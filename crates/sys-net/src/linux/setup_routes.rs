use anyhow::{Context, Result, bail};
use tokio::process::Command;

pub async fn setup_routes(utun_name: &str, enable_ipv6: bool) -> Result<()> {
    let mut command = Command::new("ip");
    command.arg("route").arg("add").arg("default").arg("dev").arg(utun_name);

    if enable_ipv6 {
        command.arg("proto").arg("static").arg("metric").arg("512");
    }

    let output = command.output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    if stderr.contains("File exists") || stdout.contains("File exists") {
        return Ok(());
    }

    bail!("Failed to add default route via {utun_name}: {stderr} {stdout}");
}
