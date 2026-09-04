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
//!
//! ## Why a level can only ever add
//!
//! There is a shape of logging level that quietly loses the one line somebody
//! needed: a threshold with levels *below* the default, so turning the dial
//! down removes the explanation of a crash along with the noise. This has no
//! such level. [`Level::Normal`] is the floor and is what everything already
//! written uses, including the panic hook; [`Level::Detailed`] is the only
//! other setting and it only turns extra lines **on**.
//!
//! So no setting anywhere can suppress a panic, a failure or the startup
//! banner, and that is a property of the type rather than of anybody
//! remembering. `nothing_can_filter_out_the_line_that_explains_a_crash` walks
//! every variant and would fail the moment a quieter one appeared.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

/// Bytes the log may reach before it is rotated.
///
/// One rotation, not a numbered series: the useful question is always "what
/// happened just now", and the previous file is enough history for that.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Bytes of crash history kept.
///
/// Small, because a crash file with a hundred crashes in it is a file nobody
/// reads. Restarted rather than rotated: the crash worth having is the one
/// that just happened, and there is no second file to look in.
const CRASH_MAX_BYTES: u64 = 256 * 1024;

static FILE: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();
static PATH: OnceLock<PathBuf> = OnceLock::new();

/// How much the log is asked to say. See the module note on why it only adds.
static LEVEL: AtomicU8 = AtomicU8::new(Level::Normal as u8);

/// How much the log is asked to say.
///
/// Two settings and no more, and **the quieter one is the default**. See the
/// module note: a level below the default is a way to lose the line that
/// explains a crash, so there isn't one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    /// What Sill did, and everything that went wrong. Never suppressed.
    #[default]
    Normal = 0,
    /// Every step of it, for reproducing a fault somebody is chasing.
    ///
    /// Off by default because these are the lines that run per keystroke and
    /// per summon, and a log that fills with them rotates away the hour before
    /// the one somebody wanted.
    Detailed = 1,
}

impl Level {
    /// Every level there is, so a test can walk them.
    ///
    /// The point of the list is the panic test: it asserts a panic is written
    /// at each of these, so adding a level quieter than `Normal` fails there
    /// rather than in somebody's missing crash report.
    pub const ALL: [Level; 2] = [Level::Normal, Level::Detailed];

    pub fn name(self) -> &'static str {
        match self {
            Level::Normal => "normal",
            Level::Detailed => "detailed",
        }
    }
}

/// Sets how much is written from here on.
pub fn set_level(level: Level) {
    LEVEL.store(level as u8, Ordering::Relaxed);
}

/// How much is being written.
pub fn level() -> Level {
    if LEVEL.load(Ordering::Relaxed) == Level::Detailed as u8 {
        Level::Detailed
    } else {
        Level::Normal
    }
}

/// Whether a line of this level is written right now.
///
/// **The only filter there is.** [`write`] does not consult it, so a panic, a
/// failure, or the startup banner cannot be filtered out by any setting; only
/// `detail!` asks, and it asks about [`Level::Detailed`]. That is what
/// `nothing_can_filter_out_the_line_that_explains_a_crash` holds, and it holds
/// it for every level rather than for the default.
pub fn wants(line: Level) -> bool {
    line <= level()
}

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

    // Whatever a previous run left behind still counts towards the limit.
    WRITTEN.store(
        std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0),
        Ordering::Relaxed,
    );

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

/// Where the last crash is written, once the log has been opened.
///
/// Beside the log rather than inside it. The log is two megabytes of ordinary
/// operation and rotates, so a panic from this morning is gone by lunchtime;
/// this file holds the one thing nobody can reproduce on request, and holds it
/// until the next one.
pub fn crash_path() -> Option<PathBuf> {
    PATH.get().map(|path| path.with_file_name("sill.crash.log"))
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

        // Before `open` there is no crash path, which is every test and the
        // first instants of startup. The log line above is what those get.
        if let Some(path) = crash_path() {
            record_crash(
                &path,
                &at,
                &said,
                &std::backtrace::Backtrace::force_capture(),
            );
        }

        previous(info);
    }));
}

/**
Writes the crash somewhere it will still be by the time anybody asks.

The log line above says what and where, which is enough to recognise a crash
and not enough to fix one: it has no stack behind it, and it is written into a
file that rotates past it. So the same panic is written a second time, with a
backtrace, into a file of its own that ordinary operation never touches. That
is the file the export bundle carries.

`force_capture` rather than `capture`, because `capture` respects
`RUST_BACKTRACE`, which is unset for everybody who is not a developer, which is
everybody whose crash we would like to see. The cost of forcing it is paid once
per process, in the moment the process is already ending.
*/
fn record_crash(path: &std::path::Path, at: &str, said: &str, trace: &std::backtrace::Backtrace) {
    // Restarted rather than appended to once it is large. A crash file is read
    // from the top and the most recent crash is the one worth reading, so the
    // failure mode to avoid is a file so long nobody opens it.
    let full = std::fs::metadata(path).is_ok_and(|meta| meta.len() > CRASH_MAX_BYTES);

    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(!full)
        .write(true)
        .truncate(full)
        .open(path)
    else {
        return;
    };

    let _ = writeln!(
        file,
        "--- Sill {} panicked at {} ---\n{at}\n{said}\n{trace}\n",
        env!("CARGO_PKG_VERSION"),
        stamp(),
    );
}

/// Appends one line, with a timestamp.
///
/// Silent on failure. A launcher that fell over because it could not write to
/// its own log would be a poor trade for the diagnostics.
/// Bytes written since the file was opened.
///
/// Counted rather than asked of the filesystem, because asking would be a
/// syscall per line and the answer only has to be close enough to catch a file
/// growing without end.
static WRITTEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn write(line: &str) {
    let Some(slot) = FILE.get() else {
        // Before `open`, which is everything that happens in a test.
        return;
    };
    let Ok(mut held) = slot.lock() else {
        return;
    };
    let Some(file) = held.as_mut() else {
        return;
    };

    let stamped = format!("{} {line}", stamp());
    let _ = writeln!(file, "{stamped}");
    let _ = file.flush();

    /*
     * Rotated here as well as at `open`.
     *
     * Rotating only at startup bounds the file across runs and not within
     * one. Sill is meant to run for weeks, and a run that logs steadily, or
     * one thing that logs in a loop, grows a single file for as long as the
     * machine is up. The comment on `open` said this was what stopped a long
     * run growing one file forever, and it stopped a long *sequence of runs*
     * doing it.
     */
    let grown = WRITTEN.fetch_add(stamped.len() as u64 + 1, Ordering::Relaxed);
    if grown < MAX_BYTES {
        return;
    }

    let Some(path) = PATH.get() else {
        return;
    };

    // Closed before the rename: Windows will not rename a file that is still
    // open for writing, and a failed rename here would mean writing into a
    // file nobody will ever look at.
    *held = None;

    let _ = std::fs::rename(path, path.with_file_name("sill.previous.log"));

    *held = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok();

    WRITTEN.store(0, Ordering::Relaxed);
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

/// Says something only when somebody has asked for detail.
///
/// For the lines that run per keystroke, per summon and per extension load:
/// worth having while a fault is being chased and not worth two megabytes of
/// log the rest of the time. **The level is checked before the arguments are
/// formatted**, so a call on a hot path costs one relaxed atomic load when the
/// setting is off.
#[macro_export]
macro_rules! detail {
    ($($arg:tt)*) => {{
        if $crate::log::wants($crate::log::Level::Detailed) {
            let line = format!($($arg)*);
            eprintln!("[sill] {}", line);
            $crate::log::write(&line);
        }
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

    /// A run that logs steadily does not grow one file forever.
    ///
    /// Rotating only at `open` bounds the file across runs and not within one,
    /// and Sill is meant to run for weeks. The comment on `open` said this was
    /// what stopped a long run growing one file forever; it stopped a long
    /// sequence of runs doing it.
    #[test]
    fn the_log_rotates_while_the_application_is_still_running() {
        let dir = tempfile::tempdir().expect("temp dir");
        open(dir.path());

        let path = dir.path().join("sill.log");
        let previous = dir.path().join("sill.previous.log");

        // A line long enough that a few hundred of them pass the limit
        // without this taking a noticeable moment.
        let line = "x".repeat(4_096);
        let lines = (MAX_BYTES / 4_096) + 2;

        for _ in 0..lines {
            write(&line);
        }

        assert!(
            previous.exists(),
            "the log never rotated, so one file grows for as long as Sill runs"
        );
        assert!(
            std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0) < MAX_BYTES,
            "the live log is still over the limit after rotating"
        );
    }

    /**
    No level anywhere can filter out the line that explains a crash.

    The failure this is against is a familiar one: somebody turns logging down
    to stop the noise, the application falls over a week later, and the only
    record of why had been filtered out by the setting. `write` is what the
    panic hook, every failure and the startup banner go through, and it does
    not consult the level at all; `wants` is the whole filter, and this asks it
    at **every** level there is rather than at the default.

    So adding a level quieter than `Normal` fails here, instead of in a crash
    report that never arrives.
    */
    #[test]
    fn nothing_can_filter_out_the_line_that_explains_a_crash() {
        for level in Level::ALL {
            set_level(level);

            assert!(
                wants(Level::Normal),
                "an ordinary line is filtered out at the {} level, so a panic \
                 written there would be lost",
                level.name()
            );
        }

        set_level(Level::default());
    }

    /// Detail is off unless somebody asked for it, and asking only adds.
    #[test]
    fn detail_is_the_only_thing_a_level_turns_on() {
        set_level(Level::Normal);
        assert!(
            !wants(Level::Detailed),
            "detail is on without being asked for"
        );

        set_level(Level::Detailed);
        assert!(wants(Level::Detailed));
        assert!(
            wants(Level::Normal),
            "turning detail on stopped an ordinary line, so the levels subtract"
        );

        set_level(Level::default());
    }

    /// The default is the quiet one, and it is also the floor.
    #[test]
    fn the_default_level_is_the_one_that_cannot_hide_anything() {
        assert_eq!(Level::default(), Level::Normal);
        assert_eq!(
            Level::ALL.iter().copied().min(),
            Some(Level::Normal),
            "there is a level below Normal now, so something can be filtered \
             out of a crash report"
        );
    }

    /// A crash leaves a file of its own, with the stack the log line has not.
    #[test]
    fn a_crash_is_written_where_the_log_cannot_rotate_past_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sill.crash.log");

        record_crash(
            &path,
            "src/somewhere.rs:12:9",
            "the thing that went wrong",
            &std::backtrace::Backtrace::force_capture(),
        );

        let written = std::fs::read_to_string(&path).unwrap_or_default();

        assert!(written.contains("src/somewhere.rs:12:9"));
        assert!(written.contains("the thing that went wrong"));
        assert!(
            written.contains("panicked at"),
            "the crash file does not say what it is: {written}"
        );
    }

    /// A second crash is kept beside the first rather than replacing it.
    #[test]
    fn a_second_crash_does_not_erase_the_first() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sill.crash.log");
        let trace = std::backtrace::Backtrace::force_capture();

        record_crash(&path, "a.rs:1:1", "the first one", &trace);
        record_crash(&path, "b.rs:2:2", "the second one", &trace);

        let written = std::fs::read_to_string(&path).unwrap_or_default();

        assert!(
            written.contains("the first one"),
            "the earlier crash was overwritten, and the earlier one is usually \
             the one that caused the later"
        );
        assert!(written.contains("the second one"));
    }

    /// It does not grow forever either. A crash file nobody opens is no file.
    #[test]
    fn a_crash_file_that_got_long_starts_again() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sill.crash.log");

        std::fs::write(&path, "x".repeat(CRASH_MAX_BYTES as usize + 1)).expect("write");

        record_crash(
            &path,
            "a.rs:1:1",
            "the one after the file got long",
            &std::backtrace::Backtrace::force_capture(),
        );

        let written = std::fs::read_to_string(&path).unwrap_or_default();

        assert!(written.contains("the one after the file got long"));
        assert!(
            (written.len() as u64) < CRASH_MAX_BYTES,
            "the crash file is still over the limit, so it grows without end"
        );
    }
}
