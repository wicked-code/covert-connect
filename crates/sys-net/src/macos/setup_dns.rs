use anyhow::{Result, anyhow, bail};

use std::{
    io::Write,
    net::IpAddr,
    process::{Command, Stdio},
};

pub fn setup_dns(utun_name: &str, dns_ip: IpAddr) -> Result<()> {
    let service_id = format!("CustomTun_{}", utun_name);
    let dns_key = format!("State:/Network/Service/{service_id}/DNS");

    let dns_script = format!(
        "d.init\n\
d.add ServerAddresses * {dns_ip}\n\
set {dns_key}\n\
quit\n",
        dns_ip = dns_ip,
        dns_key = dns_key,
    );
    run_scutil_script(&dns_script)?;

    // Bind the synthetic service to the utun interface so configd can expose it.
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
