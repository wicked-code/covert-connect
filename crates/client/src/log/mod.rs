mod rev_lines_ex;

use anyhow::{Result, anyhow, bail};
use futures_util::{StreamExt, pin_mut};
use serde::{Deserialize, Serialize};
use std::env::temp_dir;
use std::sync::OnceLock;
use std::{
    fs::{File, create_dir_all, rename},
    sync::Arc,
};
use tokio::io::BufReader;
use tracing_subscriber::{filter, prelude::*};

use crate::log::rev_lines_ex::{RevLine, RevLines};

#[cfg(debug_assertions)]
const LOG_FILE_NAME_EXT: &str = ".debug.log";
#[cfg(debug_assertions)]
const PREV_FILE_NAME_EXT: &str = ".debug.log.old";

#[cfg(not(debug_assertions))]
const LOG_FILE_NAME_EXT: &str = ".log";
#[cfg(not(debug_assertions))]
const PREV_FILE_NAME_EXT: &str = ".log.old";

static LOG_FILE: OnceLock<String> = OnceLock::new();

#[derive(Debug, Serialize, Deserialize)]
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

pub async fn get_trace_log(start: Option<u64>, end: Option<u64>, limit: usize) -> Result<Vec<LogLine>> {
    let path = temp_dir().join(LOG_FILE.get().ok_or_else(|| anyhow!("Log file not initialized"))?);

    let file = tokio::fs::File::open(path).await?;
    let rev_lines = RevLines::new_stream(BufReader::new(file), start).await?;
    pin_mut!(rev_lines);

    let mut result = Vec::new();
    while let Some(line) = rev_lines.next().await {
        if result.len() >= limit {
            break;
        }
        let line = line?;
        if let Some(end) = end
            && line.position <= end
        {
            break;
        }
        if !line.line.is_empty() {
            result.push(line);
        }
    }

    Ok(result.into_iter().map(LogLine::from).collect())
}

pub fn init_trace_log(name: &str) -> Result<()> {
    let log_file = format!("{}{}", name, LOG_FILE_NAME_EXT);
    if let Some(prev_file) = LOG_FILE.get() {
        if prev_file != &log_file {
            bail!("Log file already initialized with a different name: {}", prev_file);
        }
        return Ok(());
    } else {
        LOG_FILE.set(log_file.clone()).map_err(anyhow::Error::msg)?;
    }

    let app_dir = temp_dir();
    let path = app_dir.join(log_file);

    if path.exists() {
        // rename old file
        rename(&path, app_dir.join(format!("{}{}", name, PREV_FILE_NAME_EXT)))?;
    } else {
        create_dir_all(&app_dir)?;
    }

    let stdout_log = tracing_subscriber::fmt::layer().compact();

    let file = File::create(path)?;
    let app_log = tracing_subscriber::fmt::layer().json().with_writer(Arc::new(file));

    // Keep noisy dependencies quieter in normal operation.
    let hickory_filter = filter::Targets::new()
        .with_target("hickory_server", filter::LevelFilter::ERROR)
        .with_target("tarpc", filter::LevelFilter::WARN)
        .with_default(filter::LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(stdout_log.with_filter(hickory_filter.clone()))
        .with(app_log.with_filter(hickory_filter.clone()))
        .init();

    Ok(())
}
