use anyhow::{bail, Result};
use std::net::{Ipv4Addr, SocketAddr};
use windows::Win32::{
    Foundation::{ERROR_INSUFFICIENT_BUFFER, MAX_PATH, NO_ERROR},
    NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6TABLE_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
        MIB_UDP6TABLE_OWNER_PID, MIB_UDPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_CONNECTIONS, UDP_TABLE_OWNER_PID,
    },
    Networking::WinSock::{AF_INET, AF_INET6},
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

/// The network protocol used by a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Transmission Control Protocol.
    TCP,
    /// User Datagram Protocol.
    UDP,
}

pub fn get_process_name_by_local_addr(addr: SocketAddr, protocol: Protocol) -> Result<String> {
    let ulaf = if addr.is_ipv6() { AF_INET6.0 } else { AF_INET.0 } as u32;
    match protocol {
        Protocol::TCP => get_process_name_tcp(addr, ulaf),
        Protocol::UDP => get_process_name_udp(addr, ulaf),
    }
}

fn get_name_by_pid(pid: u32) -> Result<String> {
    return get_name_by_pid_internal(pid).map_err(|err| {
        anyhow::anyhow!("failed to get process name by pid {}: {}", pid, err)
    });
}

fn get_name_by_pid_internal(pid: u32) -> Result<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;

        const DEFAULT_BUFFER: u32 = MAX_PATH + 1;
        let mut buffer_size: u32 = DEFAULT_BUFFER;
        let mut path_buffer = [0u16; DEFAULT_BUFFER as usize];

        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(path_buffer.as_mut_ptr()),
            &mut buffer_size,
        )?;

        Ok(if buffer_size < DEFAULT_BUFFER {
            String::from_utf16(&path_buffer[0..buffer_size as usize])?
        } else {
            buffer_size = 32769;
            let mut path_buffer = vec![0u16; buffer_size as usize];
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(path_buffer.as_mut_ptr()),
                &mut buffer_size,
            )?;

            String::from_utf16(&path_buffer[0..buffer_size as usize])?
        })
    }
}

fn get_process_name_tcp(addr: SocketAddr, ulaf: u32) -> Result<String> {
    let mut buffer_size: u32 = 0;
    let result =
        unsafe { GetExtendedTcpTable(None, &mut buffer_size, false, ulaf, TCP_TABLE_OWNER_PID_CONNECTIONS, 0) };
    if result != ERROR_INSUFFICIENT_BUFFER.0 {
        bail!("GetExtendedTcpTable size error: {}", result);
    }

    for _ in 1..7 {
        let mut buffer = vec![0u8; buffer_size as usize];

        let result = unsafe {
            GetExtendedTcpTable(
                Some(buffer.as_mut_ptr() as *mut _),
                &mut buffer_size,
                false,
                ulaf,
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER.0 {
            continue;
        }
        if result != NO_ERROR.0 {
            bail!("GetExtendedTcpTable error: {}", result);
        }

        match addr {
            SocketAddr::V6(ipv6) => {
                let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID) };
                for idx in 0..table.dwNumEntries - 1 {
                    let item = unsafe { &*table.table.as_ptr().add(idx as usize) };
                    if same_port(item.dwLocalPort, ipv6.port()) && item.ucLocalAddr == ipv6.ip().octets() {
                        return get_name_by_pid(item.dwOwningPid);
                    }
                }
            }
            SocketAddr::V4(ipv4) => {
                let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
                for idx in 0..table.dwNumEntries - 1 {
                    let item = unsafe { &*table.table.as_ptr().add(idx as usize) };
                    if same_port(item.dwLocalPort, ipv4.port()) && same_address(item.dwLocalAddr, ipv4.ip()) {
                        return get_name_by_pid(item.dwOwningPid);
                    }
                }
            }
        }

        bail!("not found");
    }

    bail!("too many insufficent buffer results");
}

fn get_process_name_udp(addr: SocketAddr, ulaf: u32) -> Result<String> {
    let mut buffer_size: u32 = 0;
    let result = unsafe { GetExtendedUdpTable(None, &mut buffer_size, false, ulaf, UDP_TABLE_OWNER_PID, 0) };
    if result != ERROR_INSUFFICIENT_BUFFER.0 {
        bail!("GetExtendedTcpTable size error: {}", result);
    }

    for _ in 1..7 {
        let mut buffer = vec![0u8; buffer_size as usize];

        let result = unsafe {
            GetExtendedUdpTable(
                Some(buffer.as_mut_ptr() as *mut _),
                &mut buffer_size,
                false,
                ulaf,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };
        if result == ERROR_INSUFFICIENT_BUFFER.0 {
            continue;
        }
        if result != NO_ERROR.0 {
            bail!("GetExtendedTcpTable error: {}", result);
        }

        match addr {
            SocketAddr::V6(ipv6) => {
                let table = unsafe { &*(buffer.as_ptr() as *const MIB_UDP6TABLE_OWNER_PID) };
                for idx in 0..table.dwNumEntries - 1 {
                    let item = unsafe { &*table.table.as_ptr().add(idx as usize) };
                    if same_port(item.dwLocalPort, ipv6.port()) && item.ucLocalAddr == ipv6.ip().octets() {
                        return get_name_by_pid(item.dwOwningPid);
                    }
                }
            }
            SocketAddr::V4(ipv4) => {
                let table = unsafe { &*(buffer.as_ptr() as *const MIB_UDPTABLE_OWNER_PID) };
                for idx in 0..table.dwNumEntries - 1 {
                    let item = unsafe { &*table.table.as_ptr().add(idx as usize) };
                    if same_port(item.dwLocalPort, ipv4.port()) && same_address(item.dwLocalAddr, ipv4.ip()) {
                        return get_name_by_pid(item.dwOwningPid);
                    }
                }
            }
        }

        bail!("not found");
    }

    bail!("too many insufficent buffer results");
}

fn same_port(raw_port: u32, port: u16) -> bool {
    return ((raw_port & 0x0000FF00) >> 8) as u16 | ((raw_port & 0x000000FF) << 8) as u16 == port;
}

fn same_address(raw_addr: u32, address: &Ipv4Addr) -> bool {
    return std::net::Ipv4Addr::new(
        (raw_addr & 0x000000FF) as u8,
        ((raw_addr & 0x0000FF00) >> 8) as u8,
        ((raw_addr & 0x00FF0000) >> 16) as u8,
        ((raw_addr & 0xFF000000) >> 24) as u8,
    ) == *address;
}
