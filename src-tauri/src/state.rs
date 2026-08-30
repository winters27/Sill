//! What the application holds while it runs.
//!
//! Gathered here rather than left at the top of `lib.rs` because every command
//! module needs them, and a state type defined next to the code that starts
//! the app reads as if only that code owns it.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::exthost::ExtHost;
use crate::preferences;
use crate::registry::{CommandRecord, Frecency};

/// Holds the extension host, if one is running.
///
/// It is a Node process, and starting it with the app cost 38 MB of resident
/// memory on every machine, including the overwhelming majority of sessions
/// where no extension is ever opened. So it starts on the first extension
/// launch and shuts itself down again once nothing has used it, which is the
/// same lifecycle `dictation::server` gives the whisper process.
///
/// The slot is an `Arc` so it can be cloned out of Tauri's state and moved
/// into an async task. Holding a `State<'_, _>` across an await would borrow
/// the app handle for the life of the task.
#[derive(Clone)]
pub(crate) struct HostState {
    pub(crate) inner: Arc<tokio::sync::Mutex<Option<Arc<ExtHost>>>>,
    /// Built once and reused across host restarts. It owns `LocalStorage`,
    /// which is a file on disk, and the event sender the window listens on.
    pub(crate) api: Arc<crate::exthost::ApiLayer>,
    pub(crate) host_js: Arc<PathBuf>,
    /// When the host was last asked for, which is what the watchdog measures.
    pub(crate) last_used: Arc<std::sync::Mutex<std::time::Instant>>,
}

/// The user's own preferences.
#[derive(Clone)]
pub(crate) struct PrefsState {
    pub(crate) inner: Arc<tokio::sync::Mutex<preferences::Preferences>>,
    pub(crate) path: Arc<PathBuf>,
}

/// Sill's own index of the files under the folders it was told to watch.
///
/// An `ArcSwap` rather than a lock because the two things done to it are very
/// different: searching happens while somebody is typing and must never wait,
/// and rebuilding happens on a background thread and takes over a second. A
/// rebuild produces a whole new catalog and swaps it in, so a search either
/// sees the old one or the new one and never blocks on either.
#[derive(Clone, Default)]
pub(crate) struct CatalogState {
    pub(crate) inner: Arc<arc_swap::ArcSwap<crate::catalog::Catalog>>,
    /// How long the last walk took, which is what paces the next one.
    ///
    /// Milliseconds, because an atomic is what a watcher thread can read
    /// without waiting on whatever the rebuild thread is doing.
    pub(crate) cost: Arc<std::sync::atomic::AtomicU64>,
    /// Set while a rebuild is running, so two do not overlap.
    ///
    /// A walk is over a second of work and megabytes of allocation. Two at
    /// once would cost twice that to produce the same answer.
    pub(crate) building: Arc<std::sync::atomic::AtomicBool>,
    /// Where the index is kept between runs.
    pub(crate) cache: Arc<Option<PathBuf>>,
}

impl CatalogState {
    /// Rebuilds in the background, unless a rebuild is already running.
    ///
    /// Returns immediately. Nothing waits for the index: file search answers
    /// from whatever is currently swapped in, which on a first run is empty
    /// and a second later is not.
    pub(crate) fn rebuild(&self, roots: Vec<PathBuf>) {
        use std::sync::atomic::Ordering;

        if self
            .building
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }

        let inner = self.inner.clone();
        let building = self.building.clone();
        let cache = self.cache.clone();
        let cost = self.cost.clone();

        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let catalog = crate::catalog::Catalog::build(&roots);
            cost.store(started.elapsed().as_millis() as u64, Ordering::Release);

            crate::say!(
                "file index: {} entries in {} ms",
                catalog.len(),
                started.elapsed().as_millis()
            );

            // Saved after it is swapped in, not before. Searching gets the new
            // index a moment sooner, and writing a file is not something to
            // make anybody wait behind.
            let saved = Arc::new(catalog);
            inner.store(saved.clone());
            building.store(false, Ordering::Release);

            if let Some(path) = cache.as_ref() {
                if let Err(err) = saved.save(path) {
                    crate::say!("file index: could not save: {err}");
                }
            }
        });
    }

    /// Reads last run's index, so searching works before the walk finishes.
    ///
    /// Walking a whole drive takes nine seconds and a home folder over one.
    /// Nobody should wait either of those out to search for a file they know
    /// is there, and it was there last time too.
    ///
    /// The index is used exactly as saved and then replaced by a fresh walk a
    /// second or so later. In between it is as right as it was when the
    /// application last closed, which is a far better answer than nothing.
    pub(crate) fn warm(&self, roots: &[PathBuf]) {
        let Some(path) = self.cache.as_ref() else {
            return;
        };

        let started = std::time::Instant::now();

        if let Some(catalog) = crate::catalog::Catalog::load(path, roots) {
            crate::say!(
                "file index: {} entries read from last run in {} ms",
                catalog.len(),
                started.elapsed().as_millis()
            );
            self.inner.store(Arc::new(catalog));
        }
    }
}

/// Notices files appearing and disappearing, and rebuilds when they do.
///
/// **Coalesced hard, on purpose.** Saving a file in an editor produces several
/// events, a `git checkout` produces thousands, and a rebuild is over a second
/// of work. So changes are collected and one rebuild runs after things have
/// been quiet for a while, rather than one rebuild per event.
///
/// The quiet period is deliberately long. A file that appeared four seconds
/// ago and cannot be found yet is a much smaller problem than a launcher that
/// walks a home folder every time a build writes to disk, which is exactly the
/// kind of idle cost rule 23 exists to prevent.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(4);

/// The share of one processor a rebuild may take, over the long run.
///
/// The wait between two rebuilds is the last one's cost multiplied by this, so
/// a walk that takes a second earns a twenty second wait and one that takes
/// six earns two minutes. Indexing therefore costs about a twentieth of one
/// core while files are changing constantly, whether the folder is small or a
/// whole drive, without a number anywhere that has to be guessed per machine.
const ONE_IN: u32 = 20;

/// The least time between two rebuilds, whatever the arithmetic says.
///
/// Stops a very fast walk from rebuilding on every keystroke of somebody's
/// editor, and covers the first rebuild, which has no previous cost to go on.
const FLOOR: std::time::Duration = std::time::Duration::from_secs(20);

/// How long to leave the index alone after a rebuild that took this long.
///
/// Pure arithmetic, so it can be checked without waiting for any of it.
pub fn quiet_after(build: std::time::Duration) -> std::time::Duration {
    (build * ONE_IN).max(FLOOR)
}

/// Watches the indexed folders and keeps the index roughly current.
///
/// Held so the watcher lives as long as the app does. Dropping it stops the
/// watching, which is what should happen when the folders change: the old
/// watcher goes and a new one takes its place.
pub(crate) struct CatalogWatcher {
    // Behind a mutex because Tauri's managed state is shared across threads
    // and a watcher is only `Send`. Nothing ever locks it: the field exists to
    // keep the watcher alive, and dropping it is what stops the watching.
    _watcher: std::sync::Mutex<Box<dyn notify::Watcher + Send>>,
}

impl CatalogWatcher {
    /// Starts watching, and rebuilds after things settle.
    ///
    /// Failing to watch is not fatal and is not worth stopping over. The index
    /// is still built once at startup and can still be rebuilt by hand; all
    /// that is lost is noticing changes on its own.
    pub(crate) fn start(state: CatalogState, roots: Vec<PathBuf>) -> Option<Self> {
        use notify::{RecursiveMode, Watcher};

        if roots.is_empty() {
            return None;
        }

        // Cloned for the callback, which outlives this function.
        let watched = roots.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let cost = state.cost.clone();

        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };

            // Saving a file cannot change a list of file names, and saving
            // files is most of what a watcher ever reports.
            if !crate::catalog::changes_the_index(&event.kind) {
                return;
            }

            // Watching is recursive and the walk is not, so most of what
            // arrives here is from directories the index deliberately skips.
            // Left unfiltered this rebuilt eight times in seven minutes on an
            // idle machine, because `AppData` never stops changing.
            if !event
                .paths
                .iter()
                .any(|path| crate::catalog::worth_indexing(path, &watched))
            {
                return;
            }

            let _ = tx.send(());
        })
        .ok()?;

        for root in &roots {
            if let Err(err) = watcher.watch(root, RecursiveMode::Recursive) {
                crate::say!("file index: cannot watch {}: {err}", root.display());
            }
        }

        std::thread::spawn(move || {
            use std::sync::atomic::Ordering;

            // Starts as if a rebuild just happened, because one did: the
            // caller walks once at startup. Starting the wait in the past
            // meant the first event to arrive rebuilt immediately, so an
            // ordinary launch walked the whole drive twice, seven seconds
            // apart.
            let mut last = std::time::Instant::now();

            while rx.recv().is_ok() {
                // Drain whatever else arrived while things settle. Checking
                // out a branch writes a thousand files and should cost one
                // rebuild, not a thousand.
                while rx.recv_timeout(SETTLE).is_ok() {}

                // Waited out rather than dropped. Skipping the rebuild while
                // inside the quiet period threw the change away with it, so a
                // file created in the first two minutes after a launch stayed
                // unfindable until something else happened to change.
                let quiet = quiet_after(std::time::Duration::from_millis(
                    cost.load(Ordering::Acquire),
                ));

                if let Some(wait) = quiet.checked_sub(last.elapsed()) {
                    std::thread::sleep(wait);

                    // Anything that arrived during the wait is covered by the
                    // rebuild about to happen.
                    while rx.try_recv().is_ok() {}
                }

                last = std::time::Instant::now();
                state.rebuild(roots.clone());
            }
        });

        Some(Self {
            _watcher: std::sync::Mutex::new(Box::new(watcher)),
        })
    }
}

/// The installed command registry and its ranking state.
#[derive(Clone)]
pub(crate) struct RegistryState {
    pub(crate) inner: Arc<tokio::sync::Mutex<Registry>>,
}

pub(crate) struct Registry {
    pub(crate) commands: Vec<CommandRecord>,
    /// Sill's own settings, shaped as commands.
    ///
    /// Built once at startup: the catalogue is a `const` and cannot change
    /// while the app runs, so rebuilding it per keystroke would be pure cost.
    pub(crate) own_settings: Vec<CommandRecord>,
    /// Snippets, shaped as commands.
    ///
    /// Held here rather than read per query: the previous version parsed
    /// `snippets.json` off disk on every keystroke, which is a filesystem
    /// round trip per character typed. Refreshed whenever a snippet changes.
    pub(crate) snippets: Vec<CommandRecord>,
    /// Quicklinks, shaped as commands. Held for the same reason as snippets.
    pub(crate) quicklinks: Vec<CommandRecord>,
    pub(crate) frecency: Frecency,
    pub(crate) frecency_path: PathBuf,
    /// The names the user has chosen for things.
    ///
    /// Here beside frecency because it plays the same role: user state that
    /// ranking consults on every keystroke. Rebuilt when the preference
    /// changes rather than read from disk per query.
    pub(crate) aliases: crate::registry::Aliases,
}

pub(crate) fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}
