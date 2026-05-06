use anyhow::{anyhow, bail, Result};

use std::{
    collections::BTreeSet,
    io::Write,
    net::IpAddr,
    process::{Command, Stdio},
};

const SERVICE_ID_PREFIX: &str = "CCTun_";
const CLEANUP_SERVICE_ID_PREFIXES: &[&str] = &[SERVICE_ID_PREFIX, "CustomTun_"];

/// Lower SearchOrder = higher resolver priority. System default is ~200000.
const DNS_SEARCH_ORDER: u32 = 5000;

pub fn setup_dns(utun_name: &str, dns_ip: IpAddr) -> Result<()> {
    let service_id = format!("{SERVICE_ID_PREFIX}{utun_name}");
    let dns_key = format!("State:/Network/Service/{service_id}/DNS");

    let dns_script = format!(
        "d.init\n\
d.add ServerAddresses * {dns_ip}\n\
d.add SearchOrder {order}\n\
set {dns_key}\n\
quit\n",
        dns_ip = dns_ip,
        order = DNS_SEARCH_ORDER,
        dns_key = dns_key,
    );
    run_scutil_script(&dns_script)?;

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
    run_scutil_script(&if_state_script)?;

    let verify_script = format!("show {dns_key}\nquit\n", dns_key = dns_key);
    let verify_output = run_scutil_script(&verify_script)?;
    if !verify_output.contains("ServerAddresses") {
        bail!(
            "DNS key exists but has no ServerAddresses (key={dns_key}). scutil output:\n{verify_output}",
            dns_key = dns_key,
            verify_output = verify_output
        );
    }

    Ok(())
}

pub fn teardown_dns() {
    match list_service_ids() {
        Ok(service_ids) => {
            for service_id in service_ids {
                remove_service_resolver(&service_id);
            }
        }
        Err(err) => tracing::warn!("failed to list scutil DNS resolvers for cleanup: {:?}", err),
    }
}

fn list_service_ids() -> Result<BTreeSet<String>> {
    let mut service_ids = BTreeSet::new();

    for prefix in CLEANUP_SERVICE_ID_PREFIXES {
        for suffix in ["DNS", "IPv4", "IPv6"] {
            let script = format!("list State:/Network/Service/{prefix}.*/{suffix}\nquit\n");
            let output = run_scutil_script(&script)?;

            for line in output.lines() {
                if let Some(service_id) = parse_service_id_from_key(line) {
                    service_ids.insert(service_id.to_string());
                }
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

    if CLEANUP_SERVICE_ID_PREFIXES
        .iter()
        .any(|prefix| service_id.starts_with(prefix))
    {
        Some(service_id)
    } else {
        None
    }
}

fn remove_service_resolver(service_id: &str) {
    let script = format!(
        "remove State:/Network/Service/{service_id}/DNS\n\
remove State:/Network/Service/{service_id}/IPv4\n\
remove State:/Network/Service/{service_id}/IPv6\n\
quit\n",
        service_id = service_id,
    );
    let _ = run_scutil_script(&script);
}

fn run_scutil_script(script: &str) -> Result<String> {
    let mut child = Command::new("scutil")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("Failed to open stdin"))?;
    stdin.write_all(script.as_bytes())?;
    child.stdin.take();

    let output = child.wait_with_output()?;
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
