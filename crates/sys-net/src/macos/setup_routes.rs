use anyhow::{Context, Result, bail};
use std::process::Command;

pub fn setup_routes(utun_name: &str, enable_ipv6: bool) -> Result<()> {
    for cidr in split_default_v4() {
        add_route(utun_name, cidr, false)
            .with_context(|| format!("failed to add IPv4 route {cidr} via {utun_name}"))?;
    }

    if enable_ipv6 {
        for cidr in split_default_v6() {
            add_route(utun_name, cidr, true)
                .with_context(|| format!("failed to add IPv6 route {cidr} via {utun_name}"))?;
        }
    }

    Ok(())
}

fn split_default_v4() -> &'static [&'static str] {
    &[
        "1.0.0.0/8",
        "2.0.0.0/7",
        "4.0.0.0/6",
        "8.0.0.0/5",
        "16.0.0.0/4",
        "32.0.0.0/3",
        "64.0.0.0/2",
        "128.0.0.0/1",
    ]
}

fn split_default_v6() -> &'static [&'static str] {
    &[
        "100::/8", "200::/7", "400::/6", "800::/5", "1000::/4", "2000::/3", "4000::/2", "8000::/1",
    ]
}

fn add_route(utun_name: &str, cidr: &str, ipv6: bool) -> Result<()> {
    let mut command = Command::new("route");
    command.arg("-q").arg("-n").arg("add");

    if ipv6 {
        command.arg("-inet6");
    }

    command.arg("-net").arg(cidr).arg("-interface").arg(utun_name);

    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    if stderr.contains("File exists") || stdout.contains("File exists") {
        return Ok(());
    }

    bail!(
        "route add failed for {} via {}: stdout=`{}` stderr=`{}`",
        cidr,
        utun_name,
        stdout.trim(),
        stderr.trim()
    )
}

