//! Sill's own index of the files worth finding.
//!
//! Built because file search should not require installing something else
//! first. A third-party indexer is still used when one is running, and it sees
//! more than this does, but a launcher whose file search does nothing until you
//! go and install a second program has file search in name only.
//!
//! # Why this is affordable
//!
//! A home folder on this machine holds **2,272,143 files** and walking all of
//! it takes 48 seconds. Almost none of that is anybody's work: it is package
//! caches, build output, and the state that development tools keep. Skipping
//! what `.gitignore` already says to skip, plus a short list of directories
//! that are noise everywhere, leaves **42,976 files walked in 1.6 seconds**.
//!
//! That is a fifty-three fold cut, and it is the whole reason this can be an
//! in-memory index of a few megabytes rather than a service holding hundreds.
//! For comparison, a whole-volume indexer on the same machine holds 412 MB.
//!
//! # Why there is a bucket index
//!
//! Ranking all 42,976 entries per keystroke measures at 61 to 92 ms in a
//! release build, which is too slow to do while somebody is typing.
//!
//! The ranker already requires a match to begin where a word begins, so an
//! entry can only match a query if some word in its name starts with the
//! query's first letter. Grouping entries by those letters and ranking one
//! group cuts the work to **4.4% of the corpus on average and 27% at worst**,
//! which is a few milliseconds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::files::FileHit;

/// Directories that are noise on every machine.
///
/// `.gitignore` covers a project's own build output, and most of what is left
/// is caches belonging to tools rather than to the person using them. Nobody
/// searching for a file means the copy inside a package cache.
///
/// `AppData` is here because it is the largest single source of churn in a
/// Windows home folder and none of it is authored by hand. Anything genuinely
/// wanted from it can be added as a root of its own.
///
/// `.claude` is the same shape as `AppData` but lives one level up, so it was
/// missed. A coding assistant appends to its transcript every few seconds, and
/// each append looked like a file worth re-indexing: measured on this machine,
/// the whole 49,402-entry walk was running every 45 seconds while the launcher
/// sat hidden, against every 30 to 80 minutes with the assistant closed. None
/// of it is a file anybody searches for by name.
pub const NOISE: &[&str] = &[
    "node_modules",
    "target",
    ".git",
    "dist",
    "build",
    ".svelte-kit",
    "__pycache__",
    ".next",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    "AppData",
    ".claude",
    ".venv",
    "venv",
    "vendor",
    ".pnpm-store",
    ".cache",
];

/// Directories skipped only where a drive begins.
///
/// Separate from [`NOISE`], which is skipped wherever it appears. These are
/// Windows itself and the machinery around it: nobody searching for a file
/// means the one inside `Program Files`, and indexing them turns a drive from
/// a hundred thousand files into a million.
///
/// Only at depth one, so somebody who deliberately adds `C:\Windows` as a root
/// still gets it. Skipping a name everywhere would make that impossible to ask
/// for.
///
/// `Documents and Settings` is here because it is a junction that points back
/// into `Users`, and walking it indexes a home folder twice under two names.
pub const SYSTEM: &[&str] = &[
    "Windows",
    "Program Files",
    "Program Files (x86)",
    "ProgramData",
    "$Recycle.Bin",
    "System Volume Information",
    "Recovery",
    "PerfLogs",
    "Config.Msi",
    "Documents and Settings",
    "inetpub",
    "MSOCache",
    "$WinREAgent",
    "OneDriveTemp",
];

/// What kind of thing a drive is, which decides whether indexing it is sane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// An internal disk. The ordinary case, and the only one offered by
    /// default.
    Fixed,
    /// A stick or an external disk. Indexing one is fine and the index is
    /// wrong the moment it is unplugged, so it is opt in.
    Removable,
    /// A share, or a cloud folder pretending to be a disk. **Walking one can
    /// mean a round trip per directory, and on a cloud drive it can mean
    /// downloading the files.** Never offered by default.
    Network,
    /// A disc. Listed for completeness and never worth indexing.
    Optical,
}

/// A drive that could be indexed.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    /// Where it starts, as a root would be written: `C:\`.
    pub root: String,
    /// What the volume calls itself, when it says.
    pub label: String,
    pub kind: Kind,
    /// Whether Sill is indexing it right now.
    pub indexed: bool,
}

/// Every drive currently mounted.
///
/// Asked when somebody opens the settings that show them, never on a timer.
/// A drive appearing is something a person did, and they are looking at the
/// list when they did it.
#[cfg(windows)]
pub fn drives(roots: &[PathBuf]) -> Vec<Drive> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };

    // `GetDriveTypeW` answers with a bare number and this version of the
    // bindings does not name them, so they are named here. The values are
    // fixed by the Windows API and cannot change.
    const REMOVABLE: u32 = 2;
    const FIXED: u32 = 3;
    const REMOTE: u32 = 4;
    const CDROM: u32 = 5;

    let mounted = unsafe { GetLogicalDrives() };
    let mut found = Vec::new();

    for bit in 0..26u32 {
        if mounted & (1 << bit) == 0 {
            continue;
        }

        let letter = (b'A' + bit as u8) as char;
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();

        let kind = match unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) } {
            FIXED => Kind::Fixed,
            REMOVABLE => Kind::Removable,
            REMOTE => Kind::Network,
            CDROM => Kind::Optical,
            // Unknown, unrecognised, or a RAM disk. Treated as removable,
            // which is the cautious reading: offered, but never by default.
            _ => Kind::Removable,
        };

        let mut label = [0u16; 261];
        let named = unsafe {
            GetVolumeInformationW(
                PCWSTR(wide.as_ptr()),
                Some(&mut label),
                None,
                None,
                None,
                None,
            )
        }
        .is_ok();

        let label = if named {
            String::from_utf16_lossy(&label)
                .trim_end_matches('\0')
                .trim()
                .to_string()
        } else {
            String::new()
        };

        found.push(Drive {
            indexed: roots.iter().any(|r| same_root(r, &root)),
            root,
            label,
            kind,
        });
    }

    found
}

#[cfg(not(windows))]
pub fn drives(_roots: &[PathBuf]) -> Vec<Drive> {
    Vec::new()
}

/// Whether a configured root is this drive, however it was written.
///
/// `C:\`, `C:/` and `C:` all mean the same drive and a person may type any of
/// them. Getting this wrong shows a drive as unindexed while it is being
/// indexed, and offering to add it again is how a root ends up in the list
/// twice.
pub fn same_folder(root: &str, other: &str) -> bool {
    settled(root) == settled(other)
}

/// Whether a path sits inside any of the folders somebody narrowed to.
///
/// The folders arrive already settled and already ending in a separator, so
/// this is a prefix test and nothing more. Doing the settling here instead
/// would redo it for every candidate in the bucket.
fn under(path: &str, folders: &[String]) -> bool {
    let settled = settled(path);

    folders
        .iter()
        .any(|folder| settled.starts_with(folder.as_str()))
}

/// Whether a path sits inside any of a set of folders, settling them first.
///
/// The same question [`under`] answers, for callers holding a handful of paths
/// rather than a bucket of them. The search loop cannot use this: it would
/// settle the same folder list once per candidate.
pub fn inside_any(path: &str, folders: &[String]) -> bool {
    let folders: Vec<String> = folders
        .iter()
        .map(|folder| folder.trim())
        .filter(|folder| !folder.is_empty())
        .map(|folder| {
            let mut settled = settled(folder);
            settled.push('\\');
            settled
        })
        .collect();

    folders.is_empty() || under(path, &folders)
}

/// One folder, written one way.
///
/// Case is dropped because Windows does not care about it, and separators are
/// made to agree because a person may type either and both open the same
/// folder. Comparing what was typed rather than what it means is how the same
/// folder ends up in the list twice and gets read twice.
fn settled(path: &str) -> String {
    path.trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// The same question, for a root already parsed into a path.
fn same_root(root: &Path, drive: &str) -> bool {
    same_folder(&root.to_string_lossy(), drive)
}

/// Whether a change can alter what the index holds.
///
/// The index holds names and where they are, and nothing else. **Writing to a
/// file does not change either of them**, and writing to files is nearly
/// everything a watcher reports: every save in an editor, every log line,
/// every application touching its own state. Rebuilding for those is a walk of
/// the whole tree to arrive back at exactly the list already held.
///
/// What does change it: a file appearing, a file going away, and a file being
/// renamed, which is both at once.
pub fn changes_the_index(kind: &notify::EventKind) -> bool {
    use notify::event::{EventKind, ModifyKind};

    match kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Name(_)) => true,
        EventKind::Modify(_) | EventKind::Access(_) => false,
        // `Any` and `Other` are what a platform reports when it will not say.
        // Treated as a change, because missing one leaves the index wrong
        // until something else happens, and the floor limits what that costs.
        EventKind::Any | EventKind::Other => true,
    }
}

/// Whether a changed path is one the index would have contained.
///
/// The walk skips these directories, so a file appearing inside one changes
/// nothing about what a search would find, and rebuilding is pure cost. This
/// is the same list the walk uses, which is what makes the two agree.
///
/// A path with several components under watch is judged by all of them: a file
/// deep inside `node_modules` is not interesting no matter how interesting its
/// parent folder is.
pub fn worth_indexing(path: &Path, roots: &[PathBuf]) -> bool {
    // Judged from the root down, exactly as the walk judges it.
    //
    // The walk's filter only ever sees entries *below* a root, so a root of
    // `%TEMP%\\work` indexes everything in it however the path to it is
    // spelled. Checking the whole path instead made the watcher disagree:
    // every file under that root contains `AppData`, so nothing in a folder
    // somebody had deliberately added was ever noticed changing.
    let below = roots
        .iter()
        .find_map(|root| path.strip_prefix(root).ok())
        .unwrap_or(path);

    !below.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| NOISE.contains(&name))
    })
}

/// Where one file's path sits in the arena, and the two numbers about it that
/// a query can ask for.
///
/// Twenty-four bytes and no allocation of its own. The first version gave every
/// entry its own `Box<str>`, which measured at **25.8 MB of private memory**
/// for a home folder against 11.3 MB before the index existed: forty-nine
/// thousand separate allocations, each with a header, for five megabytes of
/// actual text.
///
/// The name is not stored at all. A file name is always the tail of its own
/// path, so it is an offset rather than a second copy of the same bytes.
///
/// # Why size and time are here rather than read when asked
///
/// `size:` and `date:` have to compare every candidate a query reaches, and on
/// this machine's index that is a few thousand entries for an ordinary query
/// and all 49,451 for one that is nothing but operators. A `GetFileAttributesEx`
/// each is disk work per candidate on the keystroke path, which rule 23 does
/// not allow and which would make the two operators the slowest thing in the
/// launcher.
///
/// So they are held, and they cost **eight bytes an entry, 386 KB on a 49,451
/// entry index**, measured against 5.1 MB of path text and 1.1 MB of slots for
/// the same walk. `suite::real_operators` prints all three. The size is
/// kibibytes rounded up rather than bytes, so a 4 TB file still fits in a `u32`
/// and a one-byte file never reads as empty; nobody filters at a finer grain
/// than that. The time is seconds since the epoch, which lasts until 2106.
///
/// **Reading them costs nothing at walk time on Windows.** The directory scan
/// already returns size and write time in `WIN32_FIND_DATA`, and both `walkdir`
/// and `ignore` keep that around, so `DirEntry::metadata()` with links not
/// followed is a clone of a struct already in hand rather than a system call.
#[derive(Debug, Clone, Copy)]
struct Slot {
    /// Where the path starts in the arena.
    at: u32,
    /// Where it ends.
    end: u32,
    /// Where the name starts, which is somewhere between the two.
    name_at: u32,
    /// How big it is, in kibibytes rounded up. Zero for a directory.
    kib: u32,
    /// When it was last written, in seconds since the epoch.
    ///
    /// Zero when the filesystem would not say, which reads as the beginning of
    /// time and so falls outside every `date:` window. A file whose age is
    /// unknown is not one somebody meant by "changed this week".
    modified: u32,
    is_dir: bool,
    /// Whether the file is still there.
    ///
    /// A deletion cannot remove the text from the arena, because every slot
    /// after it is an offset and they would all move. So the slot stays and
    /// stops counting, and the space comes back at the next compaction.
    live: bool,
}

/// Everything Sill knows about the files under its roots.
///
/// Immutable once built. Rebuilding produces a new one and swaps it in, so a
/// search in progress never sees a half-built index and never waits for one.
#[derive(Debug, Default)]
pub struct Catalog {
    /// Everything the last full walk found, in one allocation.
    ///
    /// See [`Slot`] for why. Behind an `Arc` because a patch produces a whole
    /// new catalog and this is the large part of it: sharing it is what makes
    /// patching cost the changed files rather than the whole index.
    base: Arc<str>,
    /// What has been added since that walk.
    ///
    /// Offsets are into the two arenas end to end, so anything at or past
    /// `base.len()` lands here. One number keeps addressing what it was
    /// instead of every slot carrying which half it lives in.
    added: String,
    entries: Vec<Slot>,
    /// Letters that begin a word, to the entries whose names contain them.
    ///
    /// See the module note: this is what makes ranking cheap enough to do
    /// while somebody is typing.
    buckets: HashMap<char, Vec<u32>>,
    roots: Vec<PathBuf>,
    /// Slots whose file is gone, so compaction can be decided on a number
    /// rather than by counting them.
    dead: u32,
}

/// How long to wait for a network share to say whether it is there.
///
/// Long enough for a share on a LAN that is awake, short enough that a
/// laptop off the network is not held up by it.
const REACHABLE_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether a path names a network share rather than a local disk.
fn is_a_share(root: &Path) -> bool {
    root.to_str().is_some_and(|text| {
        // Both separators, because a root is written by hand in Settings and
        // `//server/share` is a thing people type.
        text.starts_with("\\\\") || text.starts_with("//")
    })
}

/// Whether a root is there, without waiting on a share that is not.
///
/// `is_dir()` on an unreachable UNC path blocks until SMB gives up, which is
/// tens of seconds. It runs on the indexing thread at startup and again on
/// every rebuild, so a laptop carried off the network spends that long doing
/// nothing, once per root, every time anything changes.
fn reachable(root: &Path) -> bool {
    let asking = root.to_path_buf();
    answers_within(is_a_share(root), REACHABLE_WITHIN, move || asking.is_dir())
}

/// Asks, and gives up waiting.
///
/// Split from [`reachable`] so a test can supply a question that is slow on
/// purpose. Pointing the real one at an address nothing answers proves
/// nothing: the first version of that test passed with the guard removed,
/// because this machine refuses a documentation address immediately. A test
/// that cannot fail is worse than no test, and one whose result depends on
/// how this network behaves today is exactly that.
///
/// Only a share pays for the guard. A local path answers immediately and
/// spawning a thread to ask would cost more than the question.
fn answers_within(
    is_share: bool,
    wait: std::time::Duration,
    ask: impl FnOnce() -> bool + Send + 'static,
) -> bool {
    if !is_share {
        return ask();
    }

    let (tx, rx) = std::sync::mpsc::channel();

    // Left to finish on its own if it is slow. It is one thread per share per
    // rebuild, blocked in the kernel rather than spinning, and SMB does
    // eventually answer. Detaching is the point: the walk stops waiting.
    std::thread::spawn(move || {
        let _ = tx.send(ask());
    });

    // A share that has not answered is treated as absent, which skips it for
    // this walk. The next rebuild asks again, so coming back onto the network
    // needs nothing but the next change.
    rx.recv_timeout(wait).unwrap_or(false)
}

impl Catalog {
    /// How many files are in it.
    pub fn len(&self) -> usize {
        self.entries.len() - self.dead as usize
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// What it was built from, for deciding whether a change is worth noticing.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Walks the roots and indexes what it finds.
    ///
    /// Parallel, bounded well below the core count: this runs while somebody is
    /// using their machine for something else, and a walk that saturates every
    /// core to finish a second sooner is a bad trade.
    pub fn build(roots: &[PathBuf]) -> Self {
        let mut paths = String::new();
        let mut entries = Vec::new();

        for root in roots {
            if !reachable(root) {
                continue;
            }

            let mut walker = ignore::WalkBuilder::new(root);
            walker
                // The rules a developer already wrote down. Respecting them is
                // most of why this is forty thousand files and not two million.
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .ignore(true)
                .hidden(true)
                .follow_links(false)
                .threads(threads())
                .filter_entry(|entry| {
                    let Some(name) = entry.file_name().to_str() else {
                        return false;
                    };

                    if NOISE.contains(&name) {
                        return false;
                    }

                    // Depth one is a direct child of the root, so this only
                    // takes effect when the root is a whole drive. Adding
                    // `C:\Windows` deliberately still indexes it.
                    entry.depth() != 1 || !SYSTEM.contains(&name)
                });

            for found in walker.build().flatten() {
                let is_dir = found.file_type().is_some_and(|kind| kind.is_dir());

                // The root itself is not a result.
                if is_dir && found.path() == root {
                    continue;
                }

                // Free on Windows: links are not followed, so this hands back
                // the metadata the directory scan already returned rather than
                // asking the disk a second time. See [`Slot`].
                let facts = found.metadata().as_ref().map(facts).unwrap_or_default();

                if let Some(slot) = push(&mut paths, found.path(), is_dir, facts) {
                    entries.push(slot);
                }
            }
        }

        // The arena grew by doubling and is now permanent. Handing back what
        // that overshot by is the difference between an index that costs what
        // it holds and one that costs what it happened to allocate.
        paths.shrink_to_fit();
        entries.shrink_to_fit();

        let buckets = index(&paths, &entries);

        Self {
            base: paths.into(),
            added: String::new(),
            entries,
            buckets,
            roots: roots.to_vec(),
            dead: 0,
        }
    }

    /// A new catalog with some files added and some gone.
    ///
    /// ## Why this exists
    ///
    /// Every change used to re-walk every root: measured at **2,653 ms for
    /// 49,429 entries** on this machine's home folder, for a change that might
    /// be one saved file. A tool that writes continuously kept that walk
    /// running, which is the "3.4 s of Rust core per 30 s of writing" the
    /// audit measured.
    ///
    /// ## Why it hands back a new one rather than mutating
    ///
    /// The catalog is behind an `ArcSwap` so searching takes no lock, and that
    /// is worth keeping. `ArcSwap` makes reads free; it does not make
    /// read-modify-write safe. So a patch builds the next catalog and swaps it
    /// in, exactly as a rebuild does.
    ///
    /// What makes that cheap is that the large part is shared. `base` holds
    /// the walked text and cloning it is a pointer. Only the entries, the
    /// buckets and the small second arena are copied, which together are under
    /// a tenth of the index.
    ///
    /// ## What it costs
    ///
    /// Measured on this machine's home folder, 49,429 entries: a walk is
    /// **2,759 ms**, one added file is **890 us**, and a thousand at once is
    /// **209 ms**. The bulk figure is linear in entries times changed paths,
    /// because each changed path is looked for in the index; it is left that
    /// way because a thousand-file change is a branch checkout rather than
    /// something that happens while somebody types, and 209 ms of it is still
    /// an order better than walking.
    ///
    /// Nothing when there is nothing to do, or when enough slots are dead that
    /// a walk should reclaim them instead.
    pub fn apply(&self, added: &[PathBuf], removed: &[PathBuf]) -> Option<Self> {
        if (added.is_empty() && removed.is_empty()) || self.wants_compacting() {
            return None;
        }

        let mut entries = self.entries.clone();
        let mut buckets = self.buckets.clone();
        let mut arena = self.added.clone();
        let mut dead = self.dead;
        let split = self.base.len();

        // Gone first, and it has to be.
        //
        // When the same path is in both lists, which is a file deleted and
        // written again before things settled, doing the additions first means
        // `holds` sees the slot that is about to die, calls the addition a
        // duplicate and skips it, and then the removal kills it: the file ends
        // up missing from the index while sitting on disk. Removing first
        // leaves nothing for the addition to collide with.
        //
        // A rename to a different name does not care about the order, which is
        // what this comment used to claim. A sabotage of the ordering disproved
        // it by passing.
        for path in removed {
            let Some(text) = path.to_str() else {
                continue;
            };

            for slot in entries.iter_mut() {
                if !slot.live {
                    continue;
                }

                let held = span(&self.base, &arena, split, slot.at, slot.end);

                if same_path(held, text) {
                    slot.live = false;
                    dead += 1;
                    // A path is in the index at most once, because `holds`
                    // refuses a second one. So there is nothing further to
                    // find, and on a bulk delete this is the difference
                    // between scanning the index once per file and scanning
                    // half of it.
                    break;
                }
            }
        }

        for path in added {
            let Some(text) = path.to_str() else {
                continue;
            };

            // A watcher reports a write to an existing file as a create on
            // some filesystems, and indexing it twice would list it twice.
            //
            // Asked of the entries being built rather than of `self`. Asking
            // the original meant a file removed and added in the same batch
            // was refused as a duplicate of the slot the removal had just
            // struck out, so it left the index while sitting on disk.
            let known = entries
                .iter()
                .filter(|slot| slot.live)
                .any(|slot| same_path(span(&self.base, &arena, split, slot.at, slot.end), text));

            if known {
                continue;
            }

            // One question of the disk rather than two. This used to ask
            // `is_dir()`, which is a `GetFileAttributesEx` of its own; asking
            // for the whole metadata answers that as well as the size and the
            // write time, so holding two more numbers costs a patch nothing.
            let md = path.metadata().ok();
            let is_dir = md.as_ref().is_some_and(|md| md.is_dir());
            let facts = md.as_ref().map(facts).unwrap_or_default();

            let Some(slot) = push_after(&mut arena, split, path, is_dir, facts) else {
                continue;
            };

            let at = entries.len() as u32;

            // Taken from the arena rather than the path, so the bucket is
            // keyed on exactly the bytes a search will compare against.
            let name = span(&self.base, &arena, split, slot.name_at, slot.end).to_string();

            for letter in word_starts(&name) {
                buckets.entry(letter).or_default().push(at);
            }

            entries.push(slot);
        }

        Some(Self {
            // A pointer, not the megabytes behind it. This is the whole reason
            // a patch costs what changed rather than what is indexed.
            base: self.base.clone(),
            added: arena,
            entries,
            buckets,
            roots: self.roots.clone(),
            dead,
        })
    }

    /// Whether enough of the index is dead that a walk would be worth it.
    ///
    /// A dead slot costs a comparison in every search that reaches it, and the
    /// bytes it still holds. One is nothing; a third of the index is a slower
    /// search and memory held for files that are gone.
    fn wants_compacting(&self) -> bool {
        if self.entries.len() <= 64 {
            return false;
        }

        // Either a third of the slots are dead, or the arena has grown by half
        // again since the walk. The second matters on its own: renaming files
        // in a loop adds a live slot and kills one every time, so the dead
        // fraction can sit still while the text doubles.
        self.dead as usize * 3 > self.entries.len() || self.added.len() * 2 > self.base.len()
    }

    /// The text between two offsets, from whichever arena holds it.
    ///
    /// Every path is written in one piece, so a slot never straddles the two
    /// and this never has to join anything.
    fn text(&self, at: u32, end: u32) -> &str {
        span(&self.base, &self.added, self.base.len(), at, end)
    }

    /// How much path text is held, across both arenas.
    ///
    /// Includes what dead slots still point at, which is the number
    /// compaction exists to bring down.
    pub fn held(&self) -> usize {
        self.base.len() + self.added.len()
    }

    fn path(&self, slot: &Slot) -> &str {
        self.text(slot.at, slot.end)
    }

    fn name(&self, slot: &Slot) -> &str {
        self.text(slot.name_at, slot.end)
    }

    /// The files a query matches, best first.
    ///
    /// Ranked by the same code that ranks everything else, so a file behaves
    /// like every other row rather than having a second idea of what a good
    /// match is.
    pub fn search(&self, query: &str, limit: usize, only_in: &[String]) -> Vec<FileHit> {
        self.search_at(query, limit, only_in, now())
    }

    /// The same, with the clock passed in, so `date:` can be tested.
    fn search_at(&self, query: &str, limit: usize, only_in: &[String], now: u32) -> Vec<FileHit> {
        if self.entries.is_empty() {
            return Vec::new();
        }

        let (asked, filters) = operators_at(query, now);
        let query = asked.trim();
        let filtering = !filters.asked_for_nothing();

        // A query of nothing but operators is still a question: `ext:pdf` on
        // its own means every PDF. What is not a question is nothing at all.
        if query.is_empty() && !filtering {
            return Vec::new();
        }

        // Compared the way folders are compared everywhere else here: without
        // case, and with the separators made to agree. Somebody typing
        // `C:/work` into the settings means the same folder as `C:\work`.
        let inside: Vec<String> = only_in
            .iter()
            .map(|folder| folder.trim())
            .filter(|folder| !folder.is_empty())
            .map(|folder| {
                let mut settled = settled(folder);
                // With the separator on the end, so `C:\work` does not also
                // match `C:\workshop`.
                settled.push('\\');
                settled
            })
            .collect();

        let candidates = if query.is_empty() {
            // No letter to look up, so there is no bucket and the whole index
            // is in the running. Only reachable by typing an operator and
            // nothing else, and the filters below are integer comparisons, so
            // it is a scan rather than a ranking pass.
            std::borrow::Cow::Owned((0..self.entries.len() as u32).collect())
        } else {
            self.candidates(query)
        };

        let needle: Vec<char> = query.to_lowercase().chars().collect();

        // `None` when the query was only operators, and then every row has it,
        // so the sort falls through to the name length as it should.
        let mut scored: Vec<(Option<crate::registry::MatchClass>, usize, u32)> = Vec::new();

        for &at in candidates.iter() {
            let slot = self.entries[at as usize];

            // A deleted file keeps its slot and its place in the buckets until
            // the next compaction, so this is where it stops being an answer.
            if !slot.live {
                continue;
            }

            let name = self.name(&slot);

            // Before the folder test, which allocates a settled copy of the
            // path per candidate. These are integer comparisons and at worst
            // one look at the tail of the name.
            if filtering && !filters.allows(&slot, name) {
                continue;
            }

            if !inside.is_empty() && !under(self.path(&slot), &inside) {
                continue;
            }

            if needle.is_empty() {
                /*
                 * Nothing to match on, so the order is whatever the operator
                 * makes sensible.
                 *
                 * `content:` with no name is "somewhere in what I have been
                 * working on", and the files worth opening for that are the
                 * ones touched most recently. Every other operator-only query
                 * keeps the order it had, which is the shortest name first.
                 */
                let rank = match filters.content() {
                    Some(_) => (u32::MAX - slot.modified) as usize,
                    None => name.chars().count(),
                };

                scored.push((None, rank, at));
                continue;
            }

            if let Some(class) = matches(&needle, name) {
                scored.push((Some(class), name.chars().count(), at));
            }
        }

        // The same order the rest of the launcher uses: the kind of match
        // first, then the shorter name, then something stable so two equally
        // good answers do not swap places between keystrokes.
        scored.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| {
                self.path(&self.entries[a.2 as usize])
                    .cmp(self.path(&self.entries[b.2 as usize]))
            })
        });
        scored.truncate(limit);

        scored
            .into_iter()
            .map(|(_, _, at)| {
                let slot = self.entries[at as usize];
                FileHit {
                    name: self.name(&slot).to_string(),
                    path: self.path(&slot).to_string(),
                    is_dir: slot.is_dir,
                    snippet: None,
                }
            })
            .collect()
    }

    /// Which entries are worth ranking for this query.
    ///
    /// Everything whose name has a word starting with the query's first
    /// letter. A match has to begin where a word begins, so nothing else can
    /// match, and this is the difference between ninety milliseconds and four.
    ///
    /// Falls back to the whole corpus when the query has no letter or digit in
    /// it at all, which is rare and cheap to be wrong about.
    fn candidates(&self, query: &str) -> std::borrow::Cow<'_, [u32]> {
        match key(query) {
            Some(letter) => match self.buckets.get(&letter) {
                Some(bucket) => std::borrow::Cow::Borrowed(bucket.as_slice()),
                None => std::borrow::Cow::Owned(Vec::new()),
            },
            None => std::borrow::Cow::Owned((0..self.entries.len() as u32).collect()),
        }
    }
}

/// Whether a file name matches, by the rules this index can actually deliver.
///
/// `match_name` and nothing more, except that a **mid-word substring is not a
/// match here**, and that exception is the whole reason this function exists
/// rather than the call being inline.
///
/// # Why the substring tier is dropped
///
/// The bucket index groups entries by the letters that begin a word in their
/// name, because the ranker requires a match to begin where a word begins. A
/// substring match does not, so an entry that would match that way is usually
/// not in the bucket the query looks in: typing `ignore` cannot find
/// `.gitignore`, because that name has one word and it starts with `g`.
///
/// It is *sometimes* in the bucket, when the query's first letter happens to
/// begin some unrelated word in the same name. `tore` finds
/// `stores-restore.txt` because `restore` starts with `t`... no: because
/// `txt` does. That is behaviour nobody can predict from the outside, and
/// "works when another part of the name happens to start with the same
/// letter" is worse than not working.
///
/// # Why not fix the bucket instead
///
/// Measured on the real index on this machine, 49,413 entries. Keyed by word
/// starts, a query looks at **9.2% of the corpus on average and 25.6% at
/// worst**, in 0.6 MB. Keyed by every letter a name contains, so that a
/// substring match could be delivered, it is **34.6% on average and 69.1% at
/// worst**, in 2.2 MB. Ranking the whole corpus is 61 to 92 ms, so that is a
/// keystroke going from about seven milliseconds to about twenty-five, and
/// nearly four times the memory, to add a tier that is already the weakest
/// evidence the ranker has.
fn matches(needle: &[char], name: &str) -> Option<crate::registry::MatchClass> {
    let (class, _) = crate::registry::match_name(needle, name)?;

    (class != crate::registry::MatchClass::TitleSubstring).then_some(class)
}

/// The letter a query will be looked up by.
///
/// The first letter or digit, rather than the first character. Somebody
/// searching for `.rs` means files whose name has a word starting with `r`,
/// and looking that up under `.` would find only the dotfiles.
fn key(query: &str) -> Option<char> {
    query
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_lowercase().next().unwrap_or(c))
}

/// Every letter that begins a word in a name.
///
/// Deliberately includes the letter after a dot, so `registry.rs` is found
/// under `r` twice over and under nothing else. Yields each letter once.
fn word_starts(name: &str) -> Vec<char> {
    let chars: Vec<char> = name.chars().collect();
    let mut found: Vec<char> = Vec::new();

    for (at, &ch) in chars.iter().enumerate() {
        if !ch.is_alphanumeric() {
            continue;
        }

        let starts = at == 0
            || !chars[at - 1].is_alphanumeric()
            || (ch.is_uppercase() && chars[at - 1].is_lowercase());

        if starts {
            let lower = ch.to_lowercase().next().unwrap_or(ch);
            if !found.contains(&lower) {
                found.push(lower);
            }
        }
    }

    found
}

/// Groups entries by the letters their words begin with.
fn index(paths: &str, entries: &[Slot]) -> HashMap<char, Vec<u32>> {
    let mut buckets: HashMap<char, Vec<u32>> = HashMap::new();

    for (at, slot) in entries.iter().enumerate() {
        let name = &paths[slot.name_at as usize..slot.end as usize];

        for letter in word_starts(name) {
            buckets.entry(letter).or_default().push(at as u32);
        }
    }

    for bucket in buckets.values_mut() {
        bucket.shrink_to_fit();
    }

    buckets
}

/// Whether two paths name the same file.
///
/// Case-insensitive and separator-insensitive, which is what Windows means by
/// the same file and what `settled` does everywhere else here. Written out
/// rather than calling `settled` because this runs once per indexed entry for
/// every deleted path, and two allocations per comparison would be the
/// expensive part of a patch.
///
/// Byte length is a valid first test: neither folding ASCII case nor swapping
/// a separator changes how many bytes a path takes.
fn same_path(one: &str, two: &str) -> bool {
    one.len() == two.len()
        && one.chars().zip(two.chars()).all(|(a, b)| {
            let a = if a == '/' { '\\' } else { a };
            let b = if b == '/' { '\\' } else { b };
            a.eq_ignore_ascii_case(&b)
        })
}

/// The text between two offsets, from whichever of the two arenas holds it.
///
/// Offsets address the pair end to end, so anything at or past `split` is in
/// the second. A path is always written in one piece, so no span ever straddles
/// them.
fn span<'a>(base: &'a str, added: &'a str, split: usize, at: u32, end: u32) -> &'a str {
    let (at, end) = (at as usize, end as usize);

    if at < split {
        &base[at..end]
    } else {
        &added[at - split..end - split]
    }
}

/// The two numbers about a file that a query can ask for.
///
/// Read from metadata the caller already has rather than fetched, because on
/// Windows the walk is handed both by the directory scan and asking the disk
/// again would turn a free index into one syscall per file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Facts {
    kib: u32,
    modified: u32,
}

/// Reads them out of a `Metadata`, saturating rather than failing.
///
/// A file larger than four terabytes reads as four terabytes, and one written
/// after 2106 reads as 2106. Both are wrong in the same direction as the
/// comparison they will be used in, so a `size:>1gb` still finds the huge one.
fn facts(md: &std::fs::Metadata) -> Facts {
    Facts {
        // Rounded up, so a file with anything in it is never zero kibibytes and
        // `size:>0` means what it looks like it means.
        kib: u32::try_from(md.len().div_ceil(1024)).unwrap_or(u32::MAX),
        modified: md
            .modified()
            .ok()
            .and_then(|when| when.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| u32::try_from(since.as_secs()).unwrap_or(u32::MAX))
            .unwrap_or(0),
    }
}

/// Now, in seconds since the epoch.
fn now() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|since| u32::try_from(since.as_secs()).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

/// Appends a path to the second arena and returns where it landed.
///
/// The same as [`push`] except that offsets carry on from the end of the first
/// arena, so a slot made here addresses the pair the same way one made by the
/// walk does.
fn push_after(
    arena: &mut String,
    split: usize,
    path: &Path,
    is_dir: bool,
    facts: Facts,
) -> Option<Slot> {
    let name = path.file_name()?.to_str()?;
    let full = path.to_str()?;
    let name_from = full.len().checked_sub(name.len())?;

    let at = u32::try_from(split + arena.len()).ok()?;
    let end = u32::try_from(split + arena.len() + full.len()).ok()?;
    arena.push_str(full);

    Some(Slot {
        at,
        end,
        name_at: at + name_from as u32,
        kib: facts.kib,
        modified: facts.modified,
        is_dir,
        live: true,
    })
}

/// Appends a walked path to the arena and returns where it landed.
///
/// Nothing if the path has no name or is not valid text. A path Windows cannot
/// render as UTF-8 is one nobody is going to type either.
fn push(paths: &mut String, path: &Path, is_dir: bool, facts: Facts) -> Option<Slot> {
    let name = path.file_name()?.to_str()?;
    let full = path.to_str()?;

    // The name is the tail of the path, so its start is the path's length
    // minus its own. Bytes rather than characters, because it indexes a `str`.
    let name_from = full.len().checked_sub(name.len())?;

    let at = u32::try_from(paths.len()).ok()?;
    let end = u32::try_from(paths.len() + full.len()).ok()?;
    paths.push_str(full);

    Some(Slot {
        at,
        end,
        name_at: at + name_from as u32,
        kib: facts.kib,
        modified: facts.modified,
        is_dir,
        live: true,
    })
}

/// How many threads to walk with.
///
/// Half the machine, capped. The walk runs while somebody is doing something
/// else, and rule 23 is about a launcher that costs nothing when it is not
/// being used, which includes not taking the machine over to build an index.
fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(2, 6))
        .unwrap_or(2)
}

// --------------------------------------------------- asking for more than a name

/// A range of numbers a slot has to fall inside, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Between {
    low: u32,
    high: u32,
}

impl Between {
    fn holds(&self, value: u32) -> bool {
        value >= self.low && value <= self.high
    }
}

/// What a query asked for beyond a name.
///
/// `ext:rs`, `size:>1mb` and `date:week`, which narrow a search rather than
/// being one. Everything here is compared against numbers and bytes already in
/// the index, so a filter is a handful of integer comparisons per candidate and
/// never a question put to the disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// Extensions, lower case and without the dot. Any one of them will do.
    ext: Vec<String>,
    /// Size in kibibytes.
    size: Option<Between>,
    /// Last written, in seconds since the epoch.
    modified: Option<Between>,
    /// Words to look for inside the file itself.
    ///
    /// Unlike the three above, this cannot be answered from the index: the
    /// index holds names. It is carried here so one parser reads every
    /// operator, and applied by [`crate::content`] after the name search has
    /// narrowed the field to something worth opening.
    content: Option<String>,
}

impl Filters {
    /// Whether the query narrowed anything, which is the ordinary case.
    ///
    /// Hoisted out of the per-candidate loop by the caller: typing does not pay
    /// for operators beyond one bool.
    pub fn asked_for_nothing(&self) -> bool {
        self.ext.is_empty()
            && self.size.is_none()
            && self.modified.is_none()
            && self.content.is_none()
    }

    /// What to look for inside a file, when the query asked for that.
    ///
    /// Deliberately not part of [`Self::allows`], which runs once per
    /// candidate against numbers the index already holds. Opening a file is
    /// not that, and doing it there would put a disk read on the keystroke
    /// path for every entry in the index.
    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    /// Whether one indexed file answers what was asked.
    fn allows(&self, slot: &Slot, name: &str) -> bool {
        if let Some(size) = self.size {
            // A directory has no size worth comparing, so asking about size is
            // asking about files.
            if slot.is_dir || !size.holds(slot.kib) {
                return false;
            }
        }

        if let Some(modified) = self.modified {
            if !modified.holds(slot.modified) {
                return false;
            }
        }

        if !self.ext.is_empty() && !self.ext.iter().any(|want| has_extension(name, want)) {
            return false;
        }

        true
    }
}

/// Whether a file name ends in one particular extension.
///
/// Written out rather than going through `Path`, because this runs once per
/// candidate and `Path::extension` on a borrowed name would allocate nothing
/// but would still walk the name twice. A dot at the start does not count:
/// `.gitignore` has no extension, it has a name beginning with a dot.
fn has_extension(name: &str, want: &str) -> bool {
    let Some(dot) = name.rfind('.') else {
        return false;
    };

    dot != 0 && name[dot + 1..].eq_ignore_ascii_case(want)
}

/// Splits a typed query into the text to match on and what it asked to narrow.
///
/// # What this costs a query with no operator in it
///
/// One scan for a colon, and then nothing. A query without one is handed back
/// **borrowed**, so the ordinary keystroke allocates nothing here and the
/// search that follows is the search that was there before. A query that has a
/// colon but no operator, which is what `C:\work` is, costs a walk over its
/// words and still allocates nothing.
///
/// The allocation only happens once a term is actually taken out, because that
/// is the only case where the remaining text is not a piece of the input.
pub fn operators(query: &str) -> (std::borrow::Cow<'_, str>, Filters) {
    operators_at(query, now())
}

/// The same, with the clock passed in.
///
/// Split out because `date:week` means "the last seven days" and a test that
/// asks what today is cannot say what the answer should be. This is the same
/// shape as `verdict` in `files`: the rule is worth pinning down, and the fact
/// about this particular moment is not part of it.
fn operators_at(query: &str, now: u32) -> (std::borrow::Cow<'_, str>, Filters) {
    let mut filters = Filters::default();

    // Every operator has one, so a query without one cannot contain any, and
    // this is the whole of what ordinary typing pays.
    if !query.contains(':') {
        return (std::borrow::Cow::Borrowed(query), filters);
    }

    // Built only once something has actually been taken out. Until then the
    // input is still the answer and copying it would be work for nothing.
    let mut kept: Option<String> = None;

    for term in query.split_whitespace() {
        if read_operator(term, now, &mut filters) {
            if kept.is_none() {
                // What came before the first operator, which is a piece of the
                // input and so is copied once rather than word by word.
                let from = term.as_ptr() as usize - query.as_ptr() as usize;
                kept = Some(query[..from].trim_end().to_string());
            }

            continue;
        }

        if let Some(text) = kept.as_mut() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(term);
        }
    }

    match kept {
        Some(text) => (std::borrow::Cow::Owned(text), filters),
        None => (std::borrow::Cow::Borrowed(query), filters),
    }
}

/// Reads one word as an operator, saying whether it was one.
///
/// A word with a name this understands but a value it does not is **not** an
/// operator, so it stays in the query as text. That is what makes typing one
/// letter at a time behave: `size:` and `size:>` on the way to `size:>1mb`
/// match nothing rather than suddenly listing every file on the machine.
fn read_operator(term: &str, now: u32, into: &mut Filters) -> bool {
    let Some((name, value)) = term.split_once(':') else {
        return false;
    };

    if value.is_empty() {
        return false;
    }

    match name.to_ascii_lowercase().as_str() {
        "ext" => match read_extensions(value) {
            Some(mut found) => {
                into.ext.append(&mut found);
                true
            }
            None => false,
        },
        "size" => match read_size(value) {
            Some(range) => {
                into.size = Some(both_of(into.size, range));
                true
            }
            None => false,
        },
        "date" => match read_date(value, now) {
            Some(range) => {
                into.modified = Some(both_of(into.modified, range));
                true
            }
            None => false,
        },
        // The first one wins. A second is left in the query as text, which is
        // the same answer the others give to a value they cannot read: a word
        // that was not taken out is a word to match names on.
        "content" => match into.content {
            Some(_) => false,
            None => {
                into.content = Some(value.to_string());
                true
            }
        },
        _ => false,
    }
}

/// Two of the same operator mean both, not the last one.
///
/// `size:>1mb size:<4mb` is the only way to write a band, since one term
/// carries one comparison. Taking the last would make the first silently
/// nothing, which is worse than a narrow answer.
fn both_of(had: Option<Between>, now: Between) -> Between {
    match had {
        Some(before) => Between {
            low: before.low.max(now.low),
            high: before.high.min(now.high),
        },
        None => now,
    }
}

/// `ext:rs` or `ext:.rs` or `ext:rs,md`.
fn read_extensions(value: &str) -> Option<Vec<String>> {
    let found: Vec<String> = value
        .split(',')
        .map(|one| one.trim().trim_start_matches('.'))
        .filter(|one| !one.is_empty())
        .map(|one| one.to_ascii_lowercase())
        .collect();

    (!found.is_empty()).then_some(found)
}

/// `size:>1mb`, `size:<=500kb`, `size:1gb`.
///
/// A bare value reads as "at least", which is not what a whole-volume indexer
/// means by it: there, `size:1mb` is a file of exactly that many bytes.
/// Deliberately different, because an exact byte count is a thing nobody knows
/// and so a thing nobody is searching for, and answering nothing to somebody
/// who typed `size:100mb` looking for the big files is a worse default than
/// answering the big files.
fn read_size(value: &str) -> Option<Between> {
    let (compare, rest) = comparator(value);
    let kib = read_bytes(rest)?.div_ceil(1024);
    // Held as a `u32` of kibibytes, so a threshold past four terabytes is the
    // largest thing the index can describe rather than a wrap to nothing.
    let kib = u32::try_from(kib).unwrap_or(u32::MAX);

    Some(match compare {
        Compare::More => Between {
            low: kib.saturating_add(1),
            high: u32::MAX,
        },
        Compare::AtLeast => Between {
            low: kib,
            high: u32::MAX,
        },
        Compare::Less => Between {
            low: 0,
            high: kib.saturating_sub(1),
        },
        Compare::AtMost => Between { low: 0, high: kib },
        Compare::Exactly => Between {
            low: kib,
            high: kib,
        },
    })
}

/// How a value is being compared, and what is left of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Compare {
    More,
    AtLeast,
    Less,
    AtMost,
    Exactly,
}

fn comparator(value: &str) -> (Compare, &str) {
    for (mark, compare) in [
        (">=", Compare::AtLeast),
        ("<=", Compare::AtMost),
        (">", Compare::More),
        ("<", Compare::Less),
        ("=", Compare::Exactly),
    ] {
        if let Some(rest) = value.strip_prefix(mark) {
            return (compare, rest);
        }
    }

    // Nothing said, so "at least". See [`read_size`].
    (Compare::AtLeast, value)
}

/// A number with an optional unit, in bytes.
///
/// Binary units, because that is what Windows shows in a file's properties and
/// what somebody comparing against a size they read there means.
fn read_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    let digits = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());

    let number: u64 = value.get(..digits)?.parse().ok()?;

    let scale = match value[digits..].trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1u64,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" => 1024u64 * 1024 * 1024 * 1024,
        _ => return None,
    };

    number.checked_mul(scale)
}

/// `date:today`, `date:week`, `date:7d`, `date:6h`.
///
/// Always "written within the last so long", and never a comparison. A `>` on a
/// duration reads two ways round: greater than seven days could be the date or
/// the age, and the two mean opposite sets of files. One meaning is better than
/// a coin toss.
///
/// The windows roll from now rather than falling on calendar boundaries, so
/// `date:today` is the last twenty-four hours rather than since midnight.
/// Midnight needs the machine's timezone, which needs a date library this does
/// not have, and the difference is never the difference between finding a file
/// and not.
fn read_date(value: &str, now: u32) -> Option<Between> {
    let value = value.trim().to_ascii_lowercase();

    let seconds: u64 = match value.as_str() {
        "today" => 24 * 60 * 60,
        "week" => 7 * 24 * 60 * 60,
        "month" => 30 * 24 * 60 * 60,
        "year" => 365 * 24 * 60 * 60,
        _ => {
            let digits = value
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(value.len());

            let number: u64 = value.get(..digits)?.parse().ok()?;

            let scale = match &value[digits..] {
                "h" => 60 * 60,
                "d" => 24 * 60 * 60,
                "w" => 7 * 24 * 60 * 60,
                "m" => 30 * 24 * 60 * 60,
                "y" => 365 * 24 * 60 * 60,
                _ => return None,
            };

            number.checked_mul(scale)?
        }
    };

    let ago = u32::try_from(seconds).unwrap_or(u32::MAX);

    Some(Between {
        low: now.saturating_sub(ago),
        // Not `now`. A clock that has been put back, or a file copied off a
        // machine an hour ahead, leaves a write time in the future, and those
        // are the most recently written things there are.
        high: u32::MAX,
    })
}

// ------------------------------------------------------------ keeping it

/// What the saved index starts with, so an old or foreign file is not read.
///
/// The version changes whenever the layout below does. A cache written by a
/// different version is not migrated, it is ignored and rebuilt: it is a copy
/// of something that can be regenerated in seconds, and migration code for a
/// throwaway file is code that can be wrong.
const MAGIC: &[u8; 8] = b"SILLIDX2";

impl Catalog {
    /// Writes the index where the next start can find it.
    ///
    /// Walking a whole drive takes nine seconds, which is far too long to do
    /// before somebody can search. Every launcher that indexes files keeps one
    /// of these: a whole-volume indexer on this machine holds a 154 MB
    /// database on disk for the same reason.
    ///
    /// Written whole and then renamed, so a start that happens during a save
    /// reads either the old file or the new one and never half of either.
    /// Saving is also how the index is compacted.
    ///
    /// Only living slots are written, and their text is copied out in order,
    /// so the offsets on disk are already closed up. What comes back on the
    /// next start has one arena and nothing dead in it, which is why the file
    /// format did not have to learn about either.
    pub fn save(&self, to: &Path) -> std::io::Result<()> {
        let mut out =
            Vec::with_capacity(self.base.len() + self.added.len() + self.entries.len() * 24 + 64);

        out.extend_from_slice(MAGIC);

        // The roots are stored so the file can be recognised as answering a
        // different question than the one now being asked. Changing which
        // folders are indexed makes the saved index wrong rather than stale.
        put_u32(&mut out, self.roots.len() as u32);
        for root in &self.roots {
            let text = root.to_string_lossy();
            put_u32(&mut out, text.len() as u32);
            out.extend_from_slice(text.as_bytes());
        }

        // Built first, because the arena's length has to be written before
        // the arena and neither is known until the dead have been dropped.
        let mut text = String::with_capacity(self.base.len() + self.added.len());
        let mut kept: Vec<Slot> = Vec::with_capacity(self.entries.len() - self.dead as usize);

        for slot in self.entries.iter().filter(|slot| slot.live) {
            let at = text.len() as u32;
            let name_from = slot.name_at - slot.at;

            text.push_str(self.path(slot));

            kept.push(Slot {
                at,
                end: at + (slot.end - slot.at),
                name_at: at + name_from,
                kib: slot.kib,
                modified: slot.modified,
                is_dir: slot.is_dir,
                live: true,
            });
        }

        put_u32(&mut out, text.len() as u32);
        out.extend_from_slice(text.as_bytes());

        put_u32(&mut out, kept.len() as u32);
        for slot in &kept {
            put_u32(&mut out, slot.at);
            put_u32(&mut out, slot.end);
            put_u32(&mut out, slot.name_at);
            put_u32(&mut out, slot.kib);
            put_u32(&mut out, slot.modified);
            put_u32(&mut out, u32::from(slot.is_dir));
        }

        let beside = to.with_extension("writing");
        std::fs::write(&beside, &out)?;
        std::fs::rename(&beside, to)
    }

    /// Reads back a saved index, if there is one and it answers this question.
    ///
    /// Returns nothing for anything unexpected rather than an error. A cache
    /// is an optimisation: every reason it might not load ends the same way,
    /// with a walk that would have happened anyway.
    pub fn load(from: &Path, roots: &[PathBuf]) -> Option<Self> {
        let raw = std::fs::read(from).ok()?;
        let mut at = 0usize;

        if raw.len() < MAGIC.len() || &raw[..MAGIC.len()] != MAGIC {
            return None;
        }
        at += MAGIC.len();

        let saved_roots = take_u32(&raw, &mut at)? as usize;
        let mut had: Vec<PathBuf> = Vec::with_capacity(saved_roots.min(64));
        for _ in 0..saved_roots {
            let len = take_u32(&raw, &mut at)? as usize;
            let text = std::str::from_utf8(raw.get(at..at + len)?).ok()?;
            at += len;
            had.push(PathBuf::from(text));
        }

        /*
         * Indexed somewhere else. Not stale, simply about other folders.
         *
         * Compared as folders rather than as paths. `Path` equality is
         * component by component, so a separator or a trailing slash was
         * never the problem: `C:/Users` and `C:\Users\` already matched. Case
         * was, and Windows does not care about case. A root capitalised
         * differently threw the whole index away and walked the disk again
         * for three seconds to rebuild exactly what it had.
         *
         * `same_folder` is the rule the drive list already uses, and its
         * comment says the same thing about the same characters.
         */
        let unchanged = had.len() == roots.len()
            && had.iter().zip(roots).all(|(before, now)| {
                same_folder(&before.to_string_lossy(), &now.to_string_lossy())
            });

        if !unchanged {
            return None;
        }

        let arena = take_u32(&raw, &mut at)? as usize;
        let paths = std::str::from_utf8(raw.get(at..at + arena)?)
            .ok()?
            .to_string();
        at += arena;

        let count = take_u32(&raw, &mut at)? as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let slot = Slot {
                at: take_u32(&raw, &mut at)?,
                end: take_u32(&raw, &mut at)?,
                name_at: take_u32(&raw, &mut at)?,
                kib: take_u32(&raw, &mut at)?,
                modified: take_u32(&raw, &mut at)?,
                is_dir: take_u32(&raw, &mut at)? != 0,
                // Nothing dead is ever written, so everything read is alive.
                live: true,
            };

            // The file is on disk and anything may have happened to it. Every
            // offset is checked against the arena rather than trusted, because
            // a wrong one would slice a string out of bounds and take the
            // process with it.
            let sane = slot.at <= slot.name_at
                && slot.name_at <= slot.end
                && slot.end as usize <= paths.len()
                && paths.is_char_boundary(slot.at as usize)
                && paths.is_char_boundary(slot.name_at as usize)
                && paths.is_char_boundary(slot.end as usize);

            if !sane {
                return None;
            }

            entries.push(slot);
        }

        // Rebuilt rather than stored. It is derived from the names, it takes a
        // fraction of what the walk takes, and a stored one is another thing
        // that can disagree with what it was derived from.
        let buckets = index(&paths, &entries);

        Some(Self {
            base: paths.into(),
            added: String::new(),
            entries,
            buckets,
            roots: roots.to_vec(),
            dead: 0,
        })
    }
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn take_u32(raw: &[u8], at: &mut usize) -> Option<u32> {
    let bytes = raw.get(*at..*at + 4)?;
    *at += 4;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows does not care about case, and neither should the index.
    ///
    /// The saved index carries the roots it answers for. Rust compares two
    /// `Path`s component by component, so a separator or a trailing slash was
    /// never the problem here: `C:/Users` and `C:\Users\` already matched.
    /// Case was. A root typed as `c:\users\brandon`, or arriving from a folder
    /// dialog capitalised differently from the one in the preferences, threw
    /// away a fifty thousand entry index and walked the disk for three seconds
    /// to rebuild exactly what it already had.
    #[test]
    fn an_index_is_kept_when_only_the_capitalisation_changed() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("files.bin");

        let built = Catalog::build(&[dir.path().to_path_buf()]);
        built.save(&file).expect("save");

        let shouted: Vec<PathBuf> = built
            .roots()
            .iter()
            .map(|root| PathBuf::from(root.to_string_lossy().to_uppercase()))
            .collect();

        assert!(
            Catalog::load(&file, &shouted).is_some(),
            "the index was discarded because a root was capitalised differently"
        );
    }

    #[test]
    fn a_share_is_told_from_a_local_disk() {
        assert!(is_a_share(Path::new(r"\\\\server\\share")));
        assert!(is_a_share(Path::new("//server/share")));

        assert!(!is_a_share(Path::new(r"C:\\Users")));
        assert!(!is_a_share(Path::new(r"C:\\")));
        assert!(!is_a_share(Path::new("")));
    }

    /// A share that does not answer is skipped rather than waited on.
    ///
    /// The question is slow on purpose rather than pointed at a real address,
    /// so this measures the guard instead of measuring the network.
    #[test]
    fn a_share_that_does_not_answer_is_given_up_on() {
        let began = std::time::Instant::now();
        let there = answers_within(true, std::time::Duration::from_millis(120), || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            true
        });
        let waited = began.elapsed();

        assert!(!there, "an answer that never came should read as absent");
        assert!(
            waited < std::time::Duration::from_secs(2),
            "waited {waited:?}, which is the block this exists to stop"
        );
    }

    /// And a share that does answer is believed.
    #[test]
    fn a_share_that_answers_in_time_is_believed() {
        assert!(answers_within(true, REACHABLE_WITHIN, || true));
        assert!(!answers_within(true, REACHABLE_WITHIN, || false));
    }

    /// A local disk is asked straight out, with no thread and no waiting.
    #[test]
    fn a_local_path_is_asked_directly() {
        let began = std::time::Instant::now();
        assert!(answers_within(
            false,
            std::time::Duration::from_secs(30),
            || true
        ));
        assert!(
            began.elapsed() < std::time::Duration::from_millis(100),
            "a local path should not go anywhere near the timeout"
        );
    }

    #[test]
    fn a_query_is_looked_up_by_its_first_letter() {
        assert_eq!(key("registry"), Some('r'));
        assert_eq!(key("Registry"), Some('r'));
        assert_eq!(key("  reg"), Some('r'));
    }

    #[test]
    fn an_extension_is_looked_up_by_the_extension_not_the_dot() {
        // Somebody typing ".rs" means files whose name has a word starting
        // with r. Looking that up under "." would find only the dotfiles,
        // which is the opposite of what was asked.
        assert_eq!(key(".rs"), Some('r'));
        assert_eq!(key("*.json"), Some('j'));
    }

    #[test]
    fn a_query_with_no_letters_has_no_bucket() {
        // Rare, and the caller falls back to looking at everything rather
        // than to finding nothing.
        assert_eq!(key("___"), None);
        assert_eq!(key(""), None);
    }

    #[test]
    fn a_name_is_filed_under_every_word_it_starts() {
        let mut found = word_starts("registry.rs");
        found.sort_unstable();
        assert_eq!(found, vec!['r']);

        let mut found = word_starts("my-file_name.txt");
        found.sort_unstable();
        assert_eq!(found, vec!['f', 'm', 'n', 't']);
    }

    #[test]
    fn a_camel_case_hump_starts_a_word() {
        // `ByteCodeGenerator` has no separators in it, and somebody looking
        // for it may well type "code".
        let mut found = word_starts("ByteCodeGenerator");
        found.sort_unstable();
        assert_eq!(found, vec!['b', 'c', 'g']);
    }

    #[test]
    fn a_letter_is_filed_once_however_often_it_repeats() {
        // The bucket is a list of entries, not of occurrences, and filing an
        // entry twice would rank it twice.
        assert_eq!(word_starts("test-tool.tar"), vec!['t']);
    }

    fn catalog(names: &[(&str, bool)]) -> Catalog {
        let mut paths = String::new();
        let entries: Vec<Slot> = names
            .iter()
            .filter_map(|(path, is_dir)| {
                push(&mut paths, Path::new(path), *is_dir, Facts::default())
            })
            .collect();

        Catalog {
            buckets: index(&paths, &entries),
            base: paths.into(),
            added: String::new(),
            entries,
            roots: Vec::new(),
            dead: 0,
        }
    }

    /// The bucket index never hides an entry the ranker would have matched.
    ///
    /// ## Why this is the test, rather than a mid-word case
    ///
    /// The audit called this "the bucket drops mid-word substring matches",
    /// and it does: `config` does not find `reconfigure.ts`. That is not the
    /// bucket's decision though. `match_name` requires a run to begin where a
    /// word begins, everywhere, deliberately and with measurements behind it,
    /// and the bucket is built to agree with exactly that rule.
    ///
    /// So the thing worth pinning is the agreement, not either half. The
    /// bucket is an optimisation: it may only ever remove entries the ranker
    /// was going to reject anyway. If somebody later relaxes `match_name` to
    /// accept a mid-word run, files would silently keep behaving the old way,
    /// because the entry never reaches the ranker at all. That is the failure
    /// this catches, and it is invisible without it.
    #[test]
    fn the_bucket_never_hides_what_the_ranker_would_have_matched() {
        let names = [
            (r"C:\work\reconfigure.ts", false),
            (r"C:\work\app-config.json", false),
            (r"C:\work\Registry.rs", false),
            (r"C:\work\my_notes.md", false),
            (r"C:\work\Visual Studio Code", true),
            (r"C:\work\wi-fi-setup.txt", false),
            (r"C:\work\TpcdMetadata", true),
            (r"C:\work\.gitignore", false),
            (r"C:\work\2024-08-28.log", false),
            (r"C:\work\zzz", true),
        ];

        let held = catalog(&names);

        // Every prefix of every name, which is what somebody typing produces,
        // plus a few mid-word fragments that are the case in question.
        let mut queries: Vec<String> = Vec::new();
        for (path, _) in names {
            let name = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            for take in 1..=name.chars().count().min(8) {
                queries.push(name.chars().take(take).collect());
            }

            // And from the middle, which is what the bucket cannot reach.
            if name.chars().count() > 4 {
                queries.push(name.chars().skip(2).take(4).collect());
            }
        }

        for query in queries {
            let needle: Vec<char> = query.to_lowercase().chars().collect();

            // What the ranker says about every entry, with no bucket at all.
            let mut everything: Vec<&str> = held
                .entries
                .iter()
                .map(|slot| held.name(slot))
                .filter(|name| matches(&needle, name).is_some())
                .collect();

            // What the search actually returns, which went through the bucket.
            let mut found: Vec<&str> = held
                .candidates(&query)
                .iter()
                .map(|&at| held.name(&held.entries[at as usize]))
                .filter(|name| matches(&needle, name).is_some())
                .collect();

            everything.sort_unstable();
            found.sort_unstable();

            assert_eq!(
                found, everything,
                "the bucket hid an entry the ranker matches, for {query:?}"
            );
        }
    }

    #[test]
    fn the_name_is_a_slice_of_the_path_rather_than_a_second_copy() {
        let full = r"C:\Sill\src-tauri\src\registry.rs";
        let one = catalog(&[(full, false)]);
        let slot = one.entries[0];

        assert_eq!(one.name(&slot), "registry.rs");
        assert_eq!(one.path(&slot), full);
        // The name sits inside the path rather than beside it.
        assert_eq!(one.held(), full.len(), "stored twice");
    }

    #[test]
    fn every_path_shares_one_allocation() {
        // The whole point of the arena. Forty-nine thousand separate
        // allocations measured at 25.8 MB of private memory to hold five
        // megabytes of text.
        let names = [r"C:\one\a.txt", r"C:\two\b.txt", r"C:\three\c.txt"];
        let many = catalog(&names.map(|name| (name, false)));

        let text: usize = names.iter().map(|name| name.len()).sum();

        assert_eq!(many.held(), text, "the arena holds exactly the paths");
        assert_eq!(many.entries.len(), 3);
        // Twenty-four bytes each, and nothing on the heap of their own. It was
        // sixteen, including the tombstone, which went into padding that was
        // already being paid for. `size:` and `date:` added a size and a write
        // time, which is the eight bytes: **386 KB on a 49,451 entry index**,
        // against 5.1 MB of path text and 1.1 MB of slots. Guarded here so that
        // a third field cannot be added to a slot without somebody deciding
        // whether fifty thousand copies of it are worth the answer it buys.
        assert_eq!(std::mem::size_of::<Slot>(), 24);
    }

    /// A patch shares the walked text instead of copying it.
    ///
    /// This is the whole reason `apply` is worth having: if the arena were
    /// copied, patching would cost what is indexed rather than what changed,
    /// and the item would have moved the work rather than removed it.
    #[test]
    fn a_patch_does_not_copy_the_walked_text() {
        let one = catalog(&[(r"C:\work\a.txt", false), (r"C:\work\b.txt", false)]);
        let two = one
            .apply(&[PathBuf::from(r"C:\work\c.txt")], &[])
            .expect("a file was added");

        assert!(
            Arc::ptr_eq(&one.base, &two.base),
            "the patched catalog holds a second copy of the walked text"
        );
    }

    #[test]
    fn an_added_file_can_be_found() {
        let one = catalog(&[(r"C:\work\a.txt", false)]);
        assert!(one.search("ledger", 10, &[]).is_empty());

        let two = one
            .apply(&[PathBuf::from(r"C:\work\ledger.txt")], &[])
            .expect("a file was added");

        let found = two.search("ledger", 10, &[]);
        assert_eq!(found.len(), 1, "the added file is not searchable");
        assert_eq!(found[0].path, r"C:\work\ledger.txt");
        assert_eq!(two.len(), 2);
    }

    #[test]
    fn a_removed_file_stops_being_an_answer() {
        let one = catalog(&[(r"C:\work\ledger.txt", false), (r"C:\work\b.txt", false)]);
        assert_eq!(one.search("ledger", 10, &[]).len(), 1);

        let two = one
            .apply(&[], &[PathBuf::from(r"C:\work\ledger.txt")])
            .expect("a file was removed");

        assert!(
            two.search("ledger", 10, &[]).is_empty(),
            "a deleted file is still being offered"
        );
        assert_eq!(two.len(), 1, "the count still includes the dead one");
    }

    /// Windows means the same file by either separator and either case.
    #[test]
    fn a_removal_matches_however_the_path_was_written() {
        for written in [
            r"C:\work\ledger.txt",
            "C:/work/ledger.txt",
            r"c:\WORK\Ledger.TXT",
        ] {
            let one = catalog(&[(r"C:\work\ledger.txt", false)]);
            let two = one
                .apply(&[], &[PathBuf::from(written)])
                .expect("a file was removed");

            assert!(
                two.search("ledger", 10, &[]).is_empty(),
                "{written} did not match the indexed path"
            );
        }
    }

    /// A watcher reports a write to an existing file as a create on some
    /// filesystems. Indexing it again would show it twice.
    #[test]
    fn adding_a_file_that_is_already_there_does_not_list_it_twice() {
        let one = catalog(&[(r"C:\work\ledger.txt", false)]);
        let two = one
            .apply(
                &[
                    PathBuf::from(r"C:\work\ledger.txt"),
                    PathBuf::from(r"C:\work\new.txt"),
                ],
                &[],
            )
            .expect("something was added");

        assert_eq!(two.search("ledger", 10, &[]).len(), 1, "listed twice");
        assert_eq!(two.len(), 2);
    }

    /// A file deleted and written again before things settled is still there.
    ///
    /// This is the case that pins the order of the two loops. Additions first
    /// would see the dying slot, call it a duplicate, skip it, and then remove
    /// it, leaving the index saying a file is gone while it sits on disk.
    #[test]
    fn a_file_removed_and_added_in_one_batch_is_still_indexed() {
        let one = catalog(&[(r"C:\work\ledger.txt", false)]);
        let two = one
            .apply(
                &[PathBuf::from(r"C:\work\ledger.txt")],
                &[PathBuf::from(r"C:\work\ledger.txt")],
            )
            .expect("a delete and a write");

        assert_eq!(
            two.search("ledger", 10, &[]).len(),
            1,
            "a file that was rewritten is missing from the index"
        );
        assert_eq!(two.len(), 1, "it is in there twice");
    }

    /// A rename arrives as a delete and a create together. The delete must not
    /// strike out the entry the create just made.
    #[test]
    fn a_rename_in_one_batch_keeps_the_new_name() {
        let one = catalog(&[(r"C:\work\before.txt", false)]);
        let two = one
            .apply(
                &[PathBuf::from(r"C:\work\after.txt")],
                &[PathBuf::from(r"C:\work\before.txt")],
            )
            .expect("a rename");

        assert!(
            two.search("before", 10, &[]).is_empty(),
            "the old name stayed"
        );
        assert_eq!(
            two.search("after", 10, &[]).len(),
            1,
            "the new name is gone"
        );
        assert_eq!(two.len(), 1);
    }

    #[test]
    fn nothing_to_do_is_not_a_new_catalog() {
        let one = catalog(&[(r"C:\work\a.txt", false)]);
        assert!(one.apply(&[], &[]).is_none());
    }

    /// Past a third dead, a walk should reclaim the slots rather than another
    /// patch adding to them.
    #[test]
    fn enough_dead_slots_ask_for_a_walk_instead() {
        let names: Vec<String> = (0..100).map(|n| format!(r"C:\work\f{n}.txt")).collect();
        let one = catalog(
            &names
                .iter()
                .map(|name| (name.as_str(), false))
                .collect::<Vec<_>>(),
        );

        let gone: Vec<PathBuf> = names[..40].iter().map(PathBuf::from).collect();
        let two = one.apply(&[], &gone).expect("a first patch is fine");

        assert!(
            two.apply(&[], &[PathBuf::from(r"C:\work\f50.txt")])
                .is_none(),
            "a catalog that is 40% dead should be walked, not patched again"
        );
    }

    /// Saving is compaction, so what comes back is clean.
    #[test]
    fn saving_drops_the_dead_and_what_they_held() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("index.bin");

        let one = catalog(&[
            (r"C:\work\keep.txt", false),
            (r"C:\work\a-very-long-name-that-goes-away.txt", false),
        ]);
        let two = one
            .apply(
                &[],
                &[PathBuf::from(
                    r"C:\work\a-very-long-name-that-goes-away.txt",
                )],
            )
            .expect("a file was removed");

        two.save(&file).expect("saves");
        let back = Catalog::load(&file, &[]).expect("reads back");

        assert_eq!(back.len(), 1);
        assert_eq!(back.dead, 0, "a dead slot was written out");
        assert_eq!(
            back.held(),
            r"C:\work\keep.txt".len(),
            "the arena still holds the text of a file that is gone"
        );
        assert_eq!(back.search("keep", 10, &[]).len(), 1);
        assert!(back.search("goes-away", 10, &[]).is_empty());
    }

    #[test]
    fn searching_finds_a_file_by_its_name() {
        let found = catalog(&[
            (r"C:\Sill\src-tauri\src\registry.rs", false),
            (r"C:\Sill\src-tauri\src\emoji.rs", false),
        ])
        .search("registry", 10, &[]);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "registry.rs");
        assert_eq!(found[0].path, r"C:\Sill\src-tauri\src\registry.rs");
    }

    #[test]
    fn the_shorter_name_wins_when_both_match_as_well() {
        // Same reason the rest of the launcher does it: the query covers more
        // of the shorter one, so it is the likelier answer.
        let found = catalog(&[
            (r"C:\p\registry-of-everything-ever.rs", false),
            (r"C:\p\registry.rs", false),
        ])
        .search("registry", 10, &[]);

        assert_eq!(found[0].name, "registry.rs");
    }

    #[test]
    fn the_bucket_never_hides_something_that_would_have_matched() {
        // The whole risk of narrowing before ranking. Every name here really
        // does match "re", and skipping the bucket must not change that.
        let all = catalog(&[
            (r"C:\p\registry.rs", false),
            (r"C:\p\read-me.md", false),
            (r"C:\p\my-report.txt", false),
            (r"C:\p\CoreRenderer.cs", false),
            (r"C:\p\unrelated.bin", false),
        ]);

        let found: Vec<String> = all
            .search("re", 10, &[])
            .into_iter()
            .map(|f| f.name)
            .collect();

        assert_eq!(found.len(), 4, "{found:?}");
        assert!(found.iter().all(|name| name != "unrelated.bin"));
    }

    #[test]
    fn a_directory_says_that_it_is_one() {
        let found = catalog(&[(r"C:\Sill\src-tauri", true)]).search("src-tauri", 10, &[]);

        assert!(found[0].is_dir);
    }

    #[test]
    fn an_empty_query_finds_nothing_rather_than_everything() {
        // The root list is not a file listing, and forty thousand rows is not
        // an answer to a question nobody asked.
        assert!(catalog(&[(r"C:\p\a.txt", false)])
            .search("", 10, &[])
            .is_empty());
        assert!(catalog(&[(r"C:\p\a.txt", false)])
            .search("   ", 10, &[])
            .is_empty());
    }

    #[test]
    fn nothing_is_returned_past_the_limit() {
        let files: Vec<(&str, bool)> = vec![
            (r"C:\p\test-one.txt", false),
            (r"C:\p\test-two.txt", false),
            (r"C:\p\test-three.txt", false),
        ];

        assert_eq!(catalog(&files).search("test", 2, &[]).len(), 2);
    }

    #[test]
    fn noise_covers_the_directories_that_are_noise_everywhere() {
        // Pinned because the measurement that justifies this whole module
        // depends on them: with these skipped the home folder is 42,976 files
        // rather than 2,272,143.
        for wanted in ["node_modules", "target", ".git", "AppData", ".cargo"] {
            assert!(NOISE.contains(&wanted), "{wanted} is not skipped");
        }
    }

    /// Regression: a tool that appends to a file every few seconds inside the
    /// home folder rebuilt the whole index every 45 seconds, because nothing
    /// below `AppData` was the only churn the list knew about.
    #[test]
    fn a_tool_that_writes_constantly_does_not_rebuild_the_index() {
        let home = PathBuf::from(r"C:\Users\Someone");
        let roots = vec![home.clone()];

        assert!(
            !worth_indexing(&home.join(".claude/projects/session.jsonl"), &roots),
            "an assistant transcript rebuilds the index on every append"
        );
    }

    #[test]
    fn walking_stays_off_most_of_the_machine() {
        // It runs while somebody is using their computer for something else.
        let used = threads();
        let have = std::thread::available_parallelism().map_or(1, |n| n.get());

        assert!(used >= 2 && used <= 6, "{used}");
        assert!(used <= have.max(2), "asked for more threads than there are");
    }

    #[test]
    fn a_saved_index_reads_back_the_same() {
        let dir = std::env::temp_dir().join("sill-catalog-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("index.bin");

        let mut original = catalog(&[
            (r"C:\work\notes.md", false),
            (r"C:\work\src", true),
            (r"C:\work\src\main.rs", false),
        ]);
        original.roots = vec![PathBuf::from(r"C:\work")];

        original.save(&file).unwrap();
        let back = Catalog::load(&file, &[PathBuf::from(r"C:\work")]).expect("reads back");

        assert_eq!(back.len(), original.len());

        // Compared path by path rather than arena to arena. The arena is a
        // layout detail and a save is also a compaction, so the bytes are
        // allowed to differ; what must not differ is what is in it.
        let said = |one: &Catalog| -> Vec<String> {
            one.entries
                .iter()
                .filter(|slot| slot.live)
                .map(|slot| one.path(slot).to_string())
                .collect()
        };
        assert_eq!(said(&back), said(&original));

        let found = back.search("main", 10, &[]);
        assert_eq!(found[0].path, r"C:\work\src\main.rs");

        let dirs = back.search("src", 10, &[]);
        assert!(dirs[0].is_dir, "lost which entries are folders");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_index_saved_for_other_folders_is_not_used() {
        // Not stale: about a different question. Loading it would silently
        // search folders somebody has stopped asking about and miss the ones
        // they just added.
        let dir = std::env::temp_dir().join("sill-catalog-roots");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("index.bin");

        let mut saved = catalog(&[(r"C:\one\a.md", false)]);
        saved.roots = vec![PathBuf::from(r"C:\one")];
        saved.save(&file).unwrap();

        assert!(Catalog::load(&file, &[PathBuf::from(r"C:\two")]).is_none());
        assert!(Catalog::load(&file, &[]).is_none());
        assert!(Catalog::load(&file, &[PathBuf::from(r"C:\one")]).is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nonsense_on_disk_is_ignored_rather_than_trusted() {
        // The file can be anything: truncated by a power cut, half-written by
        // an older version, or edited. Every offset in it indexes a string, so
        // a wrong one is not a wrong answer, it is a crash.
        let dir = std::env::temp_dir().join("sill-catalog-junk");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("index.bin");
        let roots = vec![PathBuf::from(r"C:\work")];

        for bad in [
            b"".to_vec(),
            b"not an index at all".to_vec(),
            MAGIC.to_vec(),
        ] {
            std::fs::write(&file, &bad).unwrap();
            assert!(Catalog::load(&file, &roots).is_none(), "{bad:?}");
        }

        // A real one, truncated part way through.
        let mut real = catalog(&[(r"C:\work\a.md", false)]);
        real.roots = roots.clone();
        real.save(&file).unwrap();
        let whole = std::fs::read(&file).unwrap();
        for cut in [whole.len() / 3, whole.len() / 2, whole.len() - 1] {
            std::fs::write(&file, &whole[..cut]).unwrap();
            assert!(
                Catalog::load(&file, &roots).is_none(),
                "survived a cut at {cut}"
            );
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_file_is_not_a_problem() {
        // The ordinary first run.
        let nowhere = std::env::temp_dir().join("sill-no-such-index.bin");
        std::fs::remove_file(&nowhere).ok();

        assert!(Catalog::load(&nowhere, &[PathBuf::from(r"C:\work")]).is_none());
    }

    #[test]
    fn a_drive_is_recognised_however_the_root_was_written() {
        // Somebody may type any of these into a settings field, and all three
        // mean the same disk. Reading them as different roots would show a
        // drive as unindexed while it is being indexed, and adding it again
        // would put it in the list twice.
        for written in [r"C:\", "C:/", "C:", r"c:\", "C:\\\\"] {
            assert!(
                same_root(Path::new(written), r"C:\"),
                "{written} was not read as C:"
            );
        }

        assert!(!same_root(Path::new(r"D:\"), r"C:\"));
        assert!(!same_root(Path::new(r"C:\Users"), r"C:\"));
    }

    #[test]
    fn windows_itself_is_skipped_where_a_drive_begins() {
        // Measured: a whole drive is 127,733 files with these left out. With
        // Windows and the two Program Files folders in, it is over a million,
        // and none of them is a file anybody searches for by name.
        for wanted in ["Windows", "Program Files", "ProgramData", "$Recycle.Bin"] {
            assert!(SYSTEM.contains(&wanted), "{wanted} is indexed");
        }
    }

    #[test]
    fn a_junction_that_loops_back_is_skipped() {
        // `Documents and Settings` points into `Users`. Walking it indexes a
        // whole home folder a second time under a second name.
        assert!(SYSTEM.contains(&"Documents and Settings"));
    }

    #[test]
    fn the_two_skip_lists_do_not_overlap() {
        // They mean different things: one is skipped wherever it appears, the
        // other only where a drive begins. A name in both is a name whose
        // second listing does nothing, which reads as if it did.
        //
        // Compared without case, because Windows paths are matched without
        // case and the same folder was once listed as `$RECYCLE.BIN` in one
        // and `$Recycle.Bin` in the other. Neither spelling matched the folder
        // on disk, so one of the two entries had never done anything.
        for name in SYSTEM {
            let clash = NOISE.iter().any(|other| other.eq_ignore_ascii_case(name));

            assert!(!clash, "{name} is in both lists");
        }
    }

    #[test]
    fn a_skipped_name_is_matched_the_way_windows_matches_names() {
        // Windows does not care about case and neither does anybody typing a
        // folder name. Two lists that disagree about capitalisation are two
        // lists where one of them silently does nothing.
        for name in NOISE.iter().chain(SYSTEM) {
            assert_eq!(
                name.trim(),
                *name,
                "{name:?} has whitespace, which no directory does"
            );
            assert!(!name.is_empty());
        }
    }

    // ------------------------------------------------ narrowing to folders

    fn narrowed(only_in: &[&str]) -> Vec<String> {
        let folders: Vec<String> = only_in.iter().map(|f| f.to_string()).collect();

        catalog(&[
            (r"C:\work\notes.md", false),
            (r"C:\workshop\notes.md", false),
            (r"C:\play\notes.md", false),
            (r"C:\work\deep\down\notes.md", false),
        ])
        .search("notes", 10, &folders)
        .into_iter()
        .map(|hit| hit.path)
        .collect()
    }

    #[test]
    fn narrowing_to_nothing_narrows_nothing() {
        assert_eq!(narrowed(&[]).len(), 4);
    }

    #[test]
    fn narrowing_keeps_only_what_is_inside() {
        let found = narrowed(&[r"C:\work"]);

        // Membership, not order: both are named `notes.md`, so they tie on
        // every ranking key and the path decides, alphabetically.
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|path| path.starts_with(r"C:\work\")));
    }

    #[test]
    fn a_folder_does_not_match_one_whose_name_merely_starts_the_same() {
        // The trap in doing this as a prefix test. `C:\work` must not take in
        // `C:\workshop`, which is a different folder that happens to begin
        // with the same letters.
        let found = narrowed(&[r"C:\work"]);

        assert!(
            !found.iter().any(|path| path.contains("workshop")),
            "workshop leaked in: {found:?}"
        );
    }

    #[test]
    fn narrowing_reads_a_folder_however_it_was_typed() {
        // Settings take whatever somebody types. All of these mean `C:\work`.
        for written in [
            r"C:\work",
            "C:/work",
            r"C:\work\",
            r"c:\WORK",
            "  C:/work  ",
        ] {
            let found = narrowed(&[written]);

            assert_eq!(found.len(), 2, "{written:?} gave {found:?}");
        }
    }

    #[test]
    fn narrowing_to_several_folders_keeps_all_of_them() {
        let found = narrowed(&[r"C:\play", r"C:\workshop"]);

        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|path| path.contains("play")));
        assert!(found.iter().any(|path| path.contains("workshop")));
    }

    #[test]
    fn a_blank_folder_in_the_list_is_ignored_rather_than_matching_everything() {
        // An empty string is a prefix of every path. Left in, one stray blank
        // row in the settings would quietly undo the whole setting.
        let found = narrowed(&["", "   ", r"C:\play"]);

        assert_eq!(found, vec![r"C:\play\notes.md"], "{found:?}");
    }

    // ------------------------------------------------ asking for more

    /// What an operator costs a query that does not use one.
    ///
    /// This is the whole guard. `ext:` is three characters somebody types on
    /// the way to something else, and the parser runs on every keystroke
    /// whether or not one is there. A query with no colon in it is handed
    /// **back**, not copied: the `Cow` is borrowed and points at the very bytes
    /// that came in, so the ordinary keystroke allocates nothing here and
    /// splits nothing.
    #[test]
    fn a_query_with_no_operator_in_it_is_not_even_copied() {
        for typed in [
            "budget",
            "quarterly budget report",
            "",
            "   spaced out   ",
            "*.json",
        ] {
            let (asked, filters) = operators_at(typed, 1_700_000_000);

            assert!(
                matches!(asked, std::borrow::Cow::Borrowed(_)),
                "{typed:?} was copied when nothing was taken out of it"
            );
            assert!(
                std::ptr::eq(asked.as_ptr(), typed.as_ptr()),
                "{typed:?} came back as different bytes"
            );
            assert!(filters.asked_for_nothing());
        }
    }

    /// And a colon on its own is not an operator either.
    ///
    /// `C:\work` is a thing people type into a launcher. It reaches the slow
    /// half of the parser, because the only cheap way to rule a query out is
    /// the colon, but it still comes back borrowed and narrowing nothing.
    #[test]
    fn a_drive_letter_is_not_an_operator() {
        for typed in [r"C:\work", "notes: a list", "colour:red", "12:30"] {
            let (asked, filters) = operators_at(typed, 1_700_000_000);

            assert!(
                std::ptr::eq(asked.as_ptr(), typed.as_ptr()),
                "{typed:?} was rewritten"
            );
            assert!(filters.asked_for_nothing(), "{typed:?} narrowed something");
        }
    }

    /// The parse is not allowed to become the expensive part of typing.
    ///
    /// A generous ceiling rather than a tight one: this runs in a debug build
    /// on whatever machine happens to be free. It is here to catch the ordinary
    /// path growing a `to_lowercase` or a `split`, which would be orders away
    /// from this, not to measure the machine.
    #[test]
    fn parsing_an_ordinary_query_is_not_where_a_keystroke_goes() {
        const ROUNDS: usize = 100_000;

        let began = std::time::Instant::now();
        for _ in 0..ROUNDS {
            let asked = operators_at(std::hint::black_box("quarterly budget report"), 0);
            std::hint::black_box(asked);
        }
        let took = began.elapsed();

        assert!(
            took < std::time::Duration::from_millis(250),
            "{ROUNDS} parses took {took:?}, which is work a keystroke should not be doing"
        );
    }

    #[test]
    fn an_extension_is_taken_out_of_the_query() {
        let (asked, filters) = operators_at("budget ext:pdf", 1_700_000_000);

        assert_eq!(asked, "budget");
        assert_eq!(filters.ext, vec!["pdf".to_string()]);
        assert!(filters.size.is_none());
        assert!(filters.modified.is_none());
    }

    #[test]
    fn an_operator_may_come_first_or_last_or_in_the_middle() {
        for typed in [
            "ext:pdf budget report",
            "budget ext:pdf report",
            "budget report ext:pdf",
        ] {
            let (asked, filters) = operators_at(typed, 1_700_000_000);
            assert_eq!(asked, "budget report", "{typed:?}");
            assert_eq!(filters.ext, vec!["pdf".to_string()], "{typed:?}");
        }
    }

    #[test]
    fn an_extension_may_be_written_with_a_dot_or_as_a_list() {
        for typed in ["ext:.rs", "ext:RS"] {
            let (_, filters) = operators_at(&format!("code {typed}"), 0);
            assert_eq!(filters.ext, vec!["rs".to_string()], "{typed:?}");
        }

        let (_, filters) = operators_at("photo ext:png,jpg,.JPEG", 0);
        assert_eq!(
            filters.ext,
            vec!["png".to_string(), "jpg".to_string(), "jpeg".to_string()]
        );
    }

    #[test]
    fn a_size_reads_its_comparator_and_its_unit() {
        // Binary units, because that is what a file's properties dialog shows.
        assert_eq!(read_bytes("1mb"), Some(1024 * 1024));
        assert_eq!(read_bytes("1MiB"), Some(1024 * 1024));
        assert_eq!(read_bytes("512"), Some(512));
        assert_eq!(read_bytes("2 gb"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(read_bytes("big"), None);
        assert_eq!(read_bytes("4pb"), None);

        // Held in kibibytes, so a megabyte is 1024 of them.
        assert_eq!(
            read_size(">1mb"),
            Some(Between {
                low: 1025,
                high: u32::MAX
            })
        );
        assert_eq!(
            read_size(">=1mb"),
            Some(Between {
                low: 1024,
                high: u32::MAX
            })
        );
        assert_eq!(read_size("<1mb"), Some(Between { low: 0, high: 1023 }));
        assert_eq!(read_size("<=1mb"), Some(Between { low: 0, high: 1024 }));
        assert_eq!(
            read_size("=1mb"),
            Some(Between {
                low: 1024,
                high: 1024
            })
        );

        // Nothing said means at least, which is what somebody typing
        // `size:100mb` looking for the big files means.
        assert_eq!(read_size("1mb"), read_size(">=1mb"));
    }

    /// A half-typed operator is text, not an operator that matches everything.
    ///
    /// `size:>1mb` is typed one character at a time and passes through `size:`
    /// and `size:>` on the way. If those parsed as "any size" the list would
    /// flash the whole index for two keystrokes.
    #[test]
    fn a_half_typed_operator_stays_in_the_query() {
        for typed in [
            "size:", "size:>", "size:>x", "ext:", "ext:.", "date:", "date:3",
        ] {
            let (asked, filters) = operators_at(typed, 1_700_000_000);

            assert_eq!(asked, typed, "{typed:?} was taken out of the query");
            assert!(
                filters.asked_for_nothing(),
                "{typed:?} narrowed something before it was finished"
            );
        }
    }

    #[test]
    fn a_date_window_rolls_back_from_now() {
        let now = 1_700_000_000u32;
        let day = 24 * 60 * 60;

        let (asked, filters) = operators_at("notes date:week", now);
        assert_eq!(asked, "notes");
        assert_eq!(
            filters.modified,
            Some(Between {
                low: now - 7 * day,
                high: u32::MAX
            })
        );

        assert_eq!(read_date("today", now), read_date("1d", now));
        assert_eq!(read_date("month", now), read_date("30d", now));
        assert_eq!(read_date("year", now), read_date("365d", now));
        assert_eq!(
            read_date("6h", now),
            Some(Between {
                low: now - 6 * 60 * 60,
                high: u32::MAX
            })
        );

        // Not "up to now". A file copied off a machine an hour ahead has a
        // write time in the future, and it is the newest thing there is.
        assert_eq!(
            read_date("today", now).map(|window| window.high),
            Some(u32::MAX)
        );
    }

    #[test]
    fn two_of_the_same_operator_narrow_rather_than_replace() {
        // The only way to write a band, since one term carries one comparison.
        let (asked, filters) = operators_at("log size:>1mb size:<4mb", 0);

        assert_eq!(asked, "log");
        assert_eq!(
            filters.size,
            Some(Between {
                low: 1025,
                high: 4095
            })
        );
    }

    #[test]
    fn a_dot_at_the_start_of_a_name_is_not_an_extension() {
        // `.gitignore` is a name that begins with a dot, not a file of type
        // "gitignore". Matching it would make `ext:` answer things nobody
        // asked about.
        assert!(!has_extension(".gitignore", "gitignore"));
        assert!(has_extension("notes.md", "md"));
        assert!(has_extension("NOTES.MD", "md"));
        assert!(has_extension("archive.tar.gz", "gz"));
        assert!(!has_extension("archive.tar.gz", "tar"));
        assert!(!has_extension("Makefile", "makefile"));
    }

    // ------------------------------------------- operators against an index

    /// A small index on disk, with sizes and write times chosen by the test.
    fn a_few_files(dir: &Path, now: u32) {
        let day = 24u32 * 60 * 60;

        for (name, bytes, ago) in [
            ("report.pdf", 3 * 1024 * 1024usize, 2 * day),
            ("report.txt", 12usize, 400 * day),
            ("holiday.png", 700 * 1024usize, 2 * day),
            ("archive.zip", 40 * 1024 * 1024usize, 400 * day),
        ] {
            let at = dir.join(name);
            std::fs::write(&at, vec![b'x'; bytes]).expect("write");

            let file = std::fs::File::options()
                .write(true)
                .open(&at)
                .expect("open");

            file.set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(u64::from(now - ago)),
            )
            .expect("set the write time");
        }
    }

    fn names(hits: &[FileHit]) -> Vec<String> {
        let mut found: Vec<String> = hits.iter().map(|hit| hit.name.clone()).collect();
        found.sort();
        found
    }

    #[test]
    fn an_extension_narrows_a_search_to_one_kind_of_file() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        a_few_files(dir.path(), now);

        let catalog = Catalog::build(&[dir.path().to_path_buf()]);

        assert_eq!(
            names(&catalog.search_at("report", 20, &[], now)),
            vec!["report.pdf", "report.txt"],
            "without an operator both should be there"
        );

        assert_eq!(
            names(&catalog.search_at("report ext:pdf", 20, &[], now)),
            vec!["report.pdf"]
        );
    }

    #[test]
    fn a_size_narrows_to_the_big_ones() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        a_few_files(dir.path(), now);

        let catalog = Catalog::build(&[dir.path().to_path_buf()]);

        assert_eq!(
            names(&catalog.search_at("report size:>1mb", 20, &[], now)),
            vec!["report.pdf"],
            "the twelve byte one is not over a megabyte"
        );

        assert_eq!(
            names(&catalog.search_at("report size:<1mb", 20, &[], now)),
            vec!["report.txt"]
        );
    }

    #[test]
    fn a_date_narrows_to_what_changed_recently() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        a_few_files(dir.path(), now);

        let catalog = Catalog::build(&[dir.path().to_path_buf()]);

        assert_eq!(
            names(&catalog.search_at("report date:week", 20, &[], now)),
            vec!["report.pdf"],
            "the other one was written over a year ago"
        );

        assert_eq!(
            names(&catalog.search_at("report date:2y", 20, &[], now)),
            vec!["report.pdf", "report.txt"]
        );
    }

    /// `ext:png` on its own means every PNG, and is still a question.
    #[test]
    fn a_query_of_nothing_but_operators_is_still_a_question() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        a_few_files(dir.path(), now);

        let catalog = Catalog::build(&[dir.path().to_path_buf()]);

        assert_eq!(
            names(&catalog.search_at("ext:png", 20, &[], now)),
            vec!["holiday.png"]
        );

        assert_eq!(
            names(&catalog.search_at("size:>10mb", 20, &[], now)),
            vec!["archive.zip"]
        );

        // And nothing at all is still nothing.
        assert!(catalog.search_at("", 20, &[], now).is_empty());
        assert!(catalog.search_at("   ", 20, &[], now).is_empty());
    }

    /// Operators and the folder narrowing are both applied, not either.
    #[test]
    fn an_operator_does_not_replace_the_folder_setting() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        let inside = dir.path().join("inside");
        std::fs::create_dir(&inside).expect("a folder");
        a_few_files(&inside, now);
        a_few_files(dir.path(), now);

        let catalog = Catalog::build(&[dir.path().to_path_buf()]);

        let only_in = vec![inside.to_string_lossy().into_owned()];
        let found = catalog.search_at("report ext:pdf", 20, &only_in, now);

        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            settled(&found[0].path).starts_with(&settled(&inside.to_string_lossy())),
            "{found:?} came from outside the folder that was asked for"
        );
    }

    /// The size and the write time have to survive the saved index.
    ///
    /// They are the two fields the file format grew for this, and an index read
    /// back without them would answer `size:` and `date:` with nothing on every
    /// start but the first.
    #[test]
    fn size_and_write_time_come_back_from_the_saved_index() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        a_few_files(dir.path(), now);

        let file = dir.path().join("files.bin");
        let built = Catalog::build(&[dir.path().to_path_buf()]);
        built.save(&file).expect("save");

        let read = Catalog::load(&file, built.roots()).expect("the index reads back");

        assert_eq!(
            names(&read.search_at("report size:>1mb", 20, &[], now)),
            names(&built.search_at("report size:>1mb", 20, &[], now))
        );
        assert_eq!(
            names(&read.search_at("report date:week", 20, &[], now)),
            vec!["report.pdf"]
        );
    }

    /// And a file that arrives by a patch has them too.
    ///
    /// A patch does not walk, so it asks the disk itself. Missing this leaves
    /// anything created since the last walk invisible to both operators until
    /// the next one.
    #[test]
    fn a_patched_in_file_can_be_asked_about_by_size() {
        let now = 1_700_000_000u32;
        let dir = tempfile::tempdir().expect("temp dir");
        let built = Catalog::build(&[dir.path().to_path_buf()]);

        let late = dir.path().join("latecomer.pdf");
        std::fs::write(&late, vec![b'x'; 3 * 1024 * 1024]).expect("write");

        let patched = built.apply(&[late], &[]).expect("a patch");

        assert_eq!(
            names(&patched.search_at("latecomer size:>1mb", 20, &[], now)),
            vec!["latecomer.pdf"],
            "a file added without a walk has no size"
        );
    }
}
