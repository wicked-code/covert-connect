use parking_lot::Mutex;
use std::future::Future;
use tokio::{task::JoinHandle, time::timeout, sync::Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;

const STOP_TASK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

struct TaskHandle {
    task: JoinHandle<()>,
    token: CancellationToken,
}

pub struct CancellableTask {
    name: String,
    op_lock: AsyncMutex<()>,
    handle: Mutex<Option<TaskHandle>>,
}

impl CancellableTask {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            op_lock: AsyncMutex::new(()),
            handle: Mutex::new(None),
        }
    }

    pub fn spawn<F, R>(&self, task_fn: F)
    where
        F: FnOnce(CancellationToken) -> R,
        R: Future<Output = ()> + Send + 'static,
    {
        let _lock = match self.op_lock.try_lock() {
            Ok(_guard) => _guard,
            Err(_) => {
                tracing::error!("task {} is already starting, second start ignored", self.name);
                return;
            }
        };

        if self.handle.lock().is_some() {
            tracing::error!("task {} is already running, second start ignored", self.name);
            return;
        }

        let token = CancellationToken::new();
        let task = tokio::spawn(task_fn(token.clone()));

        *self.handle.lock() = Some(TaskHandle { task, token });
    }

    pub async fn stop(&self) {
        let op_lock = self.op_lock.lock().await;
        let handle = self.handle.lock().take();
        if let Some(mut handle) = handle {
            handle.token.cancel();
            match timeout(STOP_TASK_TIMEOUT, &mut handle.task).await {
                Ok(res) => {
                    drop(op_lock);
                    if let Err(err) = res {
                        tracing::error!("task {} error: {:?}", self.name, err);
                    }
                }
                Err(_) => {
                    tracing::warn!("task {} did not stop within timeout, aborting", self.name);
                    handle.task.abort();
                    drop(op_lock);
                    if timeout(STOP_TASK_TIMEOUT, &mut handle.task).await.is_err() {
                        tracing::error!("task {} did not stop after aborting", self.name);
                    }
                }
            }
        }
    }
}

impl Drop for CancellableTask {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.get_mut().take() {
            tracing::info!("task {}, was not stopped, dropping", self.name);
            handle.token.cancel();
            handle.task.abort();
        }
    }
}
