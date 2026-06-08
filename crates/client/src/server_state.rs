use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;
use std::time::Instant;

const BUFFERED_COUNTER_SLOT_INTERVAL: usize = 1; // sec
const BUFFERED_COUNTER_SLOT_COUNT: usize = 3;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ServerState {
    pub rx_total: AtomicU64,
    pub tx_total: AtomicU64,

    // connections with zero data or error returned from server
    // aprox in last BUFFERED_COUNTER_SLOT_INTERVAL * BUFFERED_COUNTER_SLOT_COUNT seconds
    pub err_count: Mutex<BufferedCounter>,
    // connections with no zero data returned from server
    // aprox in last BUFFERED_COUNTER_SLOT_INTERVAL * BUFFERED_COUNTER_SLOT_COUNT seconds
    pub success_count: Mutex<BufferedCounter>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedCounter {
    slot: [u64; BUFFERED_COUNTER_SLOT_COUNT],
    current_slot: usize,
    #[serde(skip, default = "Instant::now")]
    current_slot_start: Instant,
}

impl Default for BufferedCounter {
    fn default() -> Self {
        Self {
            slot: [0; BUFFERED_COUNTER_SLOT_COUNT],
            current_slot: 0,
            current_slot_start: Instant::now(),
        }
    }
}

impl BufferedCounter {
    pub fn inc(&mut self) {
        if self.current_slot_start.elapsed().as_secs() >= BUFFERED_COUNTER_SLOT_INTERVAL as u64 {
            self.current_slot = (self.current_slot + 1) % BUFFERED_COUNTER_SLOT_COUNT;
            self.slot[self.current_slot] = 0;
            self.current_slot_start = Instant::now();
        }
        self.slot[self.current_slot] += 1;
    }

    pub fn value(&self) -> u64 {
        self.slot.iter().sum()
    }
}
