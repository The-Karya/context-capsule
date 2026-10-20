// context-capsule — core: app.log in the data folder — one line per notable thing (recording started /
// ended and why, voice-note numbers and error codes). Never screen text, window titles or spoken words.
//! A port of the Swift `AppLog`. Kept under `MAX_BYTES`: when it grows past that, the newer half stays.
//! Every line also goes to stderr (the terminal, when started from one).

use crate::clock;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// One writer at a time within this process (the engine, the UI and the voice threads all log).
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppLog {
    path: Option<PathBuf>,
}

impl AppLog {
    pub const MAX_BYTES: u64 = 256 * 1024;

    pub fn new(path: impl Into<PathBuf>) -> AppLog {
        AppLog { path: Some(path.into()) }
    }

    /// stderr only (no file).
    pub fn stderr_only() -> AppLog {
        AppLog { path: None }
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn write(&self, message: &str) {
        self.write_at(message, SystemTime::now());
    }

    pub fn write_at(&self, message: &str, at: SystemTime) {
        eprintln!("Context Capsule: {message}");
        let Some(path) = &self.path else { return };
        let line = format!("{} {}\n", clock::iso(at), message.replace(['\n', '\r'], " "));
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = Self::append(&line, path);
    }

    /// Append one line; first trim the file to its newer half (from a line start) if it is too big.
    pub fn append(line: &str, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > Self::MAX_BYTES {
            let data = std::fs::read(path)?;
            let keep = (Self::MAX_BYTES / 2) as usize;
            let mut tail = &data[data.len().saturating_sub(keep)..];
            if let Some(nl) = tail.iter().position(|b| *b == b'\n') {
                tail = &tail[nl + 1..];
            }
            crate::json::write_atomically(path, &String::from_utf8_lossy(tail))?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        f.write_all(line.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn lines_and_cap() {
        let dir = std::env::temp_dir().join(format!("cc-applog-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("logs").join("app.log");
        let log = AppLog::new(&path);
        let at = UNIX_EPOCH + Duration::from_secs(1_792_404_063);
        log.write_at("recording started: x", at);
        log.write_at("two\nlines", at);
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "2026-10-19T10:01:03Z recording started: x\n2026-10-19T10:01:03Z two lines\n");

        let big = format!("{}\n", "x".repeat(1000));
        for _ in 0..300 {
            AppLog::append(&big, &path).unwrap();
        }
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size <= AppLog::MAX_BYTES + 2000, "stays small: {size}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.lines().all(|l| l.len() == 1000), "trimmed at a line start");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stderr_only_writes_no_file() {
        let log = AppLog::stderr_only();
        assert_eq!(log.path(), None);
        log.write("nothing on disk");
    }
}
