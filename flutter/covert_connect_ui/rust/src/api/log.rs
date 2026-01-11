use anyhow::Result;
use std::env::temp_dir;
use std::{
    fs::{create_dir_all, rename, File},
    sync::Arc,
};
use tokio::io::BufReader;
use tracing_subscriber::{filter, prelude::*};
use tokio_rev_lines::RevLines;
use futures_util::{pin_mut, StreamExt};

#[cfg(debug_assertions)]
const LOG_FILE_NAME: &str = "covert-connect.debug.log";
#[cfg(debug_assertions)]
const PREV_FILE_NAME: &str = "covert-connect.debug.log.old";

#[cfg(not(debug_assertions))]
const LOG_FILE_NAME: &str = "covert-connect.log";
#[cfg(not(debug_assertions))]
const PREV_FILE_NAME: &str = "covert-connect.log.old";

pub async fn get_trace_log(limit: usize) -> Result<Vec<String>> {
    let path = temp_dir().join(LOG_FILE_NAME);

    // TODO: implement custom buffered reverse reader
    // return lines with pos/id (start position in file)
    // it helps to read only new lines and merge them

    let file = tokio::fs::File::open(path).await?;
    let rev_lines = RevLines::new(BufReader::new(file)).await?;
    pin_mut!(rev_lines);

    let mut result = Vec::new();
    while let Some(line) = rev_lines.next().await {
        if result.len() >= limit {
            break;
        }
        result.push(line?);
    }
    
    Ok(result)
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
    let app_log = tracing_subscriber::fmt::layer().compact().with_writer(Arc::new(file));

    tracing_subscriber::registry()
        .with(
            stdout_log
                .with_filter(filter::LevelFilter::INFO)
                .and_then(app_log)
                .with_filter(filter::LevelFilter::INFO),
        )
        .init();

    Ok(())
}
