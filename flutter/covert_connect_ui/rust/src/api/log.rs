use anyhow::Result;
use futures_util::{StreamExt, pin_mut};
use std::env::temp_dir;
use std::{
    fs::{File, create_dir_all, rename},
    sync::Arc,
};
use tokio::io::BufReader;
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

pub async fn get_trace_log(start: Option<u64>, end: Option<u64>, limit: usize) -> Result<Vec<LogLine>> {
    let path = temp_dir().join(LOG_FILE_NAME);

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

pub fn init_trace_log() -> Result<()> {
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

    // only errors from hickory server
    let hickory_filter = filter::Targets::new()
        .with_target("hickory_server", filter::LevelFilter::ERROR)
        .with_default(filter::LevelFilter::INFO);

    tracing_subscriber::registry()
        .with(stdout_log.with_filter(hickory_filter.clone()))
        .with(app_log.with_filter(hickory_filter.clone()))
        .init();

    Ok(())
}
