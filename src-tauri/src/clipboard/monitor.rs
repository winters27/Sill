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

/// Most paths taken from one file copy.
///
/// A selection in Explorer can be a hundred thousand files. Past this the copy
/// is not recorded at all, which is what already happened to every file copy
/// before now, rather than recorded as a list quietly missing most of it.
const MAX_FILES: u32 = 10_000;

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

/// The housekeeping every recorded copy is followed by.
///
/// One function called from each branch rather than a line at the end of each,
/// **because that is exactly how the pruning got lost**: the text branch
/// returns as soon as it has emitted, so the call that sat below it ran only
/// when the thing copied happened to be a picture. Retention was therefore
/// honoured on a machine where somebody screenshots and not on one where they
/// copy words.
///
/// Neither half runs on a timer. A copy is the only moment the history grows,
/// so it is the only moment either bound can be exceeded.
fn after_recording(app: &AppHandle, clipboard: &Clipboard, rules: &Rules) {
    prune_occasionally(clipboard, rules.retain_days);
    cap_rows(clipboard, rules.max_entries);

    let _ = app.emit("clipboard:changed", ());
}

/// Holds the history to a number of entries as well as to an age.
///
/// Unthrottled, unlike the daily prune, and cheap enough to be: one statement
/// that deletes nothing at all until the cap is passed, over an index the
/// listing already uses. Throttling it would let a burst of copying put tens
/// of thousands of rows in before anything noticed, which is the case the cap
/// exists for.
///
/// **The row somebody is reading is never one of them.** See
/// [`Clipboard::viewing`].
fn cap_rows(clipboard: &Clipboard, max_entries: u32) {
    if max_entries == 0 {
        return;
    }

    let viewing = clipboard.viewing();
    match clipboard.store().trim_to(max_entries, viewing) {
        Ok(0) => {}
        Ok(gone) => crate::say!("trimmed {gone} clipboard entries past the limit"),
        Err(err) => crate::say!("could not trim the clipboard: {err}"),
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
    /// The entry the open history window last asked to see in full.
    ///
    /// The count cap deletes the oldest rows, and the oldest row is exactly
    /// what somebody scrolled to the bottom of the list to read. Deleting the
    /// thing under the cursor while they are looking at it is the one failure
    /// a cap must not have.
    ///
    /// Nothing new crosses the boundary to keep this: `clipboard_entry` is
    /// already called once per row as the selection settles, so the window
    /// says which row it is on by asking for it. A cap that needed its own
    /// notification would be an invoke per arrow key, which is the chatter
    /// rules 18 and 23 refuse.
    viewing: Arc<Mutex<Option<i64>>>,
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
    /// How many unpinned entries are kept. Zero means as many as arrive.
    ///
    /// Beside the retention rather than instead of it: they bound different
    /// things. Thirty days says nothing about a week spent copying, and a
    /// thousand rows says nothing about a code copied a month ago that is
    /// still there because nothing has pushed it out.
    pub max_entries: u32,
    /// Lock stored pictures to this Windows account.
    ///
    /// Only the pictures. The text is what full-text search reads, so
    /// encrypting it would mean either no search or an index holding the
    /// plaintext anyway, and neither is worth pretending about.
    pub encrypt_images: bool,
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
            viewing: Arc::new(Mutex::new(None)),
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
                viewing: Arc::new(Mutex::new(None)),
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
                    // read; zero here means nothing is pruned or trimmed
                    // before then, which is the safe way round for a default.
                    retain_days: 0,
                    max_entries: 0,
                    // Same reasoning, the other way up: nothing is written
                    // under a promise the setting may not actually make.
                    encrypt_images: false,
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

    /// Remembers which row the history window is showing in full.
    ///
    /// Set by `clipboard_entry`, which the window already calls once per row
    /// as the selection settles. Read by the count cap, which must not delete
    /// it out from under whoever is reading it.
    pub fn now_viewing(&self, id: i64) {
        if let Ok(mut slot) = self.viewing.lock() {
            *slot = Some(id);
        }
    }

    pub fn viewing(&self) -> Option<i64> {
        self.viewing.lock().ok().and_then(|slot| *slot)
    }

    pub fn set_rules(&self, rules: Rules) {
        // The store is what writes a blob, so it is what has to know whether
        // to lock one. Set before the rules are published so a copy arriving
        // in between is written under the setting that is already in force
        // rather than the one about to be.
        self.store().encrypt_blobs(rules.encrypt_images);

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

        after_recording(app, clipboard, &rules);
        return Ok(());
    }

    /*
     * A copy made in Explorer.
     *
     * It puts no text on the clipboard at all: `CF_HDROP`, a list of paths,
     * plus some shell formats, and neither `arboard` nor `clipboard-master`
     * reads any of them. So selecting five files and pressing Ctrl+C recorded
     * **nothing**, and the history simply had a gap where a copy had been.
     *
     * After the text rather than before it, so nothing that already worked
     * changes: an application that offers both keeps being recorded as its
     * text, which is what it was yesterday.
     */
    if let Some(paths) = read_files() {
        if let Some(text) = file_list(&paths) {
            if text.len() <= MAX_TEXT_BYTES {
                let store = clipboard.store();
                store
                    .record(Recording {
                        hash: &hash(text.as_bytes()),
                        // Known rather than guessed. `classify` reads a
                        // multi-line value as prose, which a list of paths is
                        // not, and this came from the shell as file names.
                        kind: Kind::File,
                        text: &text,
                        html: None,
                        app: name.as_deref(),
                        app_path: exe.as_deref(),
                        bytes: text.len() as i64,
                        now,
                    })
                    .map_err(|e| e.to_string())?;
                drop(store);

                after_recording(app, clipboard, &rules);
            }
            return Ok(());
        }
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
            match crate::clipboard::write::encode_png(&image) {
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

        after_recording(app, clipboard, &rules);
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

/// The paths a file copy is recorded as, one per line.
///
/// `\r\n` because these go back onto the clipboard as text and land in Windows
/// text fields, where a bare `\n` is one long line. Empty paths are dropped
/// rather than left as blank lines; an empty list is nothing to record.
fn file_list(paths: &[String]) -> Option<String> {
    let joined = paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>()
        .join("\r\n");

    (!joined.is_empty()).then_some(joined)
}

/// `CF_HDROP`, the format Explorer copies files as.
///
/// The number rather than the constant, which lives in a `windows` feature
/// this crate does not enable. It has been 15 since Windows 3.1 and is part of
/// the ABI.
#[cfg(windows)]
const CF_HDROP: u32 = 15;

/// The list of files on the clipboard, if there is one.
///
/// Retried like every other clipboard read: the application that just copied
/// is usually still holding the lock when Windows announces the change.
#[cfg(windows)]
fn read_files() -> Option<Vec<String>> {
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::UI::Shell::HDROP;

    // SAFETY: takes a format number and returns a plain result. Asked before
    // the clipboard is opened, because on almost every copy the answer is no
    // and opening it would be a lock taken for nothing.
    if unsafe { IsClipboardFormatAvailable(CF_HDROP) }.is_err() {
        return None;
    }

    for attempt in 0..CLIPBOARD_ATTEMPTS {
        // SAFETY: a null owner window is documented as allowed and means the
        // clipboard is associated with the current task.
        if unsafe { OpenClipboard(None) }.is_ok() {
            // SAFETY: the clipboard is open, so the handle it hands back is
            // valid until it is closed, which is the next statement but one.
            // The handle stays owned by the clipboard and is never freed here.
            let paths = unsafe {
                GetClipboardData(CF_HDROP)
                    .ok()
                    .map(|handle| paths_from(HDROP(handle.0)))
            };

            // SAFETY: paired with the successful open above.
            let _ = unsafe { CloseClipboard() };
            return paths.flatten();
        }

        if attempt + 1 == CLIPBOARD_ATTEMPTS {
            break;
        }
        std::thread::sleep(RETRY_DELAY);
    }

    None
}

/// Reads the paths out of an `HDROP`.
///
/// Split from the clipboard handling so it can be tested against a handle
/// built by hand. Standing up a real file copy would mean writing over
/// whatever the person running the tests had on their clipboard.
///
/// # Safety
///
/// `drop` must be a live `HDROP`, which is the only thing `CF_HDROP` ever is.
#[cfg(windows)]
unsafe fn paths_from(drop: windows::Win32::UI::Shell::HDROP) -> Option<Vec<String>> {
    use windows::Win32::UI::Shell::DragQueryFileW;

    // `0xFFFFFFFF` asks how many files there are rather than for one of them.
    let count = unsafe { DragQueryFileW(drop, u32::MAX, None) };
    if count == 0 || count > MAX_FILES {
        return None;
    }

    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        // With no buffer it returns the length, excluding the terminator.
        let needed = unsafe { DragQueryFileW(drop, index, None) };
        if needed == 0 {
            continue;
        }

        // Room for the terminator, which the call writes and does not count.
        let mut buffer = vec![0u16; needed as usize + 1];
        let written = unsafe { DragQueryFileW(drop, index, Some(&mut buffer)) };
        if written == 0 {
            continue;
        }

        paths.push(String::from_utf16_lossy(&buffer[..written as usize]));
    }

    (!paths.is_empty()).then_some(paths)
}

/// No shell, no file list.
#[cfg(not(windows))]
fn read_files() -> Option<Vec<String>> {
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

    // ----------------------------------------------------------- file lists

    #[test]
    fn a_file_copy_is_recorded_as_one_path_per_line() {
        // These go back onto the clipboard as text and land in Windows text
        // fields, where a bare newline is one long line.
        assert_eq!(
            file_list(&[r"C:\a.txt".into(), r"C:\b.txt".into()]),
            Some("C:\\a.txt\r\nC:\\b.txt".to_string())
        );
    }

    #[test]
    fn one_file_is_still_a_list_of_one() {
        assert_eq!(
            file_list(&[r"C:\only.txt".into()]),
            Some(r"C:\only.txt".to_string())
        );
    }

    #[test]
    fn nothing_to_list_is_nothing_to_record() {
        // A blank line in the middle of a paste is worse than a shorter list,
        // and an empty list is not a copy at all.
        assert_eq!(file_list(&[]), None);
        assert_eq!(file_list(&["".into(), "   ".into()]), None);
        assert_eq!(
            file_list(&[r"C:\a.txt".into(), "".into()]),
            Some(r"C:\a.txt".to_string()),
            "an empty path must not leave a blank line"
        );
    }

    /// The `HDROP` parse, against a handle built by hand.
    ///
    /// Standing up a real file copy would mean writing over whatever the
    /// person running the tests had on their clipboard, so the handle is
    /// assembled here in the shape Windows uses: a `DROPFILES` header, then a
    /// double-null-terminated run of wide strings.
    #[cfg(windows)]
    #[test]
    fn a_real_hdrop_yields_the_paths_inside_it() {
        use windows::Win32::UI::Shell::HDROP;

        // Declared by hand rather than by enabling another `windows` feature,
        // the same reasoning as `secrets.rs`: this crate's feature list has
        // already pushed rustc into an out-of-memory abort once by
        // accumulating, and these are three extern declarations.
        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalAlloc(flags: u32, bytes: usize) -> *mut core::ffi::c_void;
            fn GlobalLock(handle: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
            fn GlobalUnlock(handle: *mut core::ffi::c_void) -> i32;
            fn GlobalFree(handle: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        }

        const GMEM_MOVEABLE: u32 = 0x0002;
        /// `DROPFILES`: the offset, a POINT, and two BOOLs.
        const HEADER: usize = 20;

        let paths = [r"C:\one.txt", r"C:\a folder\two.txt"];
        let mut wide: Vec<u16> = Vec::new();
        for path in paths {
            wide.extend(path.encode_utf16());
            wide.push(0);
        }
        wide.push(0);

        // SAFETY: the block is sized for the header plus the list, written
        // once while locked, and freed at the end of the test. `paths_from`
        // only reads it.
        let found = unsafe {
            let bytes = HEADER + wide.len() * 2;
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
            assert!(!handle.is_null(), "the allocation failed");

            let base = GlobalLock(handle) as *mut u8;
            assert!(!base.is_null(), "the lock failed");
            std::ptr::write_bytes(base, 0, bytes);
            // `pFiles`, the offset the list starts at.
            (base as *mut u32).write_unaligned(HEADER as u32);
            // `fWide`, which says the list is UTF-16 rather than ANSI.
            (base.add(16) as *mut u32).write_unaligned(1);
            std::ptr::copy_nonoverlapping(
                wide.as_ptr() as *const u8,
                base.add(HEADER),
                wide.len() * 2,
            );
            GlobalUnlock(handle);

            let found = paths_from(HDROP(handle));
            GlobalFree(handle);
            found
        };

        assert_eq!(
            found,
            Some(vec![
                r"C:\one.txt".to_string(),
                r"C:\a folder\two.txt".to_string()
            ])
        );
    }

    // -------------------------------------------------------- the count cap

    #[test]
    fn the_cap_holds_the_history_to_its_limit() {
        let clipboard = Clipboard::for_test();
        {
            let store = clipboard.store();
            store.clear(true).expect("starts empty");
            for age in 0..6 {
                store
                    .record(Recording {
                        hash: &format!("entry {age}"),
                        kind: Kind::Text,
                        text: &format!("entry {age}"),
                        html: None,
                        app: None,
                        app_path: None,
                        bytes: 7,
                        now: now_seconds() - age,
                    })
                    .expect("records");
            }
        }

        cap_rows(&clipboard, 2);

        assert_eq!(clipboard.store().count().expect("counts"), 2);
    }

    #[test]
    fn a_cap_of_zero_does_not_touch_the_history() {
        // What every existing history starts at, and what somebody chooses
        // when they want everything kept.
        let clipboard = Clipboard::for_test();
        {
            let store = clipboard.store();
            store.clear(true).expect("starts empty");
            for age in 0..4 {
                store
                    .record(Recording {
                        hash: &format!("kept {age}"),
                        kind: Kind::Text,
                        text: "kept",
                        html: None,
                        app: None,
                        app_path: None,
                        bytes: 4,
                        now: now_seconds() - age,
                    })
                    .expect("records");
            }
        }

        cap_rows(&clipboard, 0);

        assert_eq!(clipboard.store().count().expect("counts"), 4);
    }

    /// The window says which row it is on by asking for it, and the cap reads
    /// that rather than deleting whatever is oldest.
    #[test]
    fn the_cap_spares_the_row_the_window_is_showing() {
        let clipboard = Clipboard::for_test();
        let oldest = {
            let store = clipboard.store();
            store.clear(true).expect("starts empty");
            let oldest = store
                .record(Recording {
                    hash: "on screen",
                    kind: Kind::Text,
                    text: "on screen",
                    html: None,
                    app: None,
                    app_path: None,
                    bytes: 9,
                    now: now_seconds() - 10_000,
                })
                .expect("records");
            for age in 0..5 {
                store
                    .record(Recording {
                        hash: &format!("later {age}"),
                        kind: Kind::Text,
                        text: "later",
                        html: None,
                        app: None,
                        app_path: None,
                        bytes: 5,
                        now: now_seconds() - age,
                    })
                    .expect("records");
            }
            oldest
        };

        clipboard.now_viewing(oldest);
        cap_rows(&clipboard, 2);

        assert!(
            clipboard.store().get(oldest).expect("reads").is_some(),
            "the cap deleted the row somebody was reading"
        );
    }

    #[test]
    fn the_default_rules_bound_nothing_until_they_are_set() {
        // Same reasoning as `enabled`: acting on a limit nobody has read yet
        // would be deleting history under a setting that may not exist.
        assert_eq!(Rules::default().max_entries, 0);
        assert_eq!(Rules::default().retain_days, 0);
        assert!(!Rules::default().encrypt_images);
    }

    #[test]
    fn the_hash_is_short_enough_to_index() {
        // The whole reason it exists: SQLite will not index a long value,
        // and copying a whole file has to still deduplicate.
        assert_eq!(hash(&vec![0u8; 10_000_000]).len(), 64);
    }
}
