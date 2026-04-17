use anyhow::{Result, bail, anyhow};

use std::{
    io::Write,
    net::IpAddr,
    process::{Command, Stdio},
};

pub fn setup_dns(utun_name: &str, dns_ip: IpAddr) -> Result<()> {
    let service_id = format!("CustomTun_{}", utun_name);

    let script = format!(
        "d.init\n\
         d.add ServerAddresses * {dns_ip}\n\
         d.add InterfaceName {utun_name}\n\
         set State:/Network/Service/{service_id}/DNS\n\
         quit\n",
        dns_ip = dns_ip,
        utun_name = utun_name,
        service_id = service_id
    );

    let mut child = Command::new("scutil")
        .stdin(Stdio::piped())
        .spawn()?;

    let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("Failed to open stdin"))?;
    stdin.write_all(script.as_bytes())?;

    let status = child.wait()?;
    if !status.success() {
        bail!("scutil failed to apply dns");
    }
    Ok(())
}
