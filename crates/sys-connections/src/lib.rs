#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(target_os = "macos")]
pub use macos::*;
#[cfg(target_os = "windows")]
pub use windows::*;

use anyhow::{Result, bail};
use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
use std::net::SocketAddr;

/// The network protocol used by a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Transmission Control Protocol.
    TCP,
    /// User Datagram Protocol.
    UDP,
}

pub fn process_path_by_local_addr(addr: SocketAddr, protocol: Protocol) -> Result<String> {
    let af_flags = if addr.is_ipv6() {
        AddressFamilyFlags::IPV6
    } else {
        AddressFamilyFlags::IPV4
    };
    let proto_flags = match protocol {
        Protocol::TCP => ProtocolFlags::TCP,
        Protocol::UDP => ProtocolFlags::UDP,
    };
    let sockets = get_sockets_info(af_flags, proto_flags)?;

    for si in sockets {
        match si.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp_si) => {
                if tcp_si.local_addr == addr.ip() && tcp_si.local_port == addr.port() {
                    return path_by_pids(si.associated_pids, addr);
                }
            }
            ProtocolSocketInfo::Udp(udp_si) => {
                if udp_si.local_addr == addr.ip() && udp_si.local_port == addr.port() {
                    return path_by_pids(si.associated_pids, addr);
                }
            }
        }
    }

    bail!("no sockets found");
}

fn path_by_pids(pids: Vec<u32>, addr: SocketAddr) -> Result<String> {
    if pids.len() > 1 {
        tracing::warn!("socket with multiple pids: {:?}, socket: {:?}", pids, addr);
    }
    if let Some(pid) = pids.first() {
        path_by_pid(*pid).or_else(|err| {
            tracing::debug!("can't get name by pid {}: {}", pid, err);
            Ok(format!("pid {}", pid))
        })
    } else {
        bail!("no pid associated with socket: {:?}", addr);
    }
}
