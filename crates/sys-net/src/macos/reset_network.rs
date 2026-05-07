use std::process::Command;

const PFCTL_STATUS_DISABLED: &str = "Status: Disabled";

pub fn reset_network() {
    let result = Command::new("pfctl").arg("-s").arg("info").output();
    let output = match result {
        Ok(ref output) => String::from_utf8_lossy(&output.stdout),
        Err(_) => PFCTL_STATUS_DISABLED.into(),
    };
    let was_disabled = output.contains(PFCTL_STATUS_DISABLED);

    if let Err(err) = Command::new("pfctl").arg("-E").output() {
        tracing::warn!("Error enabling pfctl: {}", err);
    }

    if let Err(err) = Command::new("pfctl").arg("-F").arg("state").output() {
        tracing::warn!("Error flushing pfctl state: {}", err);
    }

    if was_disabled {
        if let Err(err) = Command::new("pfctl").arg("-d").output() {
            tracing::warn!("Error reset pfctl to original state: {}", err);
        }
    }
}
