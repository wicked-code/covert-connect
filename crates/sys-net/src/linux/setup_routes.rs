use anyhow::{Context, Result, bail};
use tokio::process::Command;

pub async fn setup_routes(utun_name: &str, enable_ipv6: bool) -> Result<()> {
    for cidr in split_default_v4() {
        replace_route(utun_name, cidr, false)
            .await
            .with_context(|| format!("failed to configure IPv4 route {cidr} via {utun_name}"))?;
    }

    if enable_ipv6 {
        for cidr in split_default_v6() {
            replace_route(utun_name, cidr, true)
                .await
                .with_context(|| format!("failed to configure IPv6 route {cidr} via {utun_name}"))?;
        }
    }

    Ok(())
}

fn split_default_v4() -> &'static [&'static str] {
    &["0.0.0.0/1", "128.0.0.0/1"]
}

fn split_default_v6() -> &'static [&'static str] {
    &["::/1", "8000::/1"]
}

async fn replace_route(utun_name: &str, cidr: &str, ipv6: bool) -> Result<()> {
    let mut command = Command::new("ip");
    if ipv6 {
        command.arg("-6");
    }

    command
        .arg("route")
        .arg("replace")
        .arg(cidr)
        .arg("dev")
        .arg(utun_name)
        .arg("proto")
        .arg("static");

    let output = command.output().await?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    let family = if ipv6 { "IPv6" } else { "IPv4" };
    bail!(
        "ip route replace failed for {} cidr {} via {}: stdout=`{}` stderr=`{}`",
        family,
        cidr,
        utun_name,
        stdout.trim(),
        stderr.trim()
    );
}
