use anyhow::{Result, anyhow, bail};
use tokio::{io::AsyncWriteExt, process::Command};

use std::{collections::BTreeSet, net::IpAddr, process::Stdio};

const SERVICE_ID_PREFIX: &str = "CCTun_";

/// Lower DNS order = higher resolver priority. System default is ~200000.
const DNS_ORDER: u32 = 5000;
const DNS_MATCH_ALL_DOMAINS: &str = "";

pub async fn setup_dns(utun_name: &str, dns_ip: IpAddr) -> Result<()> {
    let service_id = format!("{SERVICE_ID_PREFIX}{utun_name}");
    let dns_key = format!("State:/Network/Service/{service_id}/DNS");

    let dns_script = format!(
        "d.init\n\
d.add ServerAddresses * {dns_ip}\n\
d.add SearchOrder {order}\n\
d.add SupplementalMatchDomains * \"{match_domain}\"\n\
d.add SupplementalMatchOrders * {order}\n\
set {dns_key}\n\
quit\n",
        dns_ip = dns_ip,
        order = DNS_ORDER,
        match_domain = DNS_MATCH_ALL_DOMAINS,
        dns_key = dns_key,
    );
    run_scutil_script(&dns_script).await?;

    let if_state_script = match dns_ip {
        IpAddr::V4(ipv4) => format!(
            "d.init\n\
d.add Addresses * {ipv4}\n\
d.add DestAddresses * {ipv4}\n\
d.add InterfaceName {utun_name}\n\
d.add SubnetMasks * 255.255.255.255\n\
set State:/Network/Service/{service_id}/IPv4\n\
quit\n",
            ipv4 = ipv4,
            utun_name = utun_name,
            service_id = service_id,
        ),
        IpAddr::V6(ipv6) => format!(
            "d.init\n\
d.add Addresses * {ipv6}\n\
d.add DestAddresses * {ipv6}\n\
d.add InterfaceName {utun_name}\n\
d.add PrefixLength 128\n\
set State:/Network/Service/{service_id}/IPv6\n\
quit\n",
            ipv6 = ipv6,
            utun_name = utun_name,
            service_id = service_id,
        ),
    };
    run_scutil_script(&if_state_script).await?;

    let verify_script = format!("show {dns_key}\nquit\n", dns_key = dns_key);
    let verify_output = run_scutil_script(&verify_script).await?;
    for required_key in [
        "ServerAddresses",
        "SearchOrder",
        "SupplementalMatchDomains",
        "SupplementalMatchOrders",
    ] {
        if verify_output.contains(required_key) {
            continue;
        }

        bail!(
            "DNS key exists but has no {required_key} (key={dns_key}). scutil output:\n{verify_output}",
            required_key = required_key,
            dns_key = dns_key,
            verify_output = verify_output
        );
    }

    Ok(())
}

pub async fn teardown_dns() {
    match list_service_ids().await {
        Ok(service_ids) => {
            for service_id in service_ids {
                remove_service_resolver(&service_id).await;
            }
        }
        Err(err) => tracing::error!("failed to list scutil DNS resolvers for cleanup: {:?}", err),
    }
}

async fn list_service_ids() -> Result<BTreeSet<String>> {
    let mut service_ids = BTreeSet::new();

    let prefix = SERVICE_ID_PREFIX;
    for suffix in ["DNS", "IPv4", "IPv6"] {
        let script = format!("list State:/Network/Service/{prefix}.*/{suffix}\nquit\n");
        let output = run_scutil_script(&script).await?;

        for line in output.lines() {
            if let Some(service_id) = parse_service_id_from_key(line) {
                service_ids.insert(service_id.to_string());
            }
        }
    }

    Ok(service_ids)
}

fn parse_service_id_from_key(line: &str) -> Option<&str> {
    let key_prefix = "State:/Network/Service/";
    let start = line.find(key_prefix)? + key_prefix.len();
    let rest = &line[start..];
    let end = rest.find('/')?;
    let service_id = &rest[..end];

    if service_id.starts_with(SERVICE_ID_PREFIX) {
        Some(service_id)
    } else {
        None
    }
}

async fn remove_service_resolver(service_id: &str) {
    let script = format!(
        "remove State:/Network/Service/{service_id}/DNS\n\
remove State:/Network/Service/{service_id}/IPv4\n\
remove State:/Network/Service/{service_id}/IPv6\n\
quit\n",
        service_id = service_id,
    );
    if let Err(err) = run_scutil_script(&script).await {
        tracing::error!("Failed to remove DNS resolver for service {}: {:?}", service_id, err);
    }
}

async fn run_scutil_script(script: &str) -> Result<String> {
    let mut child = Command::new("scutil")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("Failed to open stdin"))?;
    stdin.write_all(script.as_bytes()).await?;
    child.stdin.take();

    let output = child.wait_with_output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        bail!(
            "scutil failed (status={:?})\nscript:\n{}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            script,
            stdout,
            stderr
        );
    }

    if stderr.contains("No such key") || stderr.contains("not found") || stderr.contains("invalid") {
        bail!("scutil reported an error\nscript:\n{}\nstderr:\n{}", script, stderr);
    }

    Ok(stdout)
}
