//! Watching the clipboard.
//!
//! Windows delivers a message every time the clipboard changes; the listening
//! is [`clipboard_master`], MIT, which owns the message-only window and the
//! `AddClipboardFormatListener` registration. Reading the contents is
//! [`arboard`], MIT and Apache, which is 1Password's and handles the format
//! negotiation and image decoding.
//!
//! What is written here is the part neither crate has an opinion about: what
//! is worth keeping, what must never be kept, and how to tell one copy from
//! the same copy again.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::clipboard::kind::{classify, Kind};
use crate::clipboard::store::Store;

/// Longest text kept.
///
/// Copying an entire log file should not put sixty megabytes in a history
/// row, and nobody is going to find it again by searching for a word in it.
const MAX_TEXT_BYTES: usize = 1_000_000;

/// Largest image kept, before which it is noted but its bytes are dropped.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// How many times a read is attempted before the change is given up on.
///
/// The clipboard is a single system-wide resource held under a lock, and the
/// application that just copied is usually **still holding it** at the moment
/// Windows announces the change. A single attempt therefore loses entries at
/// random, which is the classic clipboard-manager bug. Every mature one
/// retries; this spreads eight attempts over about a quarter of a second.
const READ_ATTEMPTS: u32 = 8;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(30);

/// Shared handle to the running history.
#[derive(Clone)]
pub struct Clipboard {
    store: Arc<Mutex<Store>>,
    /// Set while Sill is itself writing to the clipboard, so a paste out of
    /// the history does not come straight back in as a new entry.
    ignoring: Arc<AtomicUsize>,
    watching: Arc<AtomicBool>,
    /// The settings the watcher needs, cached here because it runs on its own
    /// thread with no route back to the preference store.
    rules: Arc<Mutex<Rules>>,
}

/// What the watcher is allowed to record.
#[derive(Debug, Clone, Default)]
pub struct Rules {
    /// Record at all.
    ///
    /// Checked on every capture rather than by stopping the listener: the
    /// watcher owns a thread with a message pump, and tearing that down and
    /// standing it back up as a setting is toggled is far more machinery
    /// than declining to write.
    pub enabled: bool,
    pub keep_images: bool,
    pub ignored_apps: Vec<String>,
}

impl Clipboard {
    pub fn open(app: &AppHandle) -> Option<Self> {
        let path = app
            .path()
            .app_data_dir()
            .ok()?
            .join("clipboard.db");

        match Store::open(&path) {
            Ok(store) => Some(Self {
                store: Arc::new(Mutex::new(store)),
                ignoring: Arc::new(AtomicUsize::new(0)),
                watching: Arc::new(AtomicBool::new(false)),
                rules: Arc::new(Mutex::new(Rules {
                    enabled: true,
                    keep_images: true,
                    ignored_apps: Vec::new(),
                })),
            }),
            Err(err) => {
                crate::say!("could not open the clipboard history: {err}");
                None
            }
        }
    }

    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        // Poisoning would mean a panic mid-write. The history is not worth
        // taking the app down over, and the next write repairs the state.
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Marks the next clipboard change as Sill's own.
    ///
    /// Pasting from the history writes to the clipboard, which the listener
    /// sees. Without this the entry would be re-recorded and jump to the top
    /// of the list every time it was used, reordering the history under the
    /// user's hands.
    pub fn ignore_next(&self) {
        self.ignore_next_changes(1);
    }

    /// Marks the next `count` clipboard changes as Sill's own.
    ///
    /// A count rather than a flag because borrowing the clipboard takes two
    /// changes, not one: reading a selection copies into it and then puts the
    /// previous contents back. With a flag the restore was recorded, so every
    /// transformation left the user's own older clipboard entry sitting at the
    /// top of the history as though they had just copied it.
    pub fn ignore_next_changes(&self, count: usize) {
        self.ignoring.fetch_add(count, Ordering::SeqCst);
    }

    /// Cancels changes that were expected and did not happen.
    ///
    /// A capture that finds nothing selected never writes to the clipboard, so
    /// the two it reserved would otherwise swallow the user's next two real
    /// copies.
    pub fn forget_ignored(&self) {
        self.ignoring.store(0, Ordering::SeqCst);
    }

    fn take_ignore(&self) -> bool {
        self.ignoring
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_sub(1)
            })
            .is_ok()
    }

    pub fn set_rules(&self, rules: Rules) {
        if let Ok(mut current) = self.rules.lock() {
            *current = rules;
        }
    }

    fn rules(&self) -> Rules {
        self.rules
            .lock()
            .map(|rules| rules.clone())
            .unwrap_or_default()
    }
}

/// Whether an application's copies are to be left alone.
///
/// Compared case-insensitively on a substring, so "chrome" covers every
/// Chrome window without anyone having to know the executable's exact name.
pub fn is_ignored(app: Option<&str>, ignored: &[String]) -> bool {
    let Some(app) = app else {
        // Nothing known about the source, so no rule can exclude it. The
        // alternative, refusing everything unattributable, would silently
        // record nothing on a machine where the foreground read fails.
        return false;
    };

    let app = app.to_lowercase();
    ignored
        .iter()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .any(|name| app.contains(&name))
}

/// Starts watching, if it is not already.
///
/// The listener owns a thread with a message pump, which is what
/// `AddClipboardFormatListener` requires: the message arrives on the thread
/// that registered, and only while that thread pumps.
pub fn watch(app: &AppHandle, clipboard: &Clipboard) {
    if clipboard.watching.swap(true, Ordering::SeqCst) {
        return;
    }

    let app = app.clone();
    let clipboard = clipboard.clone();

    std::thread::Builder::new()
        .name("clipboard-watch".to_string())
        .spawn(move || {
            let handler = Handler { app, clipboard };
            match clipboard_master::Master::new(handler) {
                Ok(mut master) => {
                    if let Err(err) = master.run() {
                        crate::say!("the clipboard watcher stopped: {err}");
                    }
                }
                Err(err) => crate::say!("could not start the clipboard watcher: {err}"),
            }
        })
        .ok();
}

struct Handler {
    app: AppHandle,
    clipboard: Clipboard,
}

impl clipboard_master::ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) -> clipboard_master::CallbackResult {
        if self.clipboard.take_ignore() {
            return clipboard_master::CallbackResult::Next;
        }

        if let Err(err) = capture(&self.app, &self.clipboard) {
            crate::say!("could not record a clipboard change: {err}");
        }
        clipboard_master::CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> clipboard_master::CallbackResult {
        // Another application holding the clipboard open is ordinary and
        // transient; giving up on the watcher over it would be wrong.
        crate::say!("clipboard listener error: {error}");
        clipboard_master::CallbackResult::Next
    }
}

/// Reads whatever is on the clipboard now and records it.
fn capture(app: &AppHandle, clipboard: &Clipboard) -> Result<(), String> {
    if is_confidential() {
        // A password manager said so. Nothing is read, so nothing can leak.
        return Ok(());
    }

    let rules = clipboard.rules();
    if !rules.enabled {
        return Ok(());
    }

    let source = crate::dictation::context::foreground_app_full();

    let name = source.as_ref().map(|app| app.name.clone());
    let exe = source.as_ref().map(|app| app.path.clone());

    if is_ignored(name.as_deref(), &rules.ignored_apps) {
        return Ok(());
    }

    let Some(mut board) = open_clipboard() else {
        return Err("the clipboard stayed locked by another application".to_string());
    };
    let now = now_seconds();

    if let Some(text) = read_text(&mut board) {
        let trimmed = text.trim();
        // Whitespace is what a text field leaves behind when it is cleared,
        // not something anyone meant to copy.
        if trimmed.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Ok(());
        }

        let kind = classify(&text);
        let store = clipboard.store();
        store
            .record(
                &hash(text.as_bytes()),
                kind,
                &text,
                name.as_deref(),
                exe.as_deref(),
                text.len() as i64,
                now,
            )
            .map_err(|e| e.to_string())?;
        drop(store);

        let _ = app.emit("clipboard:changed", ());
        return Ok(());
    }

    if !rules.keep_images {
        return Ok(());
    }

    if let Ok(image) = board.get_image() {
        let bytes = image.bytes.len();
        let label = format!("Image {}x{}", image.width, image.height);
        let store = clipboard.store();

        // Hashed over the pixels, so the same screenshot copied twice is one
        // entry rather than two identically named ones.
        let id = store
            .record(
                &hash(&image.bytes),
                Kind::Image,
                &label,
                name.as_deref(),
                exe.as_deref(),
                bytes as i64,
                now,
            )
            .map_err(|e| e.to_string())?;

        if bytes <= MAX_IMAGE_BYTES {
            if let Some(png) = encode_png(&image) {
                let _ = store.put_blob(id, &png);
            }
        }
        drop(store);

        let _ = app.emit("clipboard:changed", ());
    }

    Ok(())
}

/// Opens the clipboard, waiting out whoever else has it.
fn open_clipboard() -> Option<arboard::Clipboard> {
    for attempt in 0..READ_ATTEMPTS {
        match arboard::Clipboard::new() {
            Ok(board) => return Some(board),
            Err(err) => {
                if attempt + 1 == READ_ATTEMPTS {
                    crate::say!("could not open the clipboard after {READ_ATTEMPTS} tries: {err}");
                    return None;
                }
                std::thread::sleep(RETRY_DELAY);
            }
        }
    }
    None
}

/// Reads text, retrying a locked clipboard but not an absent format.
///
/// The distinction is the point. `ContentNotAvailable` means there is no text
/// on the clipboard, which is a fact and answered immediately so an image can
/// be tried instead. Anything else means the read failed, which is worth
/// waiting out.
fn read_text(board: &mut arboard::Clipboard) -> Option<String> {
    for attempt in 0..READ_ATTEMPTS {
        match board.get_text() {
            Ok(text) => return Some(text),
            Err(arboard::Error::ContentNotAvailable) => return None,
            Err(_) if attempt + 1 == READ_ATTEMPTS => return None,
            Err(_) => std::thread::sleep(RETRY_DELAY),
        }
    }
    None
}

/// Whether the owner of the clipboard asked for its contents not to be kept.
///
/// Password managers register `ExcludeClipboardContentFromMonitorProcessing`
/// and `CanIncludeInClipboardHistory` for exactly this. Honouring them is not
/// optional politeness: a clipboard manager that records what 1Password just
/// put on the clipboard has written the password to disk in plain text.
#[cfg(windows)]
fn is_confidential() -> bool {
    use windows::core::w;
    use windows::Win32::System::DataExchange::{
        IsClipboardFormatAvailable, RegisterClipboardFormatW,
    };

    // SAFETY: both take static wide literals and return plain values.
    unsafe {
        for name in [
            w!("ExcludeClipboardContentFromMonitorProcessing"),
            w!("CanIncludeInClipboardHistory"),
        ] {
            let format = RegisterClipboardFormatW(name);
            if format != 0 && IsClipboardFormatAvailable(format).is_ok() {
                return true;
            }
        }
    }

    false
}

#[cfg(not(windows))]
fn is_confidential() -> bool {
    false
}

/// PNG bytes for a clipboard image.
fn encode_png(image: &arboard::ImageData<'_>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&image.bytes).ok()?;
    }
    Some(out)
}

/// The deduplication key.
///
/// A hash rather than the text itself: SQLite will not index a value past a
/// few kilobytes, and copying a whole file is something people do.
pub fn hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_bytes_hash_the_same_and_different_bytes_do_not() {
        // This is what decides whether two copies are one entry.
        assert_eq!(hash(b"hello"), hash(b"hello"));
        assert_ne!(hash(b"hello"), hash(b"hello "));
    }

    #[test]
    fn the_default_rules_record_nothing_until_they_are_set() {
        // `Rules::default()` is what a poisoned lock falls back to, and
        // recording while the settings are unknown is the wrong direction to
        // fail in.
        assert!(!Rules::default().enabled);
    }

    #[test]
    fn an_ignored_application_is_matched_loosely() {
        // Nobody should have to know that Chrome's window is `chrome.exe`
        // and its helper is something else.
        let ignored = vec!["chrome".to_string(), "1Password".to_string()];

        assert!(is_ignored(Some("Google Chrome"), &ignored));
        assert!(is_ignored(Some("chrome"), &ignored));
        assert!(is_ignored(Some("1password"), &ignored), "case is ignored");
        assert!(!is_ignored(Some("Slack"), &ignored));
    }

    #[test]
    fn an_unknown_source_is_recorded_rather_than_refused() {
        // Refusing everything unattributable would silently record nothing
        // on a machine where the foreground read fails.
        assert!(!is_ignored(None, &["chrome".to_string()]));
    }

    #[test]
    fn a_blank_rule_does_not_exclude_everything() {
        // An empty row left in the editor matches every string.
        assert!(!is_ignored(Some("Slack"), &["".to_string(), "  ".to_string()]));
    }

    #[test]
    fn the_hash_is_short_enough_to_index() {
        // The whole reason it exists: SQLite will not index a long value,
        // and copying a whole file has to still deduplicate.
        assert_eq!(hash(&vec![0u8; 10_000_000]).len(), 64);
    }
}
