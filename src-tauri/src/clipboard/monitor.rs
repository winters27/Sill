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

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::clipboard::kind::{classify, Kind};
use crate::clipboard::sensitive;
use crate::clipboard::store::{Recording, Store};

/// Longest text kept.
///
/// Copying an entire log file should not put sixty megabytes in a history
/// row, and nobody is going to find it again by searching for a word in it.
const MAX_TEXT_BYTES: usize = 1_000_000;

/// Largest image kept, before which it is noted but its bytes are dropped.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// The one report about images not being kept, named once so the copy that
/// works withdraws the one that did not.
const IMAGE_TROUBLE: &str = "clipboard-image";

/// How often old entries are cleared out.
///
/// Once a day, from whichever copy happens to be the first after that. The
/// prune used to run only at startup, which on a machine that is left on
/// means it never runs: Sill is meant to be up for weeks, and the entry
/// somebody set to expire after seven days would still be there on the
/// thirtieth.
const PRUNE_EVERY: std::time::Duration = std::time::Duration::from_secs(60 * 60 * 24);

/// Clears out entries past their retention, at most once a day.
///
/// Called from the recording path rather than a timer, because a copy is the
/// only moment the history changes and a thread waking daily to find nothing
/// to do is exactly the idle cost rule 23 refuses.
fn prune_occasionally(clipboard: &Clipboard, retain_days: u32) {
    if retain_days == 0 {
        return;
    }

    {
        let Ok(mut last) = clipboard.pruned.lock() else {
            return;
        };

        if last.is_some_and(|when| when.elapsed() < PRUNE_EVERY) {
            return;
        }

        *last = Some(std::time::Instant::now());
    }

    match clipboard
        .store()
        .prune(retain_days, crate::state::now_seconds())
    {
        Ok(0) => {}
        Ok(gone) => crate::say!("pruned {gone} old clipboard entries"),
        Err(err) => crate::say!("could not prune the clipboard: {err}"),
    }
}

/// How many times the clipboard is reached for before giving up.
///
/// The clipboard is a single system-wide resource held under a lock, and the
/// application that just copied is usually **still holding it** at the moment
/// Windows announces the change. A single attempt therefore loses entries at
/// random, which is the classic clipboard-manager bug. Every mature one
/// retries; this spreads eight attempts over about a quarter of a second.
///
/// **Writes need this as much as reads do**, which is why it is shared rather
/// than local to the reader. Putting a borrowed clipboard back lost the same
/// race, silently, and the user's clipboard was gone.
pub(crate) const CLIPBOARD_ATTEMPTS: u32 = 8;
pub(crate) const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(30);

/// Shared handle to the running history.
#[derive(Clone)]
pub struct Clipboard {
    store: Arc<Mutex<Store>>,
    /// When entries past their retention were last cleared out.
    ///
    /// Held here rather than in a `static`, which is what rule 2 refuses: this
    /// struct is already the managed state for the clipboard and the fact
    /// belongs to it. It also means a test can have its own.
    pruned: Arc<Mutex<Option<std::time::Instant>>>,
    /// Set while Sill is itself writing to the clipboard, so a paste out of
    /// the history does not come straight back in as a new entry.
    ignoring: Arc<AtomicUsize>,
    /// Set while Sill is running a whole operation against the clipboard,
    /// rather than making one known write.
    ///
    /// A count cannot express this. Running a shortcut borrows the clipboard,
    /// runs an action that **also** writes to it, pastes, and puts the
    /// original back, and the action is free to write as often as it likes.
    /// Reserving a number up front means guessing that number, and guessing it
    /// wrong either records Sill's own writes as though the user had copied
    /// them or swallows the next thing the user really does copy.
    suspended: Arc<AtomicBool>,
    /// The most recent thing that was not recorded because it looked like a
    /// credential.
    ///
    /// Held here rather than announced and forgotten, because the launcher is
    /// hidden when almost every copy happens. An event alone would arrive with
    /// nobody listening, and the user would open the history later to find
    /// something quietly missing.
    ///
    /// Memory only, and never the value itself: what it looked like and how
    /// long it was. The value is still on the clipboard, which is what makes
    /// keeping it afterwards possible without storing it twice.
    skipped: Arc<Mutex<Option<Skipped>>>,
    watching: Arc<AtomicBool>,
    /// How to tell the watcher thread to finish.
    ///
    /// The watcher owns a thread with a message pump and a hidden window
    /// listening for clipboard changes. Leaving it running when recording is
    /// switched off means Sill is still woken by every copy on the machine in
    /// order to decline to record it, which is the shape of cost rule 23
    /// exists to refuse.
    stopper: Arc<Mutex<Option<clipboard_master::Shutdown>>>,
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
    /// What to do with something that looks like a credential.
    pub secrets: crate::clipboard::sensitive::Policy,
    /// How long an entry is kept, in days. Zero means for good.
    ///
    /// Carried here so the watcher can do the pruning. It used to happen once
    /// at startup, which prunes nothing at all on a machine that is left on:
    /// a launcher meant to run for weeks would go weeks without honouring the
    /// setting, and the entry somebody expected to expire yesterday is still
    /// there.
    pub retain_days: u32,
}

impl Clipboard {
    /// A handle over a temporary database, for testing the parts that never
    /// touch it.
    ///
    /// The reservation count is arithmetic on an atomic and has nothing to do
    /// with storage, but it lives on this struct because the listener thread
    /// is what reads it. A temp file rather than `:memory:` because the store
    /// opens in WAL mode, which an in-memory database cannot use.
    #[cfg(test)]
    pub fn for_test() -> Self {
        let path = std::env::temp_dir().join(format!(
            "sill-clipboard-test-{}-{:?}.db",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);

        Self {
            store: Arc::new(Mutex::new(Store::open(&path).expect("a temp store opens"))),
            pruned: Arc::new(Mutex::new(None)),
            ignoring: Arc::new(AtomicUsize::new(0)),
            suspended: Arc::new(AtomicBool::new(false)),
            skipped: Arc::new(Mutex::new(None)),
            watching: Arc::new(AtomicBool::new(false)),
            stopper: Arc::new(Mutex::new(None)),
            rules: Arc::new(Mutex::new(Rules::default())),
        }
    }

    pub fn open(app: &AppHandle) -> Option<Self> {
        let path = app.path().app_data_dir().ok()?.join("clipboard.db");

        match Store::open(&path) {
            Ok(store) => Some(Self {
                store: Arc::new(Mutex::new(store)),
                pruned: Arc::new(Mutex::new(None)),
                ignoring: Arc::new(AtomicUsize::new(0)),
                suspended: Arc::new(AtomicBool::new(false)),
                skipped: Arc::new(Mutex::new(None)),
                watching: Arc::new(AtomicBool::new(false)),
                stopper: Arc::new(Mutex::new(None)),
                rules: Arc::new(Mutex::new(Rules {
                    enabled: true,
                    keep_images: true,
                    ignored_apps: Vec::new(),
                    secrets: sensitive::Policy::default(),
                    // Replaced by the real setting the moment preferences are
                    // read; zero here means nothing is pruned before then,
                    // which is the safe way round for a default.
                    retain_days: 0,
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

    /// Records nothing at all until [`Self::resume`].
    ///
    /// For an operation that writes an unknown number of times: a shortcut
    /// borrows the clipboard, runs an action that writes its own result, and
    /// puts the original back. Every one of those is Sill, and none of them is
    /// something the user copied.
    pub fn suspend(&self) {
        self.suspended.store(true, Ordering::SeqCst);
    }

    /// Starts recording again, forgetting anything reserved while suspended.
    ///
    /// Call after a settle: the listener runs on its own thread and reports a
    /// change slightly after it happens, so resuming the instant the last
    /// write returns lets that last write be recorded as the user's.
    pub fn resume(&self) {
        self.ignoring.store(0, Ordering::SeqCst);
        self.suspended.store(false, Ordering::SeqCst);
    }

    fn take_ignore(&self) -> bool {
        // Nothing is counted while suspended, so nothing can be miscounted.
        if self.suspended.load(Ordering::SeqCst) {
            return true;
        }

        self.ignoring
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                current.checked_sub(1)
            })
            .is_ok()
    }

    /// What was last declined, if it has not been superseded.
    pub fn last_skipped(&self) -> Option<Skipped> {
        self.skipped.lock().ok().and_then(|s| s.clone())
    }

    fn note_skipped(&self, skipped: Option<Skipped>) {
        if let Ok(mut slot) = self.skipped.lock() {
            *slot = skipped;
        }
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
            let stopper = clipboard.stopper.clone();
            let watching = clipboard.watching.clone();
            let handler = Handler {
                app,
                clipboard: clipboard.clone(),
            };

            match clipboard_master::Master::new(handler) {
                Ok(mut master) => {
                    // Taken before the run, because `run` does not return
                    // until something asks it to.
                    if let Ok(mut slot) = stopper.lock() {
                        *slot = Some(master.shutdown_channel());
                    }

                    if let Err(err) = master.run() {
                        crate::say!("the clipboard watcher stopped: {err}");
                    }
                }
                Err(err) => crate::say!("could not start the clipboard watcher: {err}"),
            }

            if let Ok(mut slot) = stopper.lock() {
                *slot = None;
            }
            watching.store(false, Ordering::SeqCst);
        })
        .ok();
}

/// Stops watching, and lets the thread and its window go.
///
/// Not merely declining to record. The watcher is woken by every copy on the
/// machine whether or not it does anything with what it sees, so switching
/// recording off has to end it rather than teach it to ignore everything.
pub fn stop(clipboard: &Clipboard) {
    let taken = clipboard.stopper.lock().ok().and_then(|mut s| s.take());
    if let Some(shutdown) = taken {
        shutdown.signal();
    }
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

/// An entry that was not recorded, and why.
///
/// Carries the length rather than any part of the value, so the notice can say
/// something useful without the thing it declined to store passing through
/// another layer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub what: String,
    pub length: usize,
}

/// Reads whatever is on the clipboard now and records it.
fn capture(app: &AppHandle, clipboard: &Clipboard) -> Result<(), String> {
    let policy = clipboard.rules().secrets;
    capture_with(app, clipboard, policy)
}

/// Records what is on the clipboard now, keeping it whatever it looks like.
///
/// The way back from a false positive. The entry is still on the clipboard, so
/// nothing had to be held anywhere to make this possible: the notice says one
/// was skipped, and this reads the same clipboard again and stores it.
///
/// A password manager's exclusion is still honoured. That is the application
/// saying so about its own data, not a guess, and no button in Sill overrides
/// somebody else's stated intent.
pub fn keep_current(app: &AppHandle, clipboard: &Clipboard) -> Result<(), String> {
    capture_with(app, clipboard, sensitive::Policy::Keep)
}

fn capture_with(
    app: &AppHandle,
    clipboard: &Clipboard,
    policy: sensitive::Policy,
) -> Result<(), String> {
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

        // Before anything is written. The history is a plain file on disk, so
        // the decision has to happen here rather than at read time: an entry
        // that reaches the database has already leaked.
        let mut text = text;
        if let Some(what) = crate::clipboard::sensitive::classify(&text).secret() {
            match policy {
                sensitive::Policy::Skip => {
                    // Said out loud rather than dropped quietly. Silently
                    // losing an entry is indistinguishable from the history
                    // being broken, and the user is the only one who can tell
                    // whether the guess was right.
                    let notice = Skipped {
                        what: what.to_string(),
                        length: text.chars().count(),
                    };
                    clipboard.note_skipped(Some(notice.clone()));
                    let _ = app.emit("clipboard:skipped", notice);
                    return Ok(());
                }
                sensitive::Policy::Redact => {
                    text = crate::clipboard::sensitive::redacted(what, text.chars().count());
                }
                sensitive::Policy::Keep => {}
            }
        }

        // The formatted version of the same copy, when there was one. Read
        // after the secret check rather than before it: a redacted entry
        // must not keep the markup that still holds the value.
        let html = if matches!(
            crate::clipboard::sensitive::classify(&text),
            sensitive::Sensitivity::Ordinary
        ) {
            read_html(&mut board)
        } else {
            None
        };

        let kind = classify(&text);
        let store = clipboard.store();
        store
            .record(Recording {
                hash: &hash(text.as_bytes()),
                kind,
                text: &text,
                html: html.as_deref(),
                app: name.as_deref(),
                app_path: exe.as_deref(),
                bytes: text.len() as i64,
                now,
            })
            .map_err(|e| e.to_string())?;
        drop(store);

        // Something was recorded, so the last refusal is no longer the most
        // recent thing that happened and its offer to keep it is stale: the
        // clipboard has moved on and there is nothing left to keep.
        clipboard.note_skipped(None);

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
            .record(Recording {
                hash: &hash(&image.bytes),
                kind: Kind::Image,
                text: &label,
                html: None,
                app: name.as_deref(),
                app_path: exe.as_deref(),
                bytes: bytes as i64,
                now,
            })
            .map_err(|e| e.to_string())?;

        /*
         * Reported, because the entry exists either way.
         *
         * A blob that does not get written leaves a row saying "Image 800x600"
         * with nothing behind it, and choosing it pastes that label as text.
         * Nothing about the list says the picture is gone, and the causes are
         * all the ones that persist: a full disk, a database opened
         * read-only, an image that grew past what SQLite would take.
         *
         * Keyed, so a hundred copies onto a full disk is one line rather than
         * a hundred. The thing that is wrong is the clipboard not keeping
         * images, and it is wrong once.
         */
        if bytes <= MAX_IMAGE_BYTES {
            match encode_png(&image) {
                Some(png) => match store.put_blob(id, &png) {
                    Ok(()) => crate::status::resolved(app, IMAGE_TROUBLE),
                    Err(err) => crate::status::report(
                        app,
                        IMAGE_TROUBLE,
                        format!(
                            "Sill is keeping copied images as their size only, because the \
                             picture itself could not be stored: {err}"
                        ),
                        Some("clipboard"),
                    ),
                },
                None => crate::status::report(
                    app,
                    IMAGE_TROUBLE,
                    "Sill could not encode a copied image, so its entry has no picture behind it.",
                    Some("clipboard"),
                ),
            }
        }
        drop(store);

        prune_occasionally(clipboard, rules.retain_days);

        let _ = app.emit("clipboard:changed", ());
    }

    Ok(())
}

/// Opens the clipboard, waiting out whoever else has it.
fn open_clipboard() -> Option<arboard::Clipboard> {
    for attempt in 0..CLIPBOARD_ATTEMPTS {
        match arboard::Clipboard::new() {
            Ok(board) => return Some(board),
            Err(err) => {
                if attempt + 1 == CLIPBOARD_ATTEMPTS {
                    crate::say!(
                        "could not open the clipboard after {CLIPBOARD_ATTEMPTS} tries: {err}"
                    );
                    return None;
                }
                std::thread::sleep(RETRY_DELAY);
            }
        }
    }
    None
}

/// The formatted version of what was copied, when the source offered one.
///
/// Absent far more often than not: a terminal, a code editor and a plain text
/// field all copy text and nothing else. Missing is the ordinary case and not
/// worth a retry ladder, unlike the text itself.
fn read_html(board: &mut arboard::Clipboard) -> Option<String> {
    board
        .get()
        .html()
        .ok()
        .map(|html| fragment(&html))
        .filter(|html| !html.is_empty())
}

/// The part of a CF_HTML payload that is actually the copied content.
///
/// **Real CF_HTML headers are frequently wrong**, and the reader trusts them.
/// Its byte offsets say where the fragment starts and ends, and when an
/// application miscounts them the reader falls back to the end of the buffer,
/// handing back the closing tags of the document wrapper and the clipboard's
/// own terminating NUL. Observed from .NET's own clipboard writer, so this is
/// the ordinary case rather than a corrupt one.
///
/// A NUL in the middle of a string is not something to write into a database
/// and then hand back to a renderer, and trailing `</body></html>` inside what
/// claims to be a fragment is markup nobody copied.
fn fragment(html: &str) -> String {
    let mut text = html;

    // The markers are comments the wrapper puts around the real content, and
    // they are correct even when the offsets that point at them are not.
    if let Some(at) = text.find("<!--StartFragment-->") {
        text = &text[at + "<!--StartFragment-->".len()..];
    }

    if let Some(at) = text.find("<!--EndFragment-->") {
        text = &text[..at];
    }

    text.trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_string()
}

/// Reads text, retrying a locked clipboard but not an absent format.
///
/// The distinction is the point. `ContentNotAvailable` means there is no text
/// on the clipboard, which is a fact and answered immediately so an image can
/// be tried instead. Anything else means the read failed, which is worth
/// waiting out.
fn read_text(board: &mut arboard::Clipboard) -> Option<String> {
    for attempt in 0..CLIPBOARD_ATTEMPTS {
        match board.get_text() {
            Ok(text) => return Some(text),
            Err(arboard::Error::ContentNotAvailable) => return None,
            Err(_) if attempt + 1 == CLIPBOARD_ATTEMPTS => return None,
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
    fn a_html_payload_with_wrong_offsets_still_yields_just_the_content() {
        // What .NET's own clipboard writer produces: the reader trusts the
        // header's byte offsets, miscounts, and hands back the rest of the
        // document plus the clipboard's terminating NUL.
        let ragged = "<b>hello</b><!--EndFragment--></body></html>\0";
        assert_eq!(fragment(ragged), "<b>hello</b>");

        // And with the opening marker still attached.
        let both = "<html><body><!--StartFragment--><i>hi</i><!--EndFragment--></body></html>\0";
        assert_eq!(fragment(both), "<i>hi</i>");
    }

    #[test]
    fn a_well_formed_fragment_is_left_exactly_as_it_is() {
        // The common case must not be mangled by the repair.
        assert_eq!(fragment("<b>hello</b>"), "<b>hello</b>");
        assert_eq!(fragment("  <p>spaced</p>  "), "<p>spaced</p>");
    }

    #[test]
    fn no_nul_survives_into_the_database() {
        // SQLite will store it, and everything downstream then carries a
        // string that truncates in half the places it is used.
        for html in ["<b>x</b>\0", "\0<b>x</b>", "<b>x</b><!--EndFragment-->\0\0"] {
            assert!(!fragment(html).contains('\0'), "{html:?}");
        }
    }

    #[test]
    fn nothing_is_recorded_while_the_clipboard_is_suspended() {
        // A shortcut writes to the clipboard an unknown number of times: the
        // borrow, the action's own copy of its result, the paste, and putting
        // the original back. Reserving a number up front means guessing it,
        // and the guess was wrong by exactly one, which is how a 5,011
        // character clipboard came back as 14.
        let clipboard = Clipboard::for_test();
        clipboard.suspend();

        for change in 0..9 {
            assert!(clipboard.take_ignore(), "change {change} is Sill's");
        }

        clipboard.resume();
        assert!(
            !clipboard.take_ignore(),
            "once resumed, a change is the user's again"
        );
    }

    #[test]
    fn resuming_forgets_anything_reserved_while_suspended() {
        // Both mechanisms are live: a single known write still reserves one
        // change. Reservations made while suspended are never consumed, so
        // without this they would swallow real copies later.
        let clipboard = Clipboard::for_test();
        clipboard.suspend();
        clipboard.ignore_next_changes(3);
        clipboard.resume();

        assert!(
            !clipboard.take_ignore(),
            "a stale reservation would eat the user's next copy"
        );
    }

    #[test]
    fn one_known_write_is_still_ignored_exactly_once() {
        // Pasting an entry out of the history writes once and must not come
        // straight back in as a new entry. That path is unchanged.
        let clipboard = Clipboard::for_test();
        clipboard.ignore_next_changes(1);

        assert!(clipboard.take_ignore());
        assert!(!clipboard.take_ignore());
    }

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
        assert!(!is_ignored(
            Some("Slack"),
            &["".to_string(), "  ".to_string()]
        ));
    }

    #[test]
    fn the_hash_is_short_enough_to_index() {
        // The whole reason it exists: SQLite will not index a long value,
        // and copying a whole file has to still deduplicate.
        assert_eq!(hash(&vec![0u8; 10_000_000]).len(), 64);
    }
}
