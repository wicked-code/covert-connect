use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::{mem, net::IpAddr, process::Command, str::FromStr, sync::Arc};

mod ffi;
mod nw_path_monitor;

#[derive(Clone)]
enum ProxyState {
    Off,
    Pac(String),
    Proxy(String),
}

struct SytemProxyInner {
    state: Mutex<ProxyState>,
}

pub struct SystemProxy {
    inner: Arc<SytemProxyInner>,
}

const GET_INFO: &str = "-getinfo";
const SET_PAC: &str = "-setautoproxyurl";
const SET_HTTPS: &str = "-setsecurewebproxy";
const SET_PAC_STATE: &str = "-setautoproxystate";
const SET_HTTPS_STATE: &str = "-setsecurewebproxystate";
const STATE_OFF: &str = "off";
const LIST_SERVICES: &str = "-listallnetworkservices";

impl SystemProxy {
    pub fn new() -> Result<SystemProxy> {
        let inner = Arc::new(SytemProxyInner {
            state: Mutex::new(ProxyState::Off),
        });

        let inner_clone = inner.clone();
        nw_path_monitor::NWMonitor::start(move || inner_clone.update());

        Ok(SystemProxy { inner })
    }

    pub fn set_pac(&self, auto_config_url: &str) -> Result<()> {
        self.inner.set_pac(auto_config_url)
    }

    pub fn set_proxy(&self, address: &str) -> Result<()> {
        self.inner.set_proxy(address)
    }

    pub fn restore(&self) -> Result<()> {
        self.inner.restore()
    }
}

impl SytemProxyInner {
    fn set_pac(&self, auto_config_url: &str) -> Result<()> {
        let services = get_services(true)?;
        for service in services {
            networksetup().args([SET_PAC, &service, auto_config_url]).output()?;
        }

        let new_state = ProxyState::Pac(auto_config_url.to_owned());
        let old_state = self.repalce_state(new_state);
        match old_state {
            ProxyState::Off | ProxyState::Pac(_) => {}
            _ => restore(old_state)?,
        }

        Ok(())
    }

    fn set_proxy(&self, address: &str) -> Result<()> {
        let mut parts = address.split(&['/', ':']).rev();
        let port = parts.next().ok_or_else(|| anyhow!("set_proxy: no port"))?;
        let domain = parts.next().ok_or_else(|| anyhow!("set_proxy: no host"))?;
        if let None = parts.next() {
            anyhow::bail!("set_proxy: invalid address");
        }

        let services = get_services(true)?;
        for service in services {
            networksetup()
                .args([SET_HTTPS, &service, domain, port, STATE_OFF])
                .output()?;
        }

        let new_state = ProxyState::Proxy(address.to_owned());
        let old_state = self.repalce_state(new_state);
        match old_state {
            ProxyState::Off | ProxyState::Proxy(_) => {}
            _ => restore(old_state)?,
        }

        Ok(())
    }

    fn restore(&self) -> Result<()> {
        let old_state = self.repalce_state(ProxyState::Off);
        restore(old_state)
    }

    fn update(&self) {
        let state = self.state.lock().clone();
        match state {
            ProxyState::Off => {}
            ProxyState::Pac(url) => {
                self.set_pac(&url).ok();
            }
            ProxyState::Proxy(address) => {
                self.set_proxy(&address).ok();
            }
        }
    }

    fn repalce_state(&self, state: ProxyState) -> ProxyState {
        let mut locked_state = self.state.lock();
        mem::replace(&mut *locked_state, state)
    }
}

fn restore(state: ProxyState) -> Result<()> {
    let services = get_services(false)?;
    for service in &services {
        match state {
            ProxyState::Off => {}
            ProxyState::Pac(_) => {
                networksetup().args([SET_PAC_STATE, service, STATE_OFF]).output()?;
            }
            ProxyState::Proxy(_) => {
                networksetup().args([SET_HTTPS_STATE, service, STATE_OFF]).output()?;
            }
        }
    }

    Ok(())
}

fn networksetup() -> Command {
    Command::new("networksetup")
}

fn get_services(active_only: bool) -> Result<Vec<String>> {
    let output = String::from_utf8(networksetup().args([LIST_SERVICES]).output()?.stdout)?;

    let services = output.lines().map(|l| l.to_owned());
    let services = if active_only {
        services.filter(|service| is_service_active(service)).collect()
    } else {
        services.collect()
    };

    Ok(services)
}

fn is_service_active(service: &str) -> bool {
    let output = get_info(service);
    if output.is_err() {
        return true;
    }

    let output = output.unwrap();
    let lines = output.lines();
    for line in lines {
        if !line.contains("IP address") {
            continue;
        }

        if let Some(ip) = line.split(':').rev().next() {
            if IpAddr::from_str(ip.trim()).is_ok() {
                return true;
            }
        }
    }

    false
}

fn get_info(service: &str) -> Result<String> {
    Ok(String::from_utf8(
        networksetup().args([GET_INFO, service]).output()?.stdout,
    )?)
}
