use parking_lot::Mutex;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
pub use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct CancellableTaskHandle {
    pub token: CancellationToken,
    // hold a sender to track clones
    _ref_count: Option<mpsc::Sender<()>>,
}

pub struct CancelWatcher {
    inner: Mutex<Option<CancelWatcherInternal>>,
}

impl CancelWatcher {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Some(CancelWatcherInternal::new())),
        }
    }

    pub fn create_handle(&self) -> CancellableTaskHandle {
        let lock = self.inner.lock();
        match *lock {
            Some(ref inner) => inner.create_handle(),
            None => {
                drop(lock);
                let token = CancellationToken::new();
                token.cancel();
                CancellableTaskHandle {
                    token,
                    _ref_count: None,
                }
            }
        }
    }

    pub fn reset(&self) {
        let mut lock = self.inner.lock();
        *lock = Some(CancelWatcherInternal::new());
    }

    pub async fn wait_for_shutdown(&self, wait_limit: Duration) {
        let inner = self.inner.lock().take();
        match inner {
            Some(inner) => inner.wait_for_shutdown(wait_limit).await,
            None => {}
        }
    }
}

struct CancelWatcherInternal {
    token: CancellationToken,
    exit_rx: mpsc::Receiver<()>,
    // keep a master sender to check the count
    master_tx: mpsc::Sender<()>,
}

impl CancelWatcherInternal {
    fn new() -> Self {
        let token = CancellationToken::new();
        let (tx, rx) = mpsc::channel(1);

        Self {
            token,
            exit_rx: rx,
            master_tx: tx,
        }
    }

    fn create_handle(&self) -> CancellableTaskHandle {
        CancellableTaskHandle {
            token: self.token.clone(),
            _ref_count: Some(self.master_tx.clone()),
        }
    }

    async fn wait_for_shutdown(mut self, wait_limit: Duration) {
        self.token.cancel();

        // drop master reference so the count only reflects active tasks
        drop(self.master_tx);

        match timeout(wait_limit, self.exit_rx.recv()).await {
            Ok(None) => {}
            _ => {
                let leaked = self.exit_rx.sender_strong_count();
                tracing::error!("Shutdown timed out! {} tasks still holding handles.", leaked);
            }
        }
    }
}
