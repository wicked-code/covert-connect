use std::env::temp_dir;
use std::fs::{File, create_dir_all, rename};
use std::io::Write;
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};

#[cfg(debug_assertions)]
const LOG_FILE_NAME: &str = "covert-connect-tray.debug.log";
#[cfg(debug_assertions)]
const PREV_FILE_NAME: &str = "covert-connect-tray.debug.log.old";

#[cfg(not(debug_assertions))]
const LOG_FILE_NAME: &str = "covert-connect-tray.log";
#[cfg(not(debug_assertions))]
const PREV_FILE_NAME: &str = "covert-connect-tray.log.old";

struct FileLogger {
    level: LevelFilter,
    file: Mutex<File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(file, "{:<5} {}: {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

fn level_from_env() -> LevelFilter {
    match std::env::var("RUST_LOG").ok().as_deref().map(str::to_ascii_lowercase) {
        Some(s) => match s.as_str() {
            "off" => LevelFilter::Off,
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => LevelFilter::Info,
        },
        None => LevelFilter::Info,
    }
}

fn init_log_file() -> std::io::Result<File> {
    let app_dir = temp_dir();
    let path = app_dir.join(LOG_FILE_NAME);

    if path.exists() {
        let prev = app_dir.join(PREV_FILE_NAME);
        rename(&path, &prev)?;
    } else {
        create_dir_all(&app_dir)?;
    }

    File::create(path)
}

/// Install the file logger as the global `log` backend.
pub fn init() {
    let level = level_from_env();

    let file = match init_log_file() {
        Ok(file) => file,
        Err(_) => {
            log::set_max_level(level);
            return;
        }
    };

    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger {
        level,
        file: Mutex::new(file),
    }));

    if log::set_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}
