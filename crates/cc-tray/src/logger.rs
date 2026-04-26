//! Tiny `log` backend for the tray binary.
//!
//! Writes one line per record to stderr.
//! Defaults to `info`.

use std::io::IsTerminal;

use log::{Level, LevelFilter, Log, Metadata, Record};

struct StderrLogger {
    level: LevelFilter,
    color: bool,
}

// ANSI SGR codes.
const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const DIM: &str = "\x1b[2m";

fn level_color(level: Level) -> &'static str {
    match level {
        Level::Error => RED,
        Level::Warn => YELLOW,
        Level::Info => GREEN,
        Level::Debug => BLUE,
        Level::Trace => DIM,
    }
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }
    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = record.level();
        if self.color {
            let color = level_color(level);
            eprintln!(
                "{color}{:<5}{RESET} {DIM}[{}]{RESET} {}",
                level,
                record.target(),
                record.args()
            );
        } else {
            eprintln!("{:<5} {}: {}", level, record.target(), record.args());
        }
    }
    fn flush(&self) {}
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

/// Install the stderr logger as the global `log` backend.
pub fn init() {
    let level = level_from_env();
    let color = std::io::stderr().is_terminal() && enable_ansi_on_windows();
    let logger: &'static StderrLogger = Box::leak(Box::new(StderrLogger { level, color }));
    let _ = log::set_logger(logger);
    log::set_max_level(level);
}

#[cfg(not(windows))]
fn enable_ansi_on_windows() -> bool {
    true
}

/// Enable VT processing on the stderr console so ANSI escapes render.
/// Returns `false` if stderr is not a real console (e.g. detached when
/// built with `windows_subsystem = "windows"`), which suppresses color.
#[cfg(windows)]
fn enable_ansi_on_windows() -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, SetConsoleMode,
    };

    let handle = std::io::stderr().as_raw_handle() as _;
    let mut mode: u32 = 0;
    // SAFETY: handle comes from a live stderr reference; pointer is to a
    // local u32 valid for the duration of the call.
    unsafe {
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}
