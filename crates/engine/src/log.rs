//! Exit diagnostics: every line goes to stderr and to `~/.ferrocraft/engine.log`.
//! The launcher may drop the engine's stderr, so the file is the record of why the engine stopped.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn line(msg: &str) {
    eprintln!("ferrocraft-engine: {msg}");
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return;
    };
    let dir = std::path::Path::new(&home).join(".ferrocraft");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("engine.log"))
    {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let _ = writeln!(f, "[{secs}] pid {}: {msg}", std::process::id());
    }
}

/// Logs panics with their location; the default hook prints to a stderr that may be lost.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| line(&format!("panic: {info}"))));
}
