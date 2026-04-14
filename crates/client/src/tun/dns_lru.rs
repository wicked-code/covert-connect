use rustc_hash::FxHashMap;
use std::net::Ipv4Addr;

use crate::utils::item_pool::{ItemId, ItemPool};

struct DnsLRUItem {
    pub ip: Ipv4Addr,
    pub host: String,
    pub prev: Option<ItemId>,
    pub next: Option<ItemId>,
}

pub struct DnsLRU {
    host_by_ip: FxHashMap<Ipv4Addr, ItemId>,
    item_by_host: FxHashMap<String, ItemId>,
    head: Option<ItemId>,
    tail: Option<ItemId>,
    items: ItemPool<DnsLRUItem>,
    max_size: usize,
}

impl DnsLRU {
    pub fn new(max_size: usize) -> Self {
        Self {
            host_by_ip: FxHashMap::default(),
            item_by_host: FxHashMap::default(),
            head: None,
            tail: None,
            items: ItemPool::new(),
            max_size,
        }
    }

    pub fn insert(&mut self, host: String, ip: Ipv4Addr) {
        // delete least recently used if capacity exceeds
        let id = if self.item_by_host.len() >= self.max_size {
            let tail = self.tail.unwrap();

            self.item_by_host.remove(&self.items[tail].host);
            self.host_by_ip.remove(&self.items[tail].ip);

            self.items[tail].host.clone_from(&host);
            self.items[tail].ip = ip;

            tail
        } else {
            let item = DnsLRUItem {
                ip,
                host: host.clone(),
                prev: None,
                next: None,
            };

            self.items.insert(item)
        };

        self.item_by_host.insert(host, id);
        self.host_by_ip.insert(ip, id);

        self.move_to_head(id);
    }

    pub fn ip_by_host_and_update(&mut self, host: &str) -> Option<Ipv4Addr> {
        let id = match self.item_by_host.get(host) {
            Some(id) => *id,
            None => return None,
        };

        self.move_to_head(id);
        Some(self.items[id].ip)
    }

    pub fn host_by_ip_and_update(&mut self, ip: &Ipv4Addr) -> Option<String> {
        let id = match self.host_by_ip.get(ip) {
            Some(id) => *id,
            None => return None,
        };

        self.move_to_head(id);
        Some(self.items[id].host.clone())
    }

    pub fn ip_exists(&self, ip: &Ipv4Addr) -> bool {
        self.host_by_ip.contains_key(ip)
    }

    pub fn clear(&mut self) {
        self.head = None;
        self.tail = None;
        self.host_by_ip.clear();
        self.item_by_host.clear();
        self.items.clear();
    }

    fn move_to_head(&mut self, id: ItemId) {
        if self.head == Some(id) {
            return;
        }

        // remove item from current position
        let prev = self.items[id].prev;
        let next = self.items[id].next;
        if let Some(prev) = prev {
            self.items[prev].next = next;
            self.items[id].prev = None;
        }
        if let Some(next) = next {
            self.items[next].prev = prev;
            self.items[id].next = None;
        }
        if self.tail == Some(id) {
            self.tail = prev;
        }

        // update head
        if let Some(head) = self.head {
            self.items[head].prev = Some(id);
            self.items[id].next = Some(head);
        }
        self.head = Some(id);
        if self.tail.is_none() {
            self.tail = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    #[test]
    fn dns_lru_basic() {
        let mut lru = super::DnsLRU::new(3);
        lru.insert("host1".to_string(), "192.168.0.1".parse().unwrap());
        lru.insert("host2".to_string(), "192.168.0.2".parse().unwrap());
        lru.insert("host3".to_string(), "192.168.0.3".parse().unwrap());
        lru.insert("host4".to_string(), "192.168.0.4".parse().unwrap());

        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.1".parse().unwrap()), None);
        assert_eq!(lru.ip_by_host_and_update("host1"), None);
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.2".parse().unwrap()), Some("host2".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host2"), Some("192.168.0.2".parse().unwrap()));
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.3".parse().unwrap()), Some("host3".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host3"), Some("192.168.0.3".parse().unwrap()));
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.4".parse().unwrap()), Some("host4".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host4"), Some("192.168.0.4".parse().unwrap()));
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.2".parse().unwrap()), Some("host2".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host2"), Some("192.168.0.2".parse().unwrap()));

        lru.insert("host5".to_string(), "192.168.0.5".parse().unwrap());

        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.1".parse().unwrap()), None);
        assert_eq!(lru.ip_by_host_and_update("host1"), None);
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.2".parse().unwrap()), Some("host2".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host2"), Some("192.168.0.2".parse().unwrap()));
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.3".parse().unwrap()), None);
        assert_eq!(lru.ip_by_host_and_update("host3"), None);
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.4".parse().unwrap()), Some("host4".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host4"), Some("192.168.0.4".parse().unwrap()));
        assert_eq!(lru.host_by_ip_and_update(&"192.168.0.2".parse().unwrap()), Some("host2".to_string()));
        assert_eq!(lru.ip_by_host_and_update("host2"), Some("192.168.0.2".parse().unwrap()));

        assert_eq!(lru.tail, Some(NonZeroU32::new(3 as u32).unwrap().into()));
        assert_eq!(lru.head, Some(NonZeroU32::new(2 as u32).unwrap().into()));
    }
}
