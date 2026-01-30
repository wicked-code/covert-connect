use anyhow::{anyhow, Result};
use flutter_rust_bridge::DartFnFuture;
use futures_util::{pin_mut, StreamExt};
use std::env::temp_dir;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs::{create_dir_all, rename, File},
    io::{self, Write},
    sync::Arc,
};
use tokio::{io::BufReader, sync::RwLock};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::{filter, prelude::*};

use crate::api::rev_lines_ex::{RevLine, RevLines};

#[cfg(debug_assertions)]
const LOG_FILE_NAME: &str = "covert-connect.debug.log";
#[cfg(debug_assertions)]
const PREV_FILE_NAME: &str = "covert-connect.debug.log.old";

#[cfg(not(debug_assertions))]
const LOG_FILE_NAME: &str = "covert-connect.log";
#[cfg(not(debug_assertions))]
const PREV_FILE_NAME: &str = "covert-connect.log.old";

pub struct LogLine {
    pub line: String,
    pub position: u64,
}

impl From<RevLine> for LogLine {
    fn from(rev_line: RevLine) -> Self {
        LogLine {
            line: rev_line.line,
            position: rev_line.position,
        }
    }
}

pub async fn get_trace_log(start: Option<u64>, limit: usize) -> Result<Vec<LogLine>> {
    let path = temp_dir().join(LOG_FILE_NAME);

    // TODO: implement custom buffered reverse reader
    // return lines with pos/id (start position in file)
    // it helps to read only new lines and merge them

    let file = tokio::fs::File::open(path).await?;
    let rev_lines = RevLines::new(BufReader::new(file), start).await?;
    pin_mut!(rev_lines);

    let mut result = Vec::new();
    while let Some(line) = rev_lines.next().await {
        if result.len() >= limit {
            break;
        }
        result.push(line?);
    }

    Ok(result.into_iter().map(LogLine::from).collect())
}

pub fn init_trace_log() -> Result<Arc<WriterNotifier>> {
    let app_dir = temp_dir();
    let path = app_dir.join(LOG_FILE_NAME);

    if path.exists() {
        // rename old file
        rename(&path, app_dir.join(PREV_FILE_NAME))?;
    } else {
        create_dir_all(&app_dir)?;
    }

    let stdout_log = tracing_subscriber::fmt::layer().compact();

    let file = File::create(path)?;
    let app_log = tracing_subscriber::fmt::layer().json().with_writer(Arc::new(file));

    let writer_notifier = Arc::new(WriterNotifier::new());
    let wrapper = WriterNotifierWrapper(writer_notifier.clone());
    let sender = tracing_subscriber::fmt::layer().json().with_writer(wrapper);

    tracing_subscriber::registry()
        .with(stdout_log.with_filter(filter::LevelFilter::INFO))
        .with(app_log.with_filter(filter::LevelFilter::INFO))
        .with(sender.with_filter(filter::LevelFilter::INFO))
        .init();

    Ok(writer_notifier)
}

/// flutter_rust_bridge:ignore
pub struct Callback {
    pub id: u64,
    pub callback: Box<dyn Fn(String) -> DartFnFuture<()> + Send + Sync>,
}

pub struct WriterNotifier {
    next_id: AtomicU64,
    callbacks: RwLock<Vec<Callback>>,
}

impl WriterNotifier {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(0),
            callbacks: Default::default(),
        }
    }
}

pub struct WriterNotifierWrapper(Arc<WriterNotifier>);

impl WriterNotifier {
    pub async fn register_logger(
        &self,
        callback: impl Fn(String) -> DartFnFuture<()> + Send + Sync + 'static,
    ) -> Result<u64> {
        let id = self.next_id.load(Ordering::Relaxed);
        self.callbacks.write().await.push(Callback {
            id: id,
            callback: Box::new(callback),
        });
        self.next_id.fetch_add(1, Ordering::Relaxed);
        Ok(id)
    }

    pub async fn unregister_logger(&self, id: u64) -> Result<()> {
        let mut wr_callbacks = self.callbacks.write().await;
        if let Some(pos) = wr_callbacks.iter().position(|s| s.id == id) {
            wr_callbacks.remove(pos);
            Ok(())
        } else {
            Err(anyhow!("not found"))
        }
    }
}

impl Write for WriterNotifierWrapper {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let self_clone = self.0.clone();
        let buf_clone = buf.to_vec();
        tokio::task::spawn(async move {
            self_clone.callbacks.read().await.iter().for_each(|callback| {
                let line = String::from_utf8_lossy(&buf_clone).to_string();
                let fut = (callback.callback)(line);
                tokio::spawn(async move {
                    let _ = fut.await;
                });
            });
        });

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for WriterNotifierWrapper {
    type Writer = WriterNotifierWrapper;
    fn make_writer(&'a self) -> Self::Writer {
        WriterNotifierWrapper(self.0.clone())
    }
}
