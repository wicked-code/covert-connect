use anyhow::{Result, bail};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

use crate::tun::dns_lru::DnsLRU;

const DEFAULT_TTL: Duration = Duration::from_secs(300);
const MAX_IP_RANGE: u32 = 131072;
const DNS_LRU_MAX_CAPACITY: usize = 4096;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct IpRecord {
    pub ip: Ipv4Addr,
    pub expire_time: Instant,
}

pub struct DnsMapper {
    dns_lru: Mutex<DnsLRU>,
    base_ip: Ipv4Addr,
    address_index: AtomicU32,
    default_ttl: Duration,
}

impl IpRecord {
    pub fn new(ip: Ipv4Addr, ttl: Duration) -> Self {
        Self {
            ip,
            expire_time: Instant::now() + ttl,
        }
    }
}

impl DnsMapper {
    pub fn new() -> Arc<Self> {
        let mut rng = ChaCha20Rng::from_entropy();
        Arc::new(Self {
            dns_lru: Mutex::new(DnsLRU::new(DNS_LRU_MAX_CAPACITY)),
            // safe for use 198.18.0.0/15 (For use in benchmark tests of network interconnect devices)
            base_ip: Ipv4Addr::new(198, 18, 0, 0),
            address_index: AtomicU32::new(rng.gen_range(0..MAX_IP_RANGE)),
            default_ttl: DEFAULT_TTL,
        })
    }

    pub async fn clear(&self) {
        self.dns_lru.lock().await.clear();
    }

    pub async fn resolve(&self, host: &str) -> IpRecord {
        let host = host.to_string();
        if let Some(ip) = self.dns_lru.lock().await.ip_by_host_and_update(&host) {
            return IpRecord::new(ip, self.default_ttl);
        }

        let mut index = self.address_index.fetch_add(1, Ordering::Relaxed) % MAX_IP_RANGE;
        let mut ip = Ipv4Addr::from(u32::from(self.base_ip) + index);
        while self.dns_lru.lock().await.ip_exists(&ip) {
            index = self.address_index.fetch_add(1, Ordering::Relaxed) % MAX_IP_RANGE;
            ip = Ipv4Addr::from(u32::from(self.base_ip) + index);
        }

        self.dns_lru.lock().await.insert(host, ip);
        IpRecord::new(ip, self.default_ttl)
    }

    pub async fn host_by_ip(&self, ip: IpAddr) -> Result<Option<String>> {
        if let IpAddr::V4(ipv4) = ip {
            let host = self.dns_lru.lock().await.host_by_ip_and_update(&ipv4);
            if host.is_none() {
                if (u32::from(ipv4) >= u32::from(self.base_ip))
                    && (u32::from(ipv4) < u32::from(self.base_ip) + MAX_IP_RANGE)
                {
                    bail!("ip {} is not mapped to any host, maybe expired", ipv4);
                }
            }

            Ok(host)
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv4Addr},
        sync::{atomic::Ordering},
    };

    #[tokio::test]
    async fn ports_overflow() {
        let mapper = super::DnsMapper::new();
        let frequent_used_host = "frequent_used.example.com";
        let frequent_used_host2 = "frequent_used2.example.com";

        let start_index = mapper.address_index.load(Ordering::Relaxed);
        let ip_from_index = |i: u32| -> IpAddr {
            let idx = (start_index + i) % super::MAX_IP_RANGE;
            IpAddr::V4(Ipv4Addr::from(u32::from(mapper.base_ip) + idx))
        };

        let check_resolve = |host: &str, i: u32| {
            let mapper_clone = mapper.clone();
            let ip = ip_from_index(i);
            let host = host.to_string();
            async move {
                let record = mapper_clone.resolve(&host).await;
                assert_eq!(record.ip, ip);
                assert_eq!(
                    mapper_clone.host_by_ip(IpAddr::V4(record.ip)).await.unwrap().unwrap(),
                    host
                );
            }
        };

        let check_host_by_ip = |i: u32, host: &str| {
            let mapper_clone = mapper.clone();
            let ip = ip_from_index(i);
            let host = host.to_string();
            async move {
                assert_eq!(mapper_clone.host_by_ip(ip).await.unwrap().unwrap(), host);
            }
        };

        let mut ip_idx = 2;
        for i in 0..super::MAX_IP_RANGE * 2 {
            if i % 100 == 0 {
                check_resolve(frequent_used_host, 0).await;
                check_resolve(frequent_used_host2, 1).await;
            }
            if ip_idx > 0 && (ip_idx % super::MAX_IP_RANGE) == 0 {
                ip_idx += 2;
            }

            let host = format!("host{}.example.com", i);
            check_resolve(&host, ip_idx).await;
            ip_idx += 1;

            check_host_by_ip(0, &frequent_used_host).await;
            check_host_by_ip(1, &frequent_used_host2).await;
        }
    }
}
