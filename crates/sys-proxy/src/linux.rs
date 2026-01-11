use anyhow::{Result, anyhow};
use std::{io, process::{Command, ExitStatus}};

const GNOME_KEY: &str = "org.gnome.system.proxy";

pub struct SystemProxy {
}

impl SystemProxy {
    pub fn new() -> Result<SystemProxy> {
        Ok(SystemProxy {})
    }

    pub fn set_pac(&self, auto_config_url: &str) -> Result<()> {

        gsettings()
            .args(["set", GNOME_KEY, "autoconfig-url", auto_config_url])
            .status()
            .exit_ok("proxy set autoconfig-url")?;

        gsettings()
            .args(["set", GNOME_KEY, "mode", "auto"])
            .status()
            .exit_ok("proxy set mode auto")
    }

    pub fn set_proxy(&self, address: &str) -> Result<()> {
        let mut parts = address.split(&['/', ':']).rev();
        let port = parts.next().ok_or_else(|| anyhow!("set_proxy: no port"))?;
        let host = parts.next().ok_or_else(|| anyhow!("set_proxy: no host"))?;
        if parts.next().is_none() {
            anyhow::bail!("set_proxy: invalid address");
        }

        let schema = GNOME_KEY.to_owned() + ".http";

        gsettings()
            .args(["set", &schema, "host", host])
            .status()
            .exit_ok("proxy set host")?;

        gsettings()
            .args(["set", &schema, "port", port])
            .status()
            .exit_ok("proxy set por")?;
        
        gsettings()
            .args(["set", GNOME_KEY, "mode", "manual"])
            .status()
            .exit_ok("proxy set mode")
    }

    pub fn restore(&self) -> Result<()> {
        gsettings()
            .args(["set", GNOME_KEY, "mode", "none"])
            .status()
            .exit_ok("proxy set mode none")
    }
}

fn gsettings() -> Command {
    Command::new("gsettings")
}

trait CheckResult {
    fn exit_ok(self, details: &str) -> Result<()>;
}

impl CheckResult for io::Result<ExitStatus> {
    fn exit_ok(self, details: &str) -> Result<()> {
        let exit_status = self.map_err(|err| anyhow!("{details}: {:?}",err))?;
        if exit_status.success() {
            Ok(())
        } else {
            Err(anyhow!("{details}, exit code: {:?}", exit_status.code()))
        }
    }
}