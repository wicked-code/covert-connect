#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

pub struct DefaultIf {
    pub ipv4: std::net::IpAddr,
    pub ipv6: std::net::IpAddr,
    pub dns: Vec<std::net::IpAddr>,
}
