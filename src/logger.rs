//! Minimal file logger. The DLL runs inside host processes (Chrome, PowerShell)
//! where stdout is useless, so everything of interest goes to engine.log in the
//! assets directory.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_dir(dir: &std::path::Path) {
    *LOG_PATH.lock().unwrap() = Some(dir.join("engine.log"));
}

pub fn log(msg: &str) {
    let guard = LOG_PATH.lock().unwrap();
    let Some(path) = guard.as_ref() else { return };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}

macro_rules! elog {
    ($($arg:tt)*) => { crate::logger::log(&format!($($arg)*)) };
}
pub(crate) use elog;
