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

    // err_count: connections with zero data or error returned from server
    // success_count: connections with non-zero data returned from server
    // both aprox in last BUFFERED_COUNTER_SLOT_INTERVAL * BUFFERED_COUNTER_SLOT_COUNT seconds.
    pub counter: Mutex<BufferedCounter>,
}

impl ServerState {
    /// Locks the counter once and returns `(err, success)` values together.
    pub fn counter_values(&self) -> (u64, u64) {
        let counter = self.counter.lock();
        (counter.err_value(), counter.success_value())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BufferedCounter {
    err_slot: [u64; BUFFERED_COUNTER_SLOT_COUNT],
    success_slot: [u64; BUFFERED_COUNTER_SLOT_COUNT],
    current_slot: usize,
    #[serde(skip, default = "Instant::now")]
    current_slot_start: Instant,
}

impl Default for BufferedCounter {
    fn default() -> Self {
        Self {
            err_slot: [0; BUFFERED_COUNTER_SLOT_COUNT],
            success_slot: [0; BUFFERED_COUNTER_SLOT_COUNT],
            current_slot: 0,
            current_slot_start: Instant::now(),
        }
    }
}

impl BufferedCounter {
    fn rotate(&mut self) {
        if self.current_slot_start.elapsed().as_secs() >= BUFFERED_COUNTER_SLOT_INTERVAL as u64 {
            self.current_slot = (self.current_slot + 1) % BUFFERED_COUNTER_SLOT_COUNT;
            self.err_slot[self.current_slot] = 0;
            self.success_slot[self.current_slot] = 0;
            self.current_slot_start = Instant::now();
        }
    }

    pub fn inc_err(&mut self) {
        self.rotate();
        self.err_slot[self.current_slot] += 1;
    }

    pub fn inc_success(&mut self) {
        self.rotate();
        self.success_slot[self.current_slot] += 1;
    }

    pub fn err_value(&self) -> u64 {
        self.err_slot.iter().sum()
    }

    pub fn success_value(&self) -> u64 {
        self.success_slot.iter().sum()
    }
}
