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

/// The share of the machine a rebuild may take, over the long run.
///
/// The wait between two rebuilds is the last one's wall-clock cost multiplied
/// by this, so a walk that takes a second earns a twenty second wait and one
/// that takes six earns two minutes. It self-tunes: a small folder and a whole
/// drive both settle at the same share without a number to guess per machine.
///
/// **The machine, not one core.** The walk is parallel, so a rebuild taking
/// 1.3 seconds of wall time on six threads spends up to 7.8 seconds of
/// processor time, and pacing on the wall clock accounts for the first number
/// rather than the second. Measured on a sixteen core machine with a home
/// folder indexed and files changing: **3.4 seconds of processor over thirty,
/// about a tenth of one core**, against the twentieth an earlier version of
/// this comment claimed.
///
/// It is left as it is deliberately. Charging the full processor cost would
/// mean two and a half minutes before a new file could be found, and the real
/// answer is not a longer wait but a smaller unit of work: patching the index
/// for the file that changed instead of walking everything again. That is
/// worth doing and is not done yet.
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

/**
The installed command registry and its ranking state.

## Why nothing here is a lock

Both halves are read on **every keystroke** and written rarely, which is the
shape `arc_swap` exists for and the shape the file catalog beside it already
uses. This was one `tokio::Mutex` around everything, held for the whole of
ranking and, until recently, across writing files. A search, a launch and a
rescan all queued behind each other for no reason: they mostly want to read.

Readers take a snapshot and never wait. A writer builds the replacement and
swaps it in, so a search in flight finishes against the version it started
with rather than seeing half an update.

## Why the two halves are separate

They change at completely different rates. The index changes when something is
installed or a snippet is edited; the ranking changes on **every launch**. One
`ArcSwap` over both would mean copying the whole index, thousands of records,
to record that somebody opened a calculator.
*/
#[derive(Clone)]
pub(crate) struct RegistryState {
    /// Everything a search can find. Replaced wholesale by a scan.
    pub(crate) index: Arc<arc_swap::ArcSwap<Index>>,
    /// What ranking weights by, and where it is kept.
    pub(crate) ranking: Arc<arc_swap::ArcSwap<Ranking>>,
    /**
    Held by whoever is about to change the ranking, and by nobody else.

    `ArcSwap` makes reads free; it does not make read-modify-write safe. Two
    launches landing together would both copy the same starting point and the
    second swap would throw the first away. Launches are human-paced so this is
    vanishingly rare, and a lost launch is a silently wrong ranking rather than
    an error, which is exactly the kind of bug that is never found.

    A plain `std::sync::Mutex`, deliberately: nothing is awaited while it is
    held, so an async one would only add a scheduling point.
    */
    pub(crate) recording: Arc<std::sync::Mutex<()>>,
}

impl RegistryState {
    /// A snapshot of everything searchable. Cheap, and never blocks.
    pub(crate) fn index(&self) -> arc_swap::Guard<Arc<Index>> {
        self.index.load()
    }

    /// A snapshot of the ranking state. Cheap, and never blocks.
    pub(crate) fn ranking(&self) -> arc_swap::Guard<Arc<Ranking>> {
        self.ranking.load()
    }

    /**
    Changes the ranking, under the writer lock.

    Copy, modify, swap. The copy is of the ranking alone, which is a map of
    counts and timestamps, not of the index beside it.

    Hands back the serialised form so the caller can put it on disk **after**
    this returns: the write is the slow part and nothing is holding it up by
    then.
    */
    pub(crate) fn record<R>(&self, change: impl FnOnce(&mut Ranking) -> R) -> (R, Option<String>) {
        let _writing = self
            .recording
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut next = Ranking::clone(&self.ranking.load());
        let answer = change(&mut next);
        let text = serde_json::to_string(&next.frecency).ok();

        self.ranking.store(Arc::new(next));
        (answer, text)
    }

    /**
    Changes what a search can find, under the same writer lock.

    Copy, modify, swap, exactly as `record` does, and for the same reason: an
    `ArcSwap` makes reads free and does nothing for read-modify-write. Five
    different things update this (the cache, the scan, snippets, quicklinks,
    scripts) and two of them landing together would lose one.

    A closure rather than a whole `Index` on purpose. Handing in a replacement
    means naming every field, so the caller that only meant to change snippets
    has to remember to carry the other four across, and forgetting one empties
    it with nothing failing. That is the same shape as every other list in this
    codebase that had to be kept in step by hand.
    */
    pub(crate) fn update_index(&self, change: impl FnOnce(&mut Index)) {
        let _writing = self
            .recording
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut next = Index::clone(&self.index.load());
        change(&mut next);
        self.index.store(Arc::new(next));
    }
}

/**
Which search is the current one.

The window already throws away a stale answer: every result carries the
`searchId` it belongs to and anything older is dropped. What it cannot do is
stop the work, so typing eight characters started eight file searches and eight
browser reads, seven of which produced answers nobody would ever see. On the
Everything path that is up to a second and a half each, and on the browser path
it is a copy of a history database that can be thirty megabytes.

This is the same idea on the other side of the wire: a search takes a token
before it starts and checks it before each expensive step, and a search that
has been overtaken stops there. Nothing is cancelled mid-flight; work simply
does not begin once it is known to be pointless, which is where nearly all of
the cost is.

One counter rather than one per source, because they are all driven by the same
keystroke: when a newer one starts, everything the older one was going to do is
equally stale.
*/
#[derive(Default)]
pub(crate) struct Searching(std::sync::atomic::AtomicU64);

impl Searching {
    /// Claims the newest search, and hands back the token to check against.
    pub(crate) fn begin(&self) -> u64 {
        self.0
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .wrapping_add(1)
    }

    /// Whether this is still the search anybody is waiting for.
    pub(crate) fn is_current(&self, token: u64) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed) == token
    }
}

/// What ranking weights by.
#[derive(Clone, Default)]
pub(crate) struct Ranking {
    pub(crate) frecency: Frecency,
    /// Where it is written. Set once, at startup, and carried here so a
    /// caller holding a snapshot needs nothing else to save it.
    pub(crate) path: PathBuf,
}

/// Everything a search can find.
#[derive(Clone, Default)]
pub(crate) struct Index {
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
    /// Script commands found in the folders somebody chose to scan.
    pub(crate) scripts: Vec<CommandRecord>,
    /// The names the user has chosen for things.
    ///
    /// Here rather than beside frecency because it changes when a preference
    /// changes, not when something is launched, which is the line these two
    /// halves are split along.
    pub(crate) aliases: crate::registry::Aliases,
}

impl Index {
    /// Everything a search can return.
    ///
    /// **One definition, used by both ends**, and that is the whole reason it
    /// exists. Searching chained four lists and launching looked a chosen row
    /// up in one of them, so a snippet, a quicklink and every one of Sill's
    /// own settings could be found and then not run: pressing Enter answered
    /// "no such command" with the id it had just been given.
    ///
    /// Nothing about either side said they had to agree. Now the only way to
    /// add a list is here, where both of them read it.
    pub(crate) fn everything(&self) -> impl Iterator<Item = &crate::registry::CommandRecord> {
        self.lists().into_iter().flatten()
    }

    /// Every list of records, named once.
    ///
    /// **Destructured on purpose, with no `..`.** A list added to the struct
    /// stops this compiling until it is named here, which is the only version
    /// of this guarantee that does not rely on somebody remembering. The test
    /// below used to be that guarantee and could not be: it counted the same
    /// lists it was checking, so a sixth added to neither side passed happily.
    fn lists(&self) -> Vec<&Vec<crate::registry::CommandRecord>> {
        let Index {
            commands,
            own_settings,
            snippets,
            quicklinks,
            scripts,
            aliases: _,
        } = self;

        vec![commands, own_settings, snippets, quicklinks, scripts]
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::CommandRecord;

    fn row(id: &str, mode: &str) -> CommandRecord {
        CommandRecord {
            id: id.to_string(),
            extension: "x".into(),
            extension_title: "X".into(),
            command: id.to_string(),
            title: id.to_string(),
            subtitle: String::new(),
            description: String::new(),
            mode: mode.to_string(),
            entrypoint: id.to_string(),
            keywords: Vec::new(),
            icon: None,
            toggle: None,
            panel: None,
            preferences: serde_json::Value::Null,
        }
    }

    fn registry() -> Index {
        Index {
            commands: vec![row("app:one", "app")],
            own_settings: vec![row("sill-setting:general:One", "sill-setting")],
            snippets: vec![row("snippet:one", "snippet")],
            quicklinks: vec![row("quicklink:one", "quicklink")],
            scripts: vec![row("script:one", "script")],
            aliases: Default::default(),
        }
    }

    fn state() -> RegistryState {
        RegistryState {
            index: Arc::new(arc_swap::ArcSwap::from_pointee(registry())),
            ranking: Arc::new(arc_swap::ArcSwap::default()),
            recording: Arc::new(std::sync::Mutex::new(())),
        }
    }

    /**
    Two things recording at once must not lose one of them.

    This is the whole reason there is still a lock in here. `ArcSwap` makes a
    read free and does nothing at all for read-modify-write: without the writer
    lock, two launches landing together both copy the same starting point and
    whichever swaps second throws the first away. A lost launch is not an
    error, it is a ranking that is quietly wrong, which is the kind of bug
    nobody ever reports because nobody can see it.

    Threads rather than tasks, because the writer lock is a plain
    `std::sync::Mutex` and this is exactly the contention it exists for.
    */
    #[test]
    fn recording_from_several_places_at_once_loses_nothing() {
        let state = state();
        let hands = 8;
        let each = 50;

        std::thread::scope(|scope| {
            for hand in 0..hands {
                let state = &state;

                scope.spawn(move || {
                    for turn in 0..each {
                        state.record(|ranking| {
                            ranking.frecency.record(&format!("app:{hand}"), 1_000 + turn);
                        });
                    }
                });
            }
        });

        let ranking = state.ranking();

        assert_eq!(
            ranking.frecency.len(),
            hands,
            "every writer's entry survived"
        );

        for hand in 0..hands {
            assert_eq!(
                ranking.frecency.count(&format!("app:{hand}")),
                each as u32,
                "app:{hand} lost some of its launches to another thread's swap"
            );
        }
    }

    /// A reader holding a snapshot is unaffected by a writer replacing it.
    ///
    /// What the swap buys: a search that started against one index finishes
    /// against that index rather than seeing a scan land underneath it.
    #[test]
    fn a_snapshot_does_not_change_under_the_reader() {
        let state = state();

        let held = state.index();
        assert!(held.everything().any(|row| row.id == "app:one"));

        state.update_index(|index| index.commands = vec![row("app:two", "app")]);

        assert!(
            held.everything().any(|row| row.id == "app:one"),
            "the snapshot changed under the reader"
        );
        assert!(
            state.index().everything().any(|row| row.id == "app:two"),
            "and the next reader sees the new one"
        );
    }

    /// What can be found must be able to be run.
    ///
    /// Searching chained four lists and launching looked in one, so a snippet,
    /// a quicklink and every one of Sill's own settings were shown and then
    /// refused: pressing Enter answered "no such command" with the id it had
    /// just been handed. Nothing failed to compile and no test noticed.
    #[test]
    fn everything_holds_every_list_a_search_can_return() {
        let registry = registry();

        for id in [
            "app:one",
            "sill-setting:general:One",
            "snippet:one",
            "quicklink:one",
            "script:one",
        ] {
            assert!(
                registry.everything().any(|row| row.id == id),
                "{id} can be searched for and not run",
            );
        }
    }

    /// Every record in the fixture comes back out.
    ///
    /// This used to be the guard against a list being added to the struct and
    /// not to `everything`, and it could not be: it added up the same lists it
    /// was checking, so one added to neither side balanced. `lists` is
    /// destructured without a `..` instead, so that is now the compiler's
    /// problem. What is left here is worth keeping anyway: it catches a list
    /// named in `lists` and then filtered, skipped or deduplicated away.
    #[test]
    fn nothing_is_left_out_of_everything() {
        let registry = registry();
        let counted: usize = registry.lists().iter().map(|list| list.len()).sum();

        assert_eq!(
            registry.everything().count(),
            counted,
            "a list of records is not in `everything`",
        );
    }
}

#[cfg(test)]
mod searching {
    use super::*;

    /// The newest search is the only current one.
    #[test]
    fn starting_another_search_supersedes_the_one_before_it() {
        let searching = Searching::default();

        let first = searching.begin();
        assert!(searching.is_current(first), "the only search is the current one");

        let second = searching.begin();
        assert!(
            !searching.is_current(first),
            "the first must know it has been overtaken, or it goes on to copy a \
             thirty megabyte history for a keystroke nobody is waiting on"
        );
        assert!(searching.is_current(second));
    }

    /// Two tokens are never the same, even from two threads.
    ///
    /// If they were, an overtaken search would believe it was still current
    /// and the check would be worse than nothing: it would look like a
    /// safeguard and never fire.
    #[test]
    fn no_two_searches_share_a_token() {
        let searching = Searching::default();
        let hands = 8;
        let each = 200;

        let tokens = std::sync::Mutex::new(Vec::new());

        std::thread::scope(|scope| {
            for _ in 0..hands {
                let searching = &searching;
                let tokens = &tokens;

                scope.spawn(move || {
                    let mine: Vec<u64> = (0..each).map(|_| searching.begin()).collect();
                    tokens.lock().unwrap().extend(mine);
                });
            }
        });

        let mut all = tokens.into_inner().unwrap();
        let total = all.len();
        all.sort_unstable();
        all.dedup();

        assert_eq!(all.len(), total, "two searches were handed the same token");
    }
}
