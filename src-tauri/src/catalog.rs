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
    ".venv",
    "venv",
    "vendor",
    ".pnpm-store",
    ".cache",
    "$RECYCLE.BIN",
    "System Volume Information",
];

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
                    !entry
                        .file_name()
                        .to_str()
                        .is_some_and(|name| NOISE.contains(&name))
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
    pub fn search(&self, query: &str, limit: usize) -> Vec<FileHit> {
        let query = query.trim();
        if query.is_empty() || self.entries.is_empty() {
            return Vec::new();
        }

        let candidates = self.candidates(query);
        let needle: Vec<char> = query.to_lowercase().chars().collect();

        let mut scored: Vec<(crate::registry::MatchClass, usize, u32)> = Vec::new();

        for &at in candidates.iter() {
            let name = self.name(&self.entries[at as usize]);

            if let Some((class, _)) = crate::registry::match_name(&needle, name) {
                scored.push((class, name.chars().count(), at));
            }
        }

        // The same order the rest of the launcher uses: the kind of match
        // first, then the shorter name, then something stable so two equally
        // good answers do not swap places between keystrokes.
        scored.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        .search("registry", 10);

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
        .search("registry", 10);

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

        let found: Vec<String> = all.search("re", 10).into_iter().map(|f| f.name).collect();

        assert_eq!(found.len(), 4, "{found:?}");
        assert!(found.iter().all(|name| name != "unrelated.bin"));
    }

    #[test]
    fn a_directory_says_that_it_is_one() {
        let found = catalog(&[(r"C:\Sill\src-tauri", true)]).search("src-tauri", 10);

        assert!(found[0].is_dir);
    }

    #[test]
    fn an_empty_query_finds_nothing_rather_than_everything() {
        // The root list is not a file listing, and forty thousand rows is not
        // an answer to a question nobody asked.
        assert!(catalog(&[(r"C:\p\a.txt", false)]).search("", 10).is_empty());
        assert!(catalog(&[(r"C:\p\a.txt", false)]).search("   ", 10).is_empty());
    }

    #[test]
    fn nothing_is_returned_past_the_limit() {
        let files: Vec<(&str, bool)> = vec![
            (r"C:\p\test-one.txt", false),
            (r"C:\p\test-two.txt", false),
            (r"C:\p\test-three.txt", false),
        ];

        assert_eq!(catalog(&files).search("test", 2).len(), 2);
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

    #[test]
    fn walking_stays_off_most_of_the_machine() {
        // It runs while somebody is using their computer for something else.
        let used = threads();
        let have = std::thread::available_parallelism().map_or(1, |n| n.get());

        assert!(used >= 2 && used <= 6, "{used}");
        assert!(used <= have.max(2), "asked for more threads than there are");
    }
}
