use std::io::{self, ErrorKind};
use anyhow::Result;
use winapi::shared::ntdef::NULL;
use winapi::um::wininet::{
    InternetSetOptionA, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
};
use winreg::{enums, RegKey};

pub struct SystemProxy {
}

const AUTO_CONFIG_URL: &str = "AutoConfigURL";
const PROXY_ENABLED: &str = "ProxyEnable";
const PROXY_SERVER: &str = "ProxyServer";
const INTERNET_SETTINGS_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Internet Settings";

impl SystemProxy {
    pub fn new() -> Result<SystemProxy> {
        Ok(SystemProxy {})
    }

    pub fn set_pac(&self, auto_config_url: &str) -> Result<()> {
        let reg_key = open_internet_settings(enums::KEY_WRITE)?;

        reg_key.set_value(AUTO_CONFIG_URL, &auto_config_url)?;

        notify_settings_changed();
        Ok(())
    }

    pub fn set_proxy(&self, address: &str) ->Result<()> {
        let reg_key = open_internet_settings(enums::KEY_WRITE)?;

        reg_key.delete_value(AUTO_CONFIG_URL).ok();

        let enabled = 1u32;
        reg_key.set_value(PROXY_ENABLED, &enabled)?;
        reg_key.set_value(PROXY_SERVER, &address)?;

        notify_settings_changed();
        Ok(())
    }

    pub fn restore(&self) -> Result<()> {
        let reg_key = open_internet_settings(enums::KEY_WRITE)?;

        match reg_key.delete_value(AUTO_CONFIG_URL) {
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
            Ok(_) => Ok(())
        }?;

        let disabled = 0u32;
        reg_key.set_value(PROXY_ENABLED, &disabled)?;

        notify_settings_changed();
        Ok(())
    }
}

fn open_internet_settings(perms: u32) -> io::Result<RegKey> {
    let hkcu = RegKey::predef(enums::HKEY_CURRENT_USER);
    hkcu.open_subkey_with_flags(INTERNET_SETTINGS_KEY, perms)
}

fn notify_settings_changed() {
    unsafe {
        InternetSetOptionA(NULL, INTERNET_OPTION_SETTINGS_CHANGED, NULL, 0);
        InternetSetOptionA(NULL, INTERNET_OPTION_REFRESH, NULL, 0);
    }
}
