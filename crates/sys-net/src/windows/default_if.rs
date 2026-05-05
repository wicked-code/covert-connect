use anyhow::Result;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use windows::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR, WIN32_ERROR},
    NetworkManagement::{
        IpHelper::{
            FreeMibTable, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, GET_ADAPTERS_ADDRESSES_FLAGS,
            GetAdaptersAddresses, GetIfEntry2, GetIpForwardTable2, GetIpInterfaceEntry, IF_TYPE_PROP_VIRTUAL,
            IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH, InitializeIpInterfaceEntry, MIB_IF_ROW2,
            MIB_IPFORWARD_TABLE2, MIB_IPINTERFACE_ROW,
        },
        Ndis::{IfOperStatusUp, NET_LUID_LH},
    },
    Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6},
};

use crate::{DefaultIf, is_ipv6_global};

struct TableGuard(*mut MIB_IPFORWARD_TABLE2);
impl Drop for TableGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeMibTable(self.0 as *const _) };
        }
    }
}

pub fn find_default_if() -> Result<DefaultIf> {
    let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();

    unsafe {
        let result = GetIpForwardTable2(AF_UNSPEC, &mut table);
        if result != NO_ERROR {
            anyhow::bail!(
                "GetIpForwardTable2 failed with error code: {}",
                windows::core::Error::from(result)
            );
        }
    }

    let _guard = TableGuard(table);

    let table_ref = unsafe { &*table };
    let num_entries = table_ref.NumEntries as usize;

    let rows = unsafe { std::slice::from_raw_parts(table_ref.Table.as_ptr(), num_entries) };

    let mut lowest_metric = u32::MAX;
    let mut lowest_luid = None;
    for row in rows {
        if row.DestinationPrefix.PrefixLength != 0 || row.Loopback {
            continue;
        }

        let luid = row.InterfaceLuid;

        let mut if_row: MIB_IF_ROW2 = unsafe { std::mem::zeroed() };
        if_row.InterfaceLuid = luid;

        unsafe {
            let result = GetIfEntry2(&mut if_row);
            if result != NO_ERROR {
                tracing::error!("GetIfEntry2 failed: {}", windows::core::Error::from(result));
                continue;
            }
        }

        if if_row.OperStatus != IfOperStatusUp {
            continue;
        }

        if if_row.Type == IF_TYPE_PROP_VIRTUAL || if_row.Type == IF_TYPE_SOFTWARE_LOOPBACK {
            continue;
        }

        let mut ip_if_row: MIB_IPINTERFACE_ROW = unsafe { std::mem::zeroed() };

        unsafe { InitializeIpInterfaceEntry(&mut ip_if_row) };

        ip_if_row.InterfaceLuid = luid;
        ip_if_row.Family = AF_INET;

        unsafe {
            let result = GetIpInterfaceEntry(&mut ip_if_row);
            if result != NO_ERROR {
                tracing::error!(
                    "GetIpInterfaceEntry failed with error code: {}",
                    windows::core::Error::from(result)
                );
                continue;
            }
        }

        if !ip_if_row.Connected {
            continue;
        }

        let metric = row.Metric + ip_if_row.Metric;
        if metric < lowest_metric {
            lowest_metric = metric;
            lowest_luid = Some(luid);
        }
    }

    let luid = match lowest_luid {
        Some(luid) => luid,
        None => {
            anyhow::bail!("No suitable default interface found");
        }
    };

    get_default_if(luid)
}

fn get_default_if(luid: NET_LUID_LH) -> Result<DefaultIf> {
    let mut buf_len = 0u32;
    let mut dns_addrs = Vec::new();
    let mut ipv4 = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
    let mut ipv6 = IpAddr::V6(Ipv6Addr::UNSPECIFIED);

    let family = AF_UNSPEC.0 as u32;
    let flags = GET_ADAPTERS_ADDRESSES_FLAGS(GAA_FLAG_SKIP_ANYCAST.0 | GAA_FLAG_SKIP_MULTICAST.0);

    unsafe {
        let res = WIN32_ERROR(GetAdaptersAddresses(family, flags, None, None, &mut buf_len));
        if res != ERROR_BUFFER_OVERFLOW {
            anyhow::bail!("GetAdaptersAddresses failed: {}", windows::core::Error::from(res));
        }
    }

    let mut buffer = vec![0u8; buf_len as usize];

    unsafe {
        let res = WIN32_ERROR(GetAdaptersAddresses(
            family,
            flags,
            None,
            Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
            &mut buf_len,
        ));

        if res != NO_ERROR {
            anyhow::bail!("GetAdaptersAddresses failed: {}", windows::core::Error::from(res));
        }
    }

    let mut current = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !current.is_null() {
        unsafe {
            let adapter = &*current;
            if adapter.Luid.Value == luid.Value {
                let mut dns_ptr = adapter.FirstDnsServerAddress;
                while !dns_ptr.is_null() {
                    let dns = &*dns_ptr;
                    if let Some(ip) = sockaddr_to_ip(dns.Address.lpSockaddr, dns.Address.iSockaddrLength) {
                        dns_addrs.push(ip);
                    }
                    dns_ptr = dns.Next;
                }

                let mut unicast_ptr = adapter.FirstUnicastAddress;
                while !unicast_ptr.is_null() {
                    let unicast = &*unicast_ptr;
                    if let Some(ip) = sockaddr_to_ip(unicast.Address.lpSockaddr, unicast.Address.iSockaddrLength) {
                        match ip {
                            IpAddr::V4(_) if ipv4.is_unspecified() => ipv4 = ip,
                            IpAddr::V6(ipv6_addr) if ipv6.is_unspecified() && is_ipv6_global(ipv6_addr) => {
                                ipv6 = ip
                            }
                            _ => {}
                        }
                    }
                    unicast_ptr = unicast.Next;
                }

                break;
            }

            current = adapter.Next;
        }
    }

    Ok(DefaultIf {
        ipv4,
        ipv6,
        dns: dns_addrs,
    })
}

unsafe fn sockaddr_to_ip(sa: *mut SOCKADDR, len: i32) -> Option<IpAddr> {
    if sa.is_null() || len < 2 {
        return None;
    }

    match unsafe { *sa }.sa_family {
        AF_INET => {
            if len < std::mem::size_of::<SOCKADDR_IN>() as i32 {
                return None;
            }
            let in4 = unsafe { &*(sa as *const SOCKADDR_IN) };
            Some(IpAddr::V4(Ipv4Addr::from(in4.sin_addr)))
        }
        AF_INET6 => {
            if len < std::mem::size_of::<SOCKADDR_IN6>() as i32 {
                return None;
            }
            let in6 = unsafe { &*(sa as *const SOCKADDR_IN6) };
            Some(IpAddr::V6(Ipv6Addr::from(in6.sin6_addr)))
        }
        _ => None,
    }
}
