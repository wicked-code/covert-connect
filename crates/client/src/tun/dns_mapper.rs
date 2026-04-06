use anyhow::Result;
use parking_lot::RwLock;
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use rustc_hash::FxHashMap;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};
use tokio::select;

use crate::cancellable_task::CancellableTask;

const DEFAULT_TTL: std::time::Duration = std::time::Duration::from_secs(300);
const TTL_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_IP_RANGE: u32 = 131072;

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct IpRecord {
    pub ip: Ipv4Addr,
    pub expire_time: std::time::Instant,
}

pub struct DnsMapper {
    host_by_ip: RwLock<FxHashMap<Ipv4Addr, String>>,
    ip_by_host: RwLock<FxHashMap<String, IpRecord>>,
    base_ip: Ipv4Addr,
    address_index: AtomicU32,
    task: CancellableTask,
}

impl DnsMapper {
    pub fn new() -> Arc<Self> {
        let mut rng = ChaCha20Rng::from_entropy();
        Arc::new(Self {
            host_by_ip: RwLock::new(FxHashMap::default()),
            ip_by_host: RwLock::new(FxHashMap::default()),
            // safe for use 198.18.0.0/15 (For use in benchmark tests of network interconnect devices)
            base_ip: Ipv4Addr::new(198, 18, 0, 0),
            address_index: AtomicU32::new(rng.gen_range(0..MAX_IP_RANGE)),
            task: CancellableTask::new("DnsMapper"),
        })
    }

    pub async fn stop(self: &Arc<Self>) {
        self.task.stop().await;

        self.host_by_ip.write().clear();
        self.ip_by_host.write().clear();
    }

    pub async fn start(self: &Arc<Self>) -> Result<()> {        
        let self_clone = self.clone();
        self.task.spawn(|token| {
            async move {
                let mut interval = tokio::time::interval(TTL_CHECK_INTERVAL);
                loop {
                    let mut delete_records = Vec::new();
                    {
                        let mut ip_by_host_wr = self_clone.ip_by_host.write();
                        ip_by_host_wr.retain(|_, record| {
                            // TODO: ??? increase actual expire time ?
                            if record.expire_time <= std::time::Instant::now() {
                                delete_records.push(record.ip);
                                false
                            } else {
                                true
                            }
                        });
                    }

                    select! {
                        _ = interval.tick() => {}
                        _ = token.cancelled() => break,
                    }

                    let mut host_by_ip_wr = self_clone.host_by_ip.write();
                    for ip in delete_records {
                        host_by_ip_wr.remove(&ip);
                    }
                    drop(host_by_ip_wr);
                }
            }
        });

        Ok(())
    }

    pub fn resolve(&self, host: &str) -> IpRecord {
        let mut ip_by_host_wr = self.ip_by_host.write();
        if let Some(record) = ip_by_host_wr.get_mut(host) {
            record.expire_time = std::time::Instant::now() + DEFAULT_TTL;
            return *record;
        }

        let mut index = self.address_index.fetch_add(1, Ordering::Relaxed) % MAX_IP_RANGE;
        let mut ip = Ipv4Addr::from(u32::from(self.base_ip) + index);
        while self.host_by_ip.read().contains_key(&ip) {
            index = self.address_index.fetch_add(1, Ordering::Relaxed) % MAX_IP_RANGE;
            ip = Ipv4Addr::from(u32::from(self.base_ip) + index);
        }

        let record = IpRecord {
            ip,
            expire_time: std::time::Instant::now() + DEFAULT_TTL,
        };
        ip_by_host_wr.insert(host.to_string(), record);
        drop(ip_by_host_wr);

        self.host_by_ip.write().insert(ip, host.to_string());
        record
    }

    pub fn host_by_ip(&self, ip: IpAddr) -> Option<String> {
        match ip {
            IpAddr::V4(ipv4) => self.host_by_ip.read().get(&ipv4).cloned().or_else(|| {
                if (u32::from(ipv4) >= u32::from(self.base_ip))
                    && (u32::from(ipv4) < u32::from(self.base_ip) + MAX_IP_RANGE)
                {
                    tracing::error!("ip {} is not mapped to any host, maybe expired", ipv4);
                }
                None
            }),
            IpAddr::V6(_) => None,
        }
    }
}
