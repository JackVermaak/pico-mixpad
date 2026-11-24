//! Minimal logger: a file next to the executable, plus the console when there is one.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use log::{Level, LevelFilter, Log, Metadata, Record};

const MAX_BYTES: u64 = 200_000;

struct Logger {
    file: Option<Mutex<File>>,
    console: bool,
}

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= Level::Info
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
        let line = format!("{h:02}:{m:02}:{s:02} UTC {} {}\n", r.level(), r.args());
        if let Some(f) = &self.file {
            if let Ok(mut f) = f.lock() {
                let _ = f.write_all(line.as_bytes());
            }
        }
        if self.console {
            eprint!("{line}");
        }
    }

    fn flush(&self) {}
}

pub fn init(path: &Path, console: bool) {
    if fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::rename(path, path.with_extension("log.1"));
    }
    let file = OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new);
    let logger = Box::leak(Box::new(Logger { file, console }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(LevelFilter::Info);
    }
}
