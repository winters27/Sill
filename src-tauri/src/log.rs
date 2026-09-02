//! Where Sill says what it is doing.
//!
//! A release build is compiled `windows_subsystem = "windows"`, which is what
//! stops a console flashing up beside the launcher. It also means **stderr
//! goes nowhere**: every diagnostic in the app was invisible in the only build
//! anyone actually runs, which made half the features impossible to diagnose
//! and the other half impossible to confirm.
//!
//! So the same line goes to both. `eprintln!` still works under `cargo run`,
//! and the file is what exists in a shipped build.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Bytes the log may reach before it is rotated.
///
/// One rotation, not a numbered series: the useful question is always "what
/// happened just now", and the previous file is enough history for that.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

static FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();
static PATH: OnceLock<PathBuf> = OnceLock::new();

/// Opens the log. Called once, as early in startup as the data directory is
/// known.
pub fn open(dir: &std::path::Path) {
    let path = dir.join("sill.log");
    let _ = std::fs::create_dir_all(dir);

    // Rotated before opening, so a long-running install does not grow one
    // file forever.
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join("sill.previous.log"));
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();

    let _ = PATH.set(path);
    let _ = FILE.set(Mutex::new(file));

    write(&format!(
        "--- Sill {} started ---",
        env!("CARGO_PKG_VERSION")
    ));
}

/// Where the log is, once it has been opened.
pub fn path() -> Option<&'static PathBuf> {
    PATH.get()
}

/**
Writes a panic to the log before the default handler runs.

Without this a panic in a release build is completely silent. The default hook
prints to stderr, and the note at the top of this module explains why stderr
goes nowhere here: the whole point of `windows_subsystem = "windows"` is that
there is no console. So the one event that most needs to leave a trace was the
one event that left none, and the report was always "it just stopped".

Chained rather than replacing: the default hook still runs, so `cargo run` and
the test binaries keep the output and the backtrace they already had.

The payload is read the way the standard library reads it, because a panic
carries either a `&str` or a `String` depending on whether it was formatted,
and a hook that only handles one of them loses exactly the messages that had
something to say.
*/
pub fn catch_panics() {
    let previous = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let said = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no message".to_string());

        let at = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "an unknown place".to_string());

        write(&format!("PANIC at {at}: {said}"));

        previous(info);
    }));
}

/// Appends one line, with a timestamp.
///
/// Silent on failure. A launcher that fell over because it could not write to
/// its own log would be a poor trade for the diagnostics.
pub fn write(line: &str) {
    let Some(slot) = FILE.get() else {
        // Before `open`, which is everything that happens in a test.
        return;
    };
    let Ok(mut file) = slot.lock() else {
        return;
    };
    let Some(file) = file.as_mut() else {
        return;
    };

    let _ = writeln!(file, "{} {line}", stamp());
    let _ = file.flush();
}

/// `HH:MM:SS` in local time.
///
/// Only the time, not the date: the file is rotated and read within a session,
/// and a full timestamp on every line makes it harder to scan.
#[cfg(windows)]
fn stamp() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: fills an owned struct and takes nothing.
    let now = unsafe { GetLocalTime() };
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        now.wHour, now.wMinute, now.wSecond, now.wMilliseconds
    )
}

#[cfg(not(windows))]
fn stamp() -> String {
    String::new()
}

/// Says something, to stderr and to the log.
///
/// `eprintln!` alone is invisible in a release build; the file alone is
/// invisible under `cargo run`. Both is the only combination that is useful
/// in both.
#[macro_export]
macro_rules! say {
    ($($arg:tt)*) => {{
        let line = format!($($arg)*);
        eprintln!("[sill] {}", line);
        $crate::log::write(&line);
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_before_the_log_is_open_is_harmless() {
        // Every unit test runs in this state, and a panic here would take the
        // whole suite with it.
        write("nothing should happen");
    }

    #[cfg(windows)]
    #[test]
    fn the_stamp_is_the_shape_it_promises() {
        let stamp = stamp();
        assert_eq!(stamp.len(), 12, "HH:MM:SS.mmm, got {stamp}");
        assert_eq!(stamp.matches(':').count(), 2);
    }
}
