use anyhow::Result;
use parking_lot::{Mutex, RwLock};
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
use tokio::{select, task::JoinHandle};
use tokio_util::sync::CancellationToken;

use crate::tun::dns_mapper::DnsMapper;
pub struct DnsServer {
    dns_mapper: Arc<DnsMapper>,
}

// flush dns !!!

impl DnsServer {
    pub fn new(dns_mapper: Arc<DnsMapper>) -> Arc<Self> {
        Arc::new(Self { dns_mapper })
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.dns_mapper.stop().await?;
        Ok(())
    }

    pub async fn serve(self: &Arc<Self>) -> Result<()> {
        self.dns_mapper.start().await?;
        Ok(())
    }
}