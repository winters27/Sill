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

/// Where one file's path sits in the arena.
///
/// Sixteen bytes and no allocation of its own. The first version gave every
/// entry its own `Box<str>`, which measured at **25.8 MB of private memory**
/// for a home folder against 11.3 MB before the index existed: forty-nine
/// thousand separate allocations, each with a header, for five megabytes of
/// actual text.
///
/// The name is not stored at all. A file name is always the tail of its own
/// path, so it is an offset rather than a second copy of the same bytes.
#[derive(Debug, Clone, Copy)]
struct Slot {
    /// Where the path starts in the arena.
    at: u32,
    /// Where it ends.
    end: u32,
    /// Where the name starts, which is somewhere between the two.
    name_at: u32,
    is_dir: bool,
}

/// Everything Sill knows about the files under its roots.
///
/// Immutable once built. Rebuilding produces a new one and swaps it in, so a
/// search in progress never sees a half-built index and never waits for one.
#[derive(Debug, Default)]
pub struct Catalog {
    /// Every path, end to end, in one allocation.
    ///
    /// See [`Slot`] for why. Paths are pure text and never change once walked,
    /// so there is nothing to gain from being able to free one of them.
    paths: String,
    entries: Vec<Slot>,
    /// Letters that begin a word, to the entries whose names contain them.
    ///
    /// See the module note: this is what makes ranking cheap enough to do
    /// while somebody is typing.
    buckets: HashMap<char, Vec<u32>>,
    roots: Vec<PathBuf>,
}

impl Catalog {
    /// How many files are in it.
    pub fn len(&self) -> usize {
        self.entries.len()
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
            if !root.is_dir() {
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

                if let Some(slot) = push(&mut paths, found.path(), is_dir) {
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
            paths,
            entries,
            buckets,
            roots: roots.to_vec(),
        }
    }

    fn path(&self, slot: &Slot) -> &str {
        &self.paths[slot.at as usize..slot.end as usize]
    }

    fn name(&self, slot: &Slot) -> &str {
        &self.paths[slot.name_at as usize..slot.end as usize]
    }

    /// The files a query matches, best first.
    ///
    /// Ranked by the same code that ranks everything else, so a file behaves
    /// like every other row rather than having a second idea of what a good
    /// match is.
    pub fn search(&self, query: &str, limit: usize, only_in: &[String]) -> Vec<FileHit> {
        let query = query.trim();
        if query.is_empty() || self.entries.is_empty() {
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

        let candidates = self.candidates(query);
        let needle: Vec<char> = query.to_lowercase().chars().collect();

        let mut scored: Vec<(crate::registry::MatchClass, usize, u32)> = Vec::new();

        for &at in candidates.iter() {
            let slot = self.entries[at as usize];

            if !inside.is_empty() && !under(self.path(&slot), &inside) {
                continue;
            }

            let name = self.name(&slot);

            if let Some(class) = matches(&needle, name) {
                scored.push((class, name.chars().count(), at));
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

/// Appends a walked path to the arena and returns where it landed.
///
/// Nothing if the path has no name or is not valid text. A path Windows cannot
/// render as UTF-8 is one nobody is going to type either.
fn push(paths: &mut String, path: &Path, is_dir: bool) -> Option<Slot> {
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
        is_dir,
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

// ------------------------------------------------------------ keeping it

/// What the saved index starts with, so an old or foreign file is not read.
///
/// The version changes whenever the layout below does. A cache written by a
/// different version is not migrated, it is ignored and rebuilt: it is a copy
/// of something that can be regenerated in seconds, and migration code for a
/// throwaway file is code that can be wrong.
const MAGIC: &[u8; 8] = b"SILLIDX1";

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
    pub fn save(&self, to: &Path) -> std::io::Result<()> {
        let mut out = Vec::with_capacity(self.paths.len() + self.entries.len() * 16 + 64);

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

        put_u32(&mut out, self.paths.len() as u32);
        out.extend_from_slice(self.paths.as_bytes());

        put_u32(&mut out, self.entries.len() as u32);
        for slot in &self.entries {
            put_u32(&mut out, slot.at);
            put_u32(&mut out, slot.end);
            put_u32(&mut out, slot.name_at);
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
                is_dir: take_u32(&raw, &mut at)? != 0,
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
            paths,
            entries,
            buckets,
            roots: roots.to_vec(),
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
            .filter_map(|(path, is_dir)| push(&mut paths, Path::new(path), *is_dir))
            .collect();

        Catalog {
            buckets: index(&paths, &entries),
            paths,
            entries,
            roots: Vec::new(),
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
        assert_eq!(one.paths.len(), full.len(), "stored twice");
    }

    #[test]
    fn every_path_shares_one_allocation() {
        // The whole point of the arena. Forty-nine thousand separate
        // allocations measured at 25.8 MB of private memory to hold five
        // megabytes of text.
        let names = [r"C:\one\a.txt", r"C:\two\b.txt", r"C:\three\c.txt"];
        let many = catalog(&names.map(|name| (name, false)));

        let text: usize = names.iter().map(|name| name.len()).sum();

        assert_eq!(many.paths.len(), text, "the arena holds exactly the paths");
        assert_eq!(many.entries.len(), 3);
        // Sixteen bytes each, and nothing on the heap of their own.
        assert_eq!(std::mem::size_of::<Slot>(), 16);
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
        assert_eq!(back.paths, original.paths);

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
}
