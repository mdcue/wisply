//! The log file. Everything the app wants to record goes through the `log` crate
//! (`log::info!("...")`, `log::warn!`, `log::error!`) and lands in one file with a timestamp.
//!
//! Like the Python app: one shared file, written to by several threads without lines mixing,
//! and when it passes 1 MB the old one is kept as `.old` and a fresh one is started.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};

use crate::Result;

/// While the Python app is still around it keeps `wisply.log`, so the new app writes its own file.
/// At the parity checkpoint (R7) this becomes `wisply.log`.
pub const LOG_NAME: &str = "wisply-rs.log";
pub const MAX_BYTES: u64 = 1_000_000;

pub struct FileLogger {
    file: Mutex<File>,
}

/// If the log is bigger than `max_bytes`, moves it to `<name>.old` (replacing an older one).
pub fn rotate_if_big(path: &Path, max_bytes: u64) -> Result<()> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > max_bytes {
            let mut old = path.as_os_str().to_owned();
            old.push(".old");
            fs::rename(path, PathBuf::from(old))?;
        }
    }
    Ok(())
}

impl FileLogger {
    /// Opens (and if needed rotates) the log file inside `dir`.
    pub fn open(dir: &Path) -> Result<FileLogger> {
        let path = dir.join(LOG_NAME);
        rotate_if_big(&path, MAX_BYTES)?;
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(FileLogger { file: Mutex::new(file) })
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= LevelFilter::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // A message can't start a fake log line: line breaks are written as text.
        let message = record.args().to_string().replace('\r', "\\r").replace('\n', "\\n");
        let line = format!("{} {:<5} {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), record.level(), message);
        // If another thread panicked while writing, the lock is "poisoned"; logging carries on anyway.
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }

    fn flush(&self) {
        let mut file = self.file.lock().unwrap_or_else(|e| e.into_inner());
        let _ = file.flush();
    }
}

/// Starts logging to the file inside `dir`, and makes a crash (a panic) leave a line in it.
/// Safe to call twice: only the first call takes effect.
pub fn init(dir: &Path) -> Result<()> {
    let logger = FileLogger::open(dir)?;
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Info);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("the app crashed: {info}");
            previous(info);
        }));
        log::info!("---- Wisply started ----");
    }
    Ok(())
}