//! Browser history and bookmarks, as searchable results.
//!
//! Read on demand rather than indexed. History is the largest body of text on
//! most machines and the fastest changing: this one holds 37 MB of it across
//! three browsers, and it grows with every page. Folding that into the
//! launcher's index would multiply the index many times over to answer a
//! question only ever asked while somebody is typing, which rule 23 rules out.
//!
//! So nothing is read until a query arrives, and nothing is kept afterwards.
//!
//! ## The files belong to somebody else
//!
//! Every one of these is open in a running program that is still writing to
//! it. Chromium takes an exclusive lock on `History`, so opening it while the
//! browser runs fails outright. Firefox leaves `places.sqlite` readable but
//! writes through a journal, so a reader can see a torn view of it.
//!
//! Both are answered the same way: never open the original. A copy is taken
//! and the copy is read, which is what every tool doing this settles on. The
//! copy is reused until it is older than `STALE_AFTER`, because taking a 31 MB
//! copy on every keystroke would be worse than not having the feature at all.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;

/// How long a copy is trusted before another is taken.
///
/// History a few minutes behind is not wrong in any way somebody notices: a
/// page visited moments ago is still in the window in front of them. A copy
/// taken per keystroke would be.
const STALE_AFTER: Duration = Duration::from_secs(300);

/// How a browser stores what it knows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    /// `History` and `Bookmarks`, under a profile of a `User Data` directory.
    Chromium,
    /// `places.sqlite`, holding both.
    Firefox,
}

/// Which directory a browser keeps its profiles under.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Base {
    Local,
    Roaming,
}

/// A browser Sill knows how to read.
struct Known {
    /// What it is called, which is what the result says it came from.
    name: &'static str,
    family: Family,
    base: Base,
    /// Where its profiles live under that base.
    path: &'static str,
    /// The program it runs, for finding its icon when Windows does not have it
    /// registered as a browser.
    exe: &'static str,
}

/// The browsers Sill looks for.
///
/// Naming them is deliberate. Both families could be found by walking AppData
/// for anything holding a `places.sqlite`, and that would also find every
/// abandoned profile, every installer's leftovers, and every unrelated program
/// that happens to use the same file name. Saying what is being read is the
/// honest version, and adding one is a line.
const KNOWN: &[Known] = &[
    Known {
        name: "Chrome",
        family: Family::Chromium,
        base: Base::Local,
        path: r"Google\Chrome\User Data",
        exe: "chrome.exe",
    },
    Known {
        name: "Edge",
        family: Family::Chromium,
        base: Base::Local,
        path: r"Microsoft\Edge\User Data",
        exe: "msedge.exe",
    },
    Known {
        name: "Brave",
        family: Family::Chromium,
        base: Base::Local,
        path: r"BraveSoftware\Brave-Browser\User Data",
        exe: "brave.exe",
    },
    Known {
        name: "Vivaldi",
        family: Family::Chromium,
        base: Base::Local,
        path: r"Vivaldi\User Data",
        exe: "vivaldi.exe",
    },
    Known {
        name: "Chromium",
        family: Family::Chromium,
        base: Base::Local,
        path: r"Chromium\User Data",
        exe: "chrome.exe",
    },
    Known {
        name: "Opera",
        family: Family::Chromium,
        base: Base::Roaming,
        path: r"Opera Software\Opera Stable",
        exe: "opera.exe",
    },
    Known {
        name: "Firefox",
        family: Family::Firefox,
        base: Base::Roaming,
        path: r"Mozilla\Firefox\Profiles",
        exe: "firefox.exe",
    },
    Known {
        name: "Zen",
        family: Family::Firefox,
        base: Base::Roaming,
        path: r"zen\Profiles",
        exe: "zen.exe",
    },
    Known {
        name: "Librewolf",
        family: Family::Firefox,
        base: Base::Roaming,
        path: r"librewolf\Profiles",
        exe: "librewolf.exe",
    },
    Known {
        name: "Waterfox",
        family: Family::Firefox,
        base: Base::Roaming,
        path: r"Waterfox\Profiles",
        exe: "waterfox.exe",
    },
];

/// One profile of one browser, and the files it keeps.
#[derive(Debug, Clone)]
pub struct Profile {
    pub browser: String,
    pub family: Family,
    /// The profile's directory name, which is what somebody renamed.
    pub name: String,
    /// The program behind it, which is where its icon comes from.
    ///
    /// A result should wear the mark of the browser it came out of rather than
    /// Sill's, for the same reason a row that changes a Windows setting wears
    /// the icon of the program that owns the setting: the row is not Sill's
    /// doing, and dressing it as Sill's would say it was.
    pub program: Option<PathBuf>,
    /// Chromium's `History`, or Firefox's `places.sqlite`.
    pub history: Option<PathBuf>,
    /// Chromium's `Bookmarks`. Firefox keeps its bookmarks in the same file as
    /// its history, so for that family this is the same path.
    pub bookmarks: Option<PathBuf>,
}

/// A page somebody visited or saved.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub title: String,
    pub url: String,
    /// Which browser it came from, so two copies of a page are tellable apart.
    pub browser: String,
    /// Saved rather than merely visited.
    pub bookmark: bool,
    pub visits: i64,
    /// The program behind the browser it came from, for the row's icon.
    pub icon: Option<String>,
}

/// What to read.
#[derive(Debug, Clone, Copy)]
pub struct Want {
    pub history: bool,
    pub bookmarks: bool,
}

impl Default for Want {
    fn default() -> Self {
        Self {
            history: true,
            bookmarks: true,
        }
    }
}

/// The browser a running program is, if Sill knows it as one.
///
/// Named from the same table the profile reader uses, rather than a second
/// list of executables kept beside it. A browser added to `KNOWN` becomes
/// readable and switchable in the same line, and there is no way to add one
/// and get half of it.
///
/// The file name only, and case-insensitively, because this is compared
/// against whatever path a running process reports and that is neither
/// normalised nor consistently cased.
pub fn known_by_exe(exe: &str) -> Option<(&'static str, Family)> {
    let exe = exe.to_ascii_lowercase();

    KNOWN
        .iter()
        .find(|known| known.exe.eq_ignore_ascii_case(&exe))
        .map(|known| (known.name, known.family))
}

fn base_dir(base: Base) -> Option<PathBuf> {
    let var = match base {
        Base::Local => "LOCALAPPDATA",
        Base::Roaming => "APPDATA",
    };

    std::env::var_os(var).map(PathBuf::from)
}

/// Every profile of every known browser that is actually there.
///
/// A directory is not enough. Uninstalling leaves the tree behind, and this
/// machine carries a Zen profile directory under Local holding no
/// `places.sqlite` at all, beside the real one under Roaming. So the file
/// decides, not the folder.
pub fn profiles() -> Vec<Profile> {
    let mut out = Vec::new();

    for known in KNOWN {
        let Some(base) = base_dir(known.base) else {
            continue;
        };

        let root = base.join(known.path);
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };

        /*
         * A browser that is not installed is not offered.
         *
         * Uninstalling leaves the profile behind, so this machine has a Chrome
         * directory holding real history and no chrome.exe anywhere: not on
         * disk, not registered as a browser, not in App Paths. That history is
         * finished, nothing is being added to it, and a row from it would be
         * the one row in the list with no mark on it saying where it came from.
         *
         * Looked up once per browser rather than once per profile, because it
         * reads the registry and a browser can have a dozen profiles.
         */
        let Some(program) = browser_exe(known.name) else {
            continue;
        };
        let program = Some(program);

        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }

            let profile = match known.family {
                Family::Chromium => Profile {
                    browser: known.name.to_string(),
                    family: known.family,
                    name: file_name(&dir),
                    program: program.clone(),
                    history: existing(dir.join("History")),
                    bookmarks: existing(dir.join("Bookmarks")),
                },
                Family::Firefox => {
                    let places = existing(dir.join("places.sqlite"));
                    Profile {
                        browser: known.name.to_string(),
                        family: known.family,
                        name: file_name(&dir),
                        program: program.clone(),
                        bookmarks: places.clone(),
                        history: places,
                    }
                }
            };

            if profile.history.is_some() || profile.bookmarks.is_some() {
                out.push(profile);
            }
        }
    }

    out
}

fn existing(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A readable copy of a file another program has open.
///
/// Hands back an existing copy younger than `stale_after` untouched, which is
/// what stops a 31 MB file being copied on every keystroke.
///
/// Copying can fail while the browser is mid-write, and an older copy is a
/// better answer than none, so the previous one is only replaced once a new one
/// has been written in full.
/// How long a copy nobody is using is kept.
///
/// Long enough that a browser somebody opens once a week does not have its
/// copy thrown away between uses, short enough that a profile they deleted
/// stops taking up space and stops existing.
const KEEP_ORPHANS_FOR: Duration = Duration::from_secs(60 * 60 * 24);

/// Deletes copies that no live profile claims.
///
/// ## Why this exists
///
/// Nothing ever deleted them. A copy is made per profile per browser, and on
/// this machine one of them is 31 MB; they are replaced when they go stale and
/// otherwise kept for good. Uninstall a browser, or delete a profile, and its
/// copy stayed in Sill's own data directory until somebody went looking.
///
/// That is not only space. **These are copies of somebody's browsing
/// history.** Keeping one for a profile that no longer exists is holding on to
/// a record of where they went in a browser they have got rid of, which is not
/// a thing a launcher should be doing quietly.
///
/// The age is a parameter rather than read from the constant, so the rule can
/// be stated in a test without waiting a day or reaching for a crate that
/// rewrites timestamps.
///
/// ## Why an age as well as a claim
///
/// A profile is only "live" if its browser is installed and its directory is
/// where it was. A browser that is being upgraded, or a profile on a drive
/// that is not plugged in this minute, would look gone. The day of grace is
/// what stops a sweep during one of those moments from throwing away a copy
/// that will be wanted again in an hour.
pub fn sweep(into: &Path, claimed: &[PathBuf], keep_orphans_for: Duration) {
    let Ok(entries) = std::fs::read_dir(into) else {
        // No directory means nothing has been copied yet, which is the
        // cheapest possible state and nothing to tidy.
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if claimed.iter().any(|kept| kept == &path) {
            continue;
        }

        // The write-ahead log belongs to the database beside it, so it is kept
        // or dropped with it rather than judged on its own.
        if let Some(name) = path.to_str() {
            if name.ends_with("-wal")
                && claimed
                    .iter()
                    .any(|kept| name.starts_with(&kept.to_string_lossy().to_string()))
            {
                continue;
            }
        }

        let old_enough = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|when| when.elapsed().unwrap_or_default() > keep_orphans_for)
            .unwrap_or(false);

        if old_enough {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Where the copy of one file goes.
///
/// Its own function because two callers need the answer and they must agree:
/// one makes the copy, the other lists what to keep, and a name that differed
/// between them would sweep away every copy and make them all again.
///
/// Two browsers both call the file `History`, and every Firefox profile calls
/// it `places.sqlite`, so the name alone is not enough to tell copies apart.
/// The path they came from is.
fn copy_path(source: &Path, into: &Path) -> PathBuf {
    into.join(format!(
        "{}-{}",
        short_hash(&source.to_string_lossy()),
        file_name(source)
    ))
}

pub fn readable_copy(source: &Path, into: &Path, stale_after: Duration) -> Option<PathBuf> {
    // Two browsers both call the file `History`, and every Firefox profile
    // calls it `places.sqlite`, so the name alone is not enough to tell copies
    // apart. The path they came from is.
    let copy = copy_path(source, into);

    if fresh_enough(&copy, stale_after) {
        return Some(copy);
    }

    std::fs::create_dir_all(into).ok()?;

    let staging = copy.with_extension("part");
    match std::fs::copy(source, &staging) {
        Ok(_) => {
            std::fs::rename(&staging, &copy).ok()?;

            /*
             * The write-ahead log comes too, when there is one.
             *
             * Both families keep their databases in write-ahead mode, which
             * means the newest pages are in the log rather than in the file
             * beside it. Copying only the database gets a view that stops at
             * the last checkpoint, so the page somebody visited this morning
             * is missing and the feature looks broken in exactly the case it
             * is most wanted.
             *
             * It is copied second and on a best effort: a log without its
             * database is useless, a database without its log is merely a
             * little behind, and that is the safer of the two to end up with.
             */
            copy_sidecar(source, &copy, "-wal");

            Some(copy)
        }
        Err(_) => {
            let _ = std::fs::remove_file(&staging);
            copy.is_file().then_some(copy)
        }
    }
}

fn copy_sidecar(source: &Path, copy: &Path, suffix: &str) {
    let from = with_suffix(source, suffix);
    if !from.is_file() {
        // No log is the ordinary case for a browser that is not running.
        let _ = std::fs::remove_file(with_suffix(copy, suffix));
        return;
    }

    let _ = std::fs::copy(from, with_suffix(copy, suffix));
}

/// `places.sqlite` becomes `places.sqlite-wal`, which is a suffix on the whole
/// name rather than a change of extension.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn fresh_enough(copy: &Path, stale_after: Duration) -> bool {
    let Ok(modified) = std::fs::metadata(copy).and_then(|meta| meta.modified()) else {
        return false;
    };

    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < stale_after)
        .unwrap_or(false)
}

/// Enough of a path to tell two files apart, short enough to be a file name.
///
/// FNV-1a, so there is no dependency and nothing allocated to produce it. It
/// names a temporary file and guards nothing, so it does not need to be a
/// cryptographic hash and should not pretend to be one.
fn short_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;

    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    format!("{hash:016x}")
}

/// The most rows read out of one file for one query.
///
/// Wide enough that the right answer is in it, narrow enough that a query
/// against a 31 MB file stays cheap. What comes back is ordered by how often
/// the page was visited, so the cut falls on pages somebody has been to once.
const CANDIDATES: usize = 200;

/// Escapes what SQL's `LIKE` treats as a pattern.
///
/// Without this, typing `%` matches the entire history and typing `_` matches
/// any character, which reads as the search being broken rather than as
/// punctuation meaning something.
fn like(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 2);
    out.push('%');

    for ch in query.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }

    out.push('%');
    out
}

/// Opens a copy for reading.
///
/// The copy is Sill's own, so it is opened writable on purpose: a file left
/// with a write-ahead log needs to replay it before the newest rows are
/// visible, and SQLite cannot do that on a read-only connection. Nothing here
/// writes; the permission is only what lets the log be read.
fn open(path: &Path) -> Option<rusqlite::Connection> {
    let db = rusqlite::Connection::open(path).ok()?;
    // A locked or half-copied file should give up rather than hold the query.
    db.busy_timeout(Duration::from_millis(250)).ok()?;
    Some(db)
}

fn rows(
    db: &rusqlite::Connection,
    sql: &str,
    pattern: &str,
    browser: &str,
    icon: Option<&str>,
    bookmark: bool,
) -> Vec<Hit> {
    let Ok(mut statement) = db.prepare(sql) else {
        return Vec::new();
    };

    let found = statement.query_map(rusqlite::params![pattern, CANDIDATES as i64], |row| {
        Ok(Hit {
            url: row.get::<_, String>(0)?,
            // A page can be stored with no title, and its address is a better
            // label than an empty row.
            title: row
                .get::<_, Option<String>>(1)?
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| row.get::<_, String>(0).unwrap_or_default()),
            visits: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            browser: browser.to_string(),
            bookmark,
            icon: icon.map(str::to_string),
        })
    });

    match found {
        Ok(iter) => iter.flatten().collect(),
        Err(_) => Vec::new(),
    }
}

/// Chromium keeps its bookmarks as a JSON tree rather than in the database.
fn chromium_bookmarks(path: &Path, query: &str, browser: &str, icon: Option<&str>) -> Vec<Hit> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };

    let needle = query.to_lowercase();
    let mut out = Vec::new();

    // `roots` holds the bar, the other-bookmarks folder and the synced one,
    // each an ordinary node, so all three are walked the same way.
    if let Some(roots) = root.get("roots").and_then(|r| r.as_object()) {
        for node in roots.values() {
            walk_bookmarks(node, &needle, browser, icon, &mut out);
        }
    }

    out
}

fn walk_bookmarks(
    node: &serde_json::Value,
    needle: &str,
    browser: &str,
    icon: Option<&str>,
    out: &mut Vec<Hit>,
) {
    if out.len() >= CANDIDATES {
        return;
    }

    match node.get("type").and_then(|t| t.as_str()) {
        Some("url") => {
            let title = node
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            let url = node.get("url").and_then(|u| u.as_str()).unwrap_or_default();

            if url.is_empty() {
                return;
            }

            if title.to_lowercase().contains(needle) || url.to_lowercase().contains(needle) {
                out.push(Hit {
                    title: if title.is_empty() {
                        url.to_string()
                    } else {
                        title.to_string()
                    },
                    url: url.to_string(),
                    browser: browser.to_string(),
                    bookmark: true,
                    visits: 0,
                    icon: icon.map(str::to_string),
                });
            }
        }
        Some("folder") => {
            if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    walk_bookmarks(child, needle, browser, icon, out);
                }
            }
        }
        _ => {}
    }
}

/// Everything one profile knows that matches.
fn from_profile(profile: &Profile, query: &str, want: Want, scratch: &Path) -> Vec<Hit> {
    let pattern = like(query);
    let icon = profile
        .program
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());
    let icon = icon.as_deref();
    let mut out = Vec::new();

    match profile.family {
        Family::Chromium => {
            if want.history {
                if let Some(copy) = profile
                    .history
                    .as_ref()
                    .and_then(|p| readable_copy(p, scratch, STALE_AFTER))
                {
                    if let Some(db) = open(&copy) {
                        out.extend(rows(
                            &db,
                            "SELECT url, title, visit_count FROM urls \
                             WHERE (title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\') \
                             ORDER BY visit_count DESC LIMIT ?2",
                            &pattern,
                            &profile.browser,
                            icon,
                            false,
                        ));
                    }
                }
            }

            if want.bookmarks {
                if let Some(path) = profile.bookmarks.as_ref() {
                    // Small, rewritten whole, and not held open, so it is read
                    // where it lies rather than copied.
                    out.extend(chromium_bookmarks(path, query, &profile.browser, icon));
                }
            }
        }

        Family::Firefox => {
            let Some(copy) = profile
                .history
                .as_ref()
                .and_then(|p| readable_copy(p, scratch, STALE_AFTER))
            else {
                return out;
            };
            let Some(db) = open(&copy) else {
                return out;
            };

            if want.bookmarks {
                out.extend(rows(
                    &db,
                    "SELECT p.url, COALESCE(b.title, p.title), p.visit_count \
                     FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id \
                     WHERE b.type = 1 AND (b.title LIKE ?1 ESCAPE '\\' OR p.url LIKE ?1 ESCAPE '\\') \
                     ORDER BY p.visit_count DESC LIMIT ?2",
                    &pattern,
                    &profile.browser,
                    icon,
                    true,
                ));
            }

            if want.history {
                // `hidden` marks redirects and framed pages, which are visits
                // nobody made on purpose and would not recognise in a list.
                out.extend(rows(
                    &db,
                    "SELECT url, title, visit_count FROM moz_places \
                     WHERE hidden = 0 AND (title LIKE ?1 ESCAPE '\\' OR url LIKE ?1 ESCAPE '\\') \
                     ORDER BY visit_count DESC LIMIT ?2",
                    &pattern,
                    &profile.browser,
                    icon,
                    false,
                ));
            }
        }
    }

    out
}

/// Merges duplicates and puts the best first.
///
/// The same page is commonly in history several times over and bookmarked as
/// well, and in more than one browser. Collapsing on the address means one row
/// per page; keeping the bookmark when there is one means the row says it is
/// saved, which is the more useful of the two things to know.
pub fn rank(mut hits: Vec<Hit>, query: &str, limit: usize) -> Vec<Hit> {
    let needle = query.to_lowercase();

    hits.sort_by(|a, b| {
        score(b, &needle)
            .cmp(&score(a, &needle))
            .then_with(|| b.visits.cmp(&a.visits))
            .then_with(|| a.title.len().cmp(&b.title.len()))
    });

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(limit.min(hits.len()));

    for hit in hits {
        if out.len() >= limit {
            break;
        }
        if seen.insert(hit.url.clone()) {
            out.push(hit);
        }
    }

    out
}

/// How well a page answers what was typed.
///
/// Deliberately coarse. It only has to separate the obvious from the
/// incidental, because within a band the visit count decides, and that is a
/// better signal than any arithmetic over the title.
fn score(hit: &Hit, needle: &str) -> i32 {
    let title = hit.title.to_lowercase();
    let url = hit.url.to_lowercase();

    let mut score = match () {
        _ if title == *needle => 100,
        _ if title.starts_with(needle) => 80,
        // The host is what somebody means by "the site", so matching there
        // beats matching somewhere down a path.
        _ if host_of(&url).starts_with(needle) => 70,
        _ if title.contains(needle) => 50,
        _ if url.contains(needle) => 30,
        _ => 0,
    };

    // Saving a page is a deliberate act and visiting one is not, so a bookmark
    // outranks history of the same strength without outranking a better match.
    if hit.bookmark {
        score += 15;
    }

    score
}

fn host_of(url: &str) -> &str {
    url.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .trim_start_matches("www.")
}

/// Searches every profile of every browser that is present.
pub fn search(query: &str, limit: usize, want: Want, scratch: &Path) -> Vec<Hit> {
    if query.trim().is_empty() || (!want.history && !want.bookmarks) {
        return Vec::new();
    }

    let found = profiles();

    /*
     * Copies belonging to profiles that no longer exist are deleted here.
     *
     * On every search rather than on a timer, and with no "not more than once
     * an hour" to remember. It is a directory listing of a handful of files,
     * it only runs behind the debounce that already holds this whole search
     * back, and the throttle it replaces was a static mutable of exactly the
     * kind rule 2 forbids. Cheaper to do than to remember not to do.
     */
    sweep(scratch, &copies_for(&found, scratch), KEEP_ORPHANS_FOR);

    let mut hits = Vec::new();
    for profile in &found {
        hits.extend(from_profile(profile, query, want, scratch));
    }

    rank(hits, query, limit)
}

/// Where each live profile's copy would be, whether or not it has been made.
///
/// The same name `readable_copy` computes, and it has to stay the same name:
/// this is the list of paths the sweep is told to keep, and a name that
/// differed by so much as a separator would delete every copy on every pass
/// and then make them all again.
fn copies_for(profiles: &[Profile], into: &Path) -> Vec<PathBuf> {
    let mut kept = Vec::new();

    for profile in profiles {
        for source in [profile.history.as_ref(), profile.bookmarks.as_ref()]
            .into_iter()
            .flatten()
        {
            kept.push(copy_path(source, into));
        }
    }

    kept
}

/// Where Windows keeps the list of installed browsers.
///
/// Every browser that wants to be one registers here, with the name it calls
/// itself and the command that runs it. It is how the Default Apps page knows
/// what to offer, so it is the same list Windows itself works from.
#[cfg(windows)]
const BROWSER_CLIENTS: &str = r"SOFTWARE\Clients\StartMenuInternet";

/// The program that opens a web address on this machine.
///
/// Asked of the shell rather than worked out, because "the default browser" is
/// a question Windows already answers and the registry path behind it has
/// moved more than once.
#[cfg(windows)]
pub fn default_browser() -> Option<PathBuf> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE};

    let scheme: Vec<u16> = "http\0".encode_utf16().collect();
    let mut length: u32 = 0;

    // SAFETY: a null output buffer with a zero length is how this call is asked
    // for the size it needs. Both pointers are valid for the call.
    unsafe {
        let _ = AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            PCWSTR(scheme.as_ptr()),
            PCWSTR::null(),
            None,
            &mut length,
        );
    }

    if length == 0 {
        return None;
    }

    let mut buffer = vec![0u16; length as usize];

    // SAFETY: the buffer is exactly the size the call just asked for, and
    // `length` is passed by pointer so it can say how much it used.
    unsafe {
        AssocQueryStringW(
            ASSOCF_NONE,
            ASSOCSTR_EXECUTABLE,
            PCWSTR(scheme.as_ptr()),
            PCWSTR::null(),
            Some(PWSTR(buffer.as_mut_ptr())),
            &mut length,
        )
        .ok()
        .ok()?;
    }

    let text = String::from_utf16_lossy(&buffer[..buffer.len().saturating_sub(1)]);
    let path = PathBuf::from(text.trim_end_matches('\0'));

    path.is_file().then_some(path)
}

#[cfg(not(windows))]
pub fn default_browser() -> Option<PathBuf> {
    None
}

/// The program behind a browser Sill found profiles for.
///
/// Matched on the name Windows registers rather than on ours, because they are
/// not always the same word: this machine registers "Zen Browser" for what the
/// profile directory calls `zen`. One contains the other, which is enough to
/// tell ten browsers apart and does not need a table of aliases nobody will
/// remember to update.
#[cfg(windows)]
pub fn browser_exe(name: &str) -> Option<PathBuf> {
    let wanted = name.to_lowercase();

    let registered = installed_browsers().into_iter().find(|(registered, _)| {
        let registered = registered.to_lowercase();
        registered.contains(&wanted) || wanted.contains(&registered)
    });

    if let Some((_, path)) = registered {
        return Some(path);
    }

    /*
     * Not every browser registers as one.
     *
     * This machine has a Chrome profile full of history and no entry under
     * either hive, so the list Windows offers in Default Apps does not have it.
     * A program that does not claim to be a browser still has an icon, and
     * App Paths is where Windows records where programs live.
     */
    let exe = KNOWN
        .iter()
        .find(|known| known.name.to_lowercase() == wanted)
        .map(|known| known.exe)?;

    app_path(exe)
}

/// Where Windows records a program, by the name it is launched under.
#[cfg(windows)]
fn app_path(exe: &str) -> Option<PathBuf> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    const APP_PATHS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths";

    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let found = read_string(root, &format!(r"{APP_PATHS}\{exe}"), "")
            .map(|text| PathBuf::from(unquote(&text)))
            .filter(|path| path.is_file());

        if found.is_some() {
            return found;
        }
    }

    None
}

#[cfg(not(windows))]
pub fn browser_exe(_name: &str) -> Option<PathBuf> {
    None
}

/// Every browser Windows knows is installed, with the program behind it.
///
/// Both hives. A browser installed for one person registers under the user and
/// never appears machine-wide, which is how Chrome is usually installed: this
/// machine has a Chrome profile full of history and no HKLM entry at all.
#[cfg(windows)]
pub fn installed_browsers() -> Vec<(String, PathBuf)> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let mut out: Vec<(String, PathBuf)> = Vec::new();

    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for (name, path) in registered_under(root) {
            // The same browser can be registered in both hives.
            if !out.iter().any(|(seen, _)| seen == &name) {
                out.push((name, path));
            }
        }
    }

    out
}

#[cfg(windows)]
fn registered_under(root: windows::Win32::System::Registry::HKEY) -> Vec<(String, PathBuf)> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY, KEY_READ,
    };

    let mut out = Vec::new();

    let mut clients = HKEY::default();
    // SAFETY: the path is a valid wide string and the handle is closed below on
    // every path out.
    let opened = unsafe {
        RegOpenKeyExW(
            root,
            &HSTRING::from(BROWSER_CLIENTS),
            Some(0),
            KEY_READ,
            &mut clients,
        )
    };

    if opened.is_err() {
        return out;
    }

    let mut index = 0u32;
    loop {
        let mut name = [0u16; 256];
        let mut length = name.len() as u32;

        // SAFETY: `length` says how much room `name` has, and the call writes
        // no more than that.
        let read = unsafe {
            RegEnumKeyExW(
                clients,
                index,
                Some(windows::core::PWSTR(name.as_mut_ptr())),
                &mut length,
                None,
                None,
                None,
                None,
            )
        };

        if read.is_err() {
            break;
        }

        let key = String::from_utf16_lossy(&name[..length as usize]);
        if let Some(path) = client_command(root, &key) {
            out.push((client_name(root, &key).unwrap_or(key), path));
        }

        index += 1;
    }

    // SAFETY: the handle came from the matching open above.
    unsafe {
        let _ = RegCloseKey(clients);
    }

    out
}

/// What a registered browser calls itself.
#[cfg(windows)]
fn client_name(root: windows::Win32::System::Registry::HKEY, key: &str) -> Option<String> {
    read_string(root, &format!(r"{BROWSER_CLIENTS}\{key}"), "").filter(|s| !s.trim().is_empty())
}

/// The program a registered browser runs.
#[cfg(windows)]
fn client_command(root: windows::Win32::System::Registry::HKEY, key: &str) -> Option<PathBuf> {
    let command = read_string(
        root,
        &format!(r"{BROWSER_CLIENTS}\{key}\shell\open\command"),
        "",
    )?;
    let path = PathBuf::from(unquote(&command));

    path.is_file().then_some(path)
}

/// A registered command is usually quoted and may carry arguments.
#[cfg(windows)]
fn unquote(command: &str) -> String {
    let command = command.trim();

    if let Some(rest) = command.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest).to_string();
    }

    // Unquoted, so the program is everything up to the first argument. A path
    // with a space in it and no quotes is ambiguous and Windows treats it the
    // same way.
    command
        .split_once(" -")
        .map(|(program, _)| program)
        .unwrap_or(command)
        .trim()
        .to_string()
}

#[cfg(windows)]
fn read_string(
    root: windows::Win32::System::Registry::HKEY,
    path: &str,
    value: &str,
) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ};

    let mut size: u32 = 0;

    // SAFETY: a null buffer asks for the size, which is what `size` receives.
    unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(path),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }

    if size == 0 {
        return None;
    }

    let mut buffer = vec![0u8; size as usize];

    // SAFETY: the buffer is the size the call just asked for.
    unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(path),
            &HSTRING::from(value),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }

    let wide: Vec<u16> = buffer
        .chunks_exact(2)
        .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
        .take_while(|c| *c != 0)
        .collect();

    Some(String::from_utf16_lossy(&wide))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of this test's own, named after the test using it.
    ///
    /// Tests run at the same time as each other, and every one of these writes
    /// files. Sharing a directory would make them fail in whichever order they
    /// happened to interleave, which is the kind of flake nobody ever chases.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sill-browser-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    fn hit(title: &str, url: &str, bookmark: bool, visits: i64) -> Hit {
        Hit {
            title: title.to_string(),
            url: url.to_string(),
            browser: "Test".to_string(),
            bookmark,
            visits,
            icon: None,
        }
    }

    mod patterns {
        use super::*;

        /// Punctuation has to be punctuation.
        ///
        /// `%` and `_` are wildcards to SQL, so without escaping, typing a
        /// percent sign matches the entire history and an underscore matches
        /// any character at all. Both read as the search being broken.
        #[test]
        fn wildcards_are_escaped() {
            assert_eq!(like("100%"), r"%100\%%");
            assert_eq!(like("a_b"), r"%a\_b%");
            assert_eq!(like(r"back\slash"), r"%back\\slash%");
        }

        #[test]
        fn an_ordinary_word_is_wrapped_and_otherwise_left_alone() {
            assert_eq!(like("github"), "%github%");
        }

        #[test]
        fn the_host_is_what_a_site_is_called() {
            assert_eq!(host_of("https://www.example.com/a/b?c=d"), "example.com");
            assert_eq!(host_of("http://localhost:3000/x"), "localhost:3000");
            // Not every stored address has a scheme.
            assert_eq!(host_of("example.org/page"), "example.org");
        }
    }

    mod ranking {
        use super::*;

        /// The same page is usually in history many times and bookmarked too,
        /// and in more than one browser. One row per page is the whole point.
        #[test]
        fn one_row_per_address() {
            let out = rank(
                vec![
                    hit("Example", "https://example.com", false, 3),
                    hit("Example", "https://example.com", false, 3),
                    hit("Other", "https://other.com", false, 1),
                ],
                "example",
                10,
            );

            assert_eq!(out.len(), 2);
        }

        /// Saving a page is deliberate. Visiting one is not.
        #[test]
        fn a_saved_page_beats_a_visited_one_of_the_same_strength() {
            let out = rank(
                vec![
                    hit("Rust docs", "https://doc.rust-lang.org", false, 40),
                    hit("Rust docs", "https://rust-lang.org", true, 0),
                ],
                "rust docs",
                10,
            );

            assert!(out[0].bookmark, "the saved page did not come first");
        }

        /// But not at the cost of answering the question.
        #[test]
        fn a_better_match_still_beats_a_saved_page() {
            let out = rank(
                vec![
                    hit("Unrelated, mentions rust", "https://a.com/rust", true, 0),
                    hit("Rust", "https://b.com", false, 1),
                ],
                "rust",
                10,
            );

            assert_eq!(out[0].title, "Rust");
        }

        /// Within a band, how often somebody went there is the better signal.
        #[test]
        fn equal_matches_are_separated_by_how_often_they_were_visited() {
            let out = rank(
                vec![
                    hit("News", "https://rarely.com", false, 2),
                    hit("News", "https://daily.com", false, 900),
                ],
                "news",
                10,
            );

            assert_eq!(out[0].url, "https://daily.com");
        }

        #[test]
        fn the_limit_is_respected() {
            let many: Vec<Hit> = (0..50)
                .map(|i| hit("Page", &format!("https://example.com/{i}"), false, i))
                .collect();

            assert_eq!(rank(many, "page", 7).len(), 7);
        }
    }

    mod chromium_bookmark_file {
        use super::*;

        const TREE: &str = r#"{
          "roots": {
            "bookmark_bar": {
              "type": "folder",
              "name": "Bookmarks bar",
              "children": [
                { "type": "url", "name": "Rust", "url": "https://rust-lang.org" },
                {
                  "type": "folder",
                  "name": "Reading",
                  "children": [
                    { "type": "url", "name": "The Rust Book", "url": "https://doc.rust-lang.org/book" }
                  ]
                }
              ]
            },
            "other": {
              "type": "folder",
              "name": "Other",
              "children": [
                { "type": "url", "name": "Unrelated", "url": "https://example.com" }
              ]
            }
          }
        }"#;

        fn written(name: &str) -> PathBuf {
            let path = scratch(name).join("Bookmarks");
            std::fs::write(&path, TREE).expect("fixture written");
            path
        }

        /// Bookmarks nest, so a match can be at any depth.
        #[test]
        fn a_bookmark_inside_a_folder_is_found() {
            let found = chromium_bookmarks(&written("nested"), "rust book", "Test", None);

            assert_eq!(found.len(), 1);
            assert_eq!(found[0].url, "https://doc.rust-lang.org/book");
            assert!(found[0].bookmark);
        }

        /// Every root is walked, not only the bar.
        #[test]
        fn all_three_roots_are_read() {
            let found = chromium_bookmarks(&written("roots"), "unrelated", "Test", None);

            assert_eq!(found.len(), 1, "the other-bookmarks root was not read");
        }

        /// A folder called "Rust" is not a page, and offering it as one would
        /// give a row that cannot be opened.
        #[test]
        fn folders_are_not_offered_as_pages() {
            let found = chromium_bookmarks(&written("folders"), "reading", "Test", None);

            assert!(
                found.is_empty(),
                "a folder was returned as a page: {found:?}"
            );
        }

        #[test]
        fn a_file_that_is_not_there_is_not_an_error() {
            let missing = scratch("absent").join("Bookmarks");

            assert!(chromium_bookmarks(&missing, "anything", "Test", None).is_empty());
        }
    }

    mod reading_a_database {
        use super::*;

        /// A Chromium `History`, with only the columns that are read.
        fn chromium_history(at: &Path) {
            let db = rusqlite::Connection::open(at).expect("fixture database");
            db.execute_batch(
                "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER);
                 INSERT INTO urls (url, title, visit_count) VALUES
                   ('https://github.com', 'GitHub', 90),
                   ('https://github.com/rust-lang/rust', 'rust-lang/rust', 12),
                   ('https://example.com', 'Example', 3),
                   ('https://untitled.example', NULL, 1);",
            )
            .expect("fixture rows");
        }

        /// A Firefox `places.sqlite`, likewise.
        fn firefox_places(at: &Path) {
            let db = rusqlite::Connection::open(at).expect("fixture database");
            db.execute_batch(
                "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT,
                                          visit_count INTEGER, hidden INTEGER DEFAULT 0);
                 CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, fk INTEGER, type INTEGER, title TEXT);
                 INSERT INTO moz_places (id, url, title, visit_count, hidden) VALUES
                   (1, 'https://github.com', 'GitHub', 90, 0),
                   (2, 'https://redirect.example', 'Redirected', 40, 1),
                   (3, 'https://saved.example', 'Saved Page', 2, 0);
                 INSERT INTO moz_bookmarks (fk, type, title) VALUES (3, 1, 'Saved Page');",
            )
            .expect("fixture rows");
        }

        fn profile(family: Family, dir: &Path) -> Profile {
            match family {
                Family::Chromium => {
                    let history = dir.join("History");
                    chromium_history(&history);
                    Profile {
                        browser: "Test".into(),
                        family,
                        name: "Default".into(),
                        program: None,
                        history: Some(history),
                        bookmarks: None,
                    }
                }
                Family::Firefox => {
                    let places = dir.join("places.sqlite");
                    firefox_places(&places);
                    Profile {
                        browser: "Test".into(),
                        family,
                        name: "default".into(),
                        program: None,
                        history: Some(places.clone()),
                        bookmarks: Some(places),
                    }
                }
            }
        }

        #[test]
        fn a_chromium_history_is_searched_by_title_and_by_address() {
            let dir = scratch("chromium-read");
            let it = profile(Family::Chromium, &dir);

            let by_title = from_profile(&it, "GitHub", Want::default(), &dir.join("copies"));
            assert!(
                !by_title.is_empty(),
                "nothing matched a title that is there"
            );

            let by_url = from_profile(&it, "rust-lang", Want::default(), &dir.join("copies"));
            assert!(
                !by_url.is_empty(),
                "nothing matched an address that is there"
            );
        }

        /// A page can be stored with no title, and an empty row is unusable.
        #[test]
        fn a_page_with_no_title_falls_back_to_its_address() {
            let dir = scratch("chromium-untitled");
            let it = profile(Family::Chromium, &dir);

            let found = from_profile(
                &it,
                "untitled.example",
                Want::default(),
                &dir.join("copies"),
            );

            assert_eq!(found.len(), 1);
            assert_eq!(found[0].title, "https://untitled.example");
        }

        /// Firefox marks redirects and framed pages hidden. Nobody went there
        /// on purpose and nobody would recognise them in a list.
        #[test]
        fn firefox_hidden_visits_are_left_out() {
            let dir = scratch("firefox-hidden");
            let it = profile(Family::Firefox, &dir);

            let found = from_profile(&it, "Redirected", Want::default(), &dir.join("copies"));

            assert!(found.is_empty(), "a hidden visit was offered: {found:?}");
        }

        /// Firefox keeps bookmarks in the same file, so both come from one read.
        #[test]
        fn a_firefox_bookmark_comes_back_marked_as_saved() {
            let dir = scratch("firefox-saved");
            let it = profile(Family::Firefox, &dir);

            let found = from_profile(&it, "Saved Page", Want::default(), &dir.join("copies"));

            assert!(
                found.iter().any(|h| h.bookmark),
                "the bookmark was not marked: {found:?}"
            );
        }

        /// Turning one off has to actually stop reading it.
        #[test]
        fn asking_for_only_bookmarks_does_not_return_history() {
            let dir = scratch("firefox-only-saved");
            let it = profile(Family::Firefox, &dir);

            let found = from_profile(
                &it,
                "GitHub",
                Want {
                    history: false,
                    bookmarks: true,
                },
                &dir.join("copies"),
            );

            assert!(
                found.is_empty(),
                "history came back with history switched off: {found:?}"
            );
        }

        /// The original is somebody else's file and is never opened.
        #[test]
        fn the_original_is_read_through_a_copy() {
            let dir = scratch("copy-made");
            let copies = dir.join("copies");
            let it = profile(Family::Chromium, &dir);

            let _ = from_profile(&it, "GitHub", Want::default(), &copies);

            let made: Vec<_> = std::fs::read_dir(&copies)
                .expect("the copy directory")
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();

            assert!(
                made.iter().any(|name| name.ends_with("History")),
                "no copy was taken, so the original was read directly: {made:?}",
            );
        }
    }

    mod copies {
        use super::*;

        #[test]
        fn a_fresh_copy_is_reused_rather_than_taken_again() {
            let dir = scratch("copy-reuse");
            let source = dir.join("data.sqlite");
            std::fs::write(&source, b"first").expect("source written");

            let into = dir.join("copies");
            let once = readable_copy(&source, &into, Duration::from_secs(300)).expect("a copy");

            // If the copy were taken again, it would carry the new contents.
            std::fs::write(&source, b"second").expect("source rewritten");
            let twice = readable_copy(&source, &into, Duration::from_secs(300)).expect("a copy");

            assert_eq!(once, twice);
            assert_eq!(std::fs::read(&twice).expect("read back"), b"first");
        }

        #[test]
        fn a_stale_copy_is_taken_again() {
            let dir = scratch("copy-stale");
            let source = dir.join("data.sqlite");
            std::fs::write(&source, b"first").expect("source written");

            let into = dir.join("copies");
            readable_copy(&source, &into, Duration::from_secs(300)).expect("a copy");

            std::fs::write(&source, b"second").expect("source rewritten");
            // Nothing can be fresher than no age at all.
            let again = readable_copy(&source, &into, Duration::ZERO).expect("a copy");

            assert_eq!(std::fs::read(&again).expect("read back"), b"second");
        }

        /// Two files with the same name must not become one copy.
        ///
        /// Every Firefox profile calls its database `places.sqlite`, and two
        /// browsers both call theirs `History`, so the name alone collides and
        /// one profile would be answering for another.
        #[test]
        fn two_profiles_with_the_same_file_name_get_their_own_copies() {
            let dir = scratch("copy-collide");
            let one = dir.join("a");
            let two = dir.join("b");
            std::fs::create_dir_all(&one).expect("a");
            std::fs::create_dir_all(&two).expect("b");
            std::fs::write(one.join("places.sqlite"), b"one").expect("written");
            std::fs::write(two.join("places.sqlite"), b"two").expect("written");

            let into = dir.join("copies");
            let first =
                readable_copy(&one.join("places.sqlite"), &into, STALE_AFTER).expect("a copy");
            let second =
                readable_copy(&two.join("places.sqlite"), &into, STALE_AFTER).expect("a copy");

            assert_ne!(first, second);
            assert_eq!(std::fs::read(&first).expect("read back"), b"one");
            assert_eq!(std::fs::read(&second).expect("read back"), b"two");
        }

        /// The newest pages live in the write-ahead log, not in the database
        /// beside it, so a copy without it stops at the last checkpoint.
        #[test]
        fn the_write_ahead_log_is_copied_too() {
            let dir = scratch("copy-wal");
            let source = dir.join("places.sqlite");
            std::fs::write(&source, b"database").expect("source written");
            std::fs::write(dir.join("places.sqlite-wal"), b"log").expect("log written");

            let into = dir.join("copies");
            let copy = readable_copy(&source, &into, STALE_AFTER).expect("a copy");

            let log = with_suffix(&copy, "-wal");
            assert!(log.is_file(), "the log was left behind");
            assert_eq!(std::fs::read(&log).expect("read back"), b"log");
        }

        /// A log left over from a previous copy would be replayed against a
        /// database it does not belong to.
        #[test]
        fn a_log_that_is_no_longer_there_is_cleared_from_the_copy() {
            let dir = scratch("copy-wal-gone");
            let source = dir.join("places.sqlite");
            std::fs::write(&source, b"database").expect("source written");
            std::fs::write(dir.join("places.sqlite-wal"), b"log").expect("log written");

            let into = dir.join("copies");
            let copy = readable_copy(&source, &into, STALE_AFTER).expect("a copy");
            assert!(with_suffix(&copy, "-wal").is_file());

            // The browser checkpointed and removed its log.
            std::fs::remove_file(dir.join("places.sqlite-wal")).expect("log removed");
            readable_copy(&source, &into, Duration::ZERO).expect("a copy");

            assert!(
                !with_suffix(&copy, "-wal").is_file(),
                "a log from an earlier copy was left in place",
            );
        }

        #[test]
        fn a_source_that_is_not_there_gives_nothing() {
            let dir = scratch("copy-absent");

            assert!(
                readable_copy(&dir.join("nothing"), &dir.join("copies"), STALE_AFTER).is_none()
            );
        }
    }

    mod searching {
        use super::*;

        #[test]
        fn an_empty_query_reads_nothing() {
            let dir = scratch("search-empty");

            assert!(search("", 10, Want::default(), &dir).is_empty());
            assert!(search("   ", 10, Want::default(), &dir).is_empty());
        }

        /// Both switched off means the feature is off, so no file is touched.
        #[test]
        fn wanting_neither_reads_nothing() {
            let dir = scratch("search-neither");
            let want = Want {
                history: false,
                bookmarks: false,
            };

            assert!(search("github", 10, want, &dir).is_empty());
            assert!(!dir.join("copies").exists(), "a copy was taken anyway");
        }
    }

    /// A copy nobody claims is deleted once it is old enough.
    ///
    /// Nothing ever deleted these. One of them on this machine is 31 MB, and
    /// every one is a copy of somebody's browsing history, kept in Sill's own
    /// data directory for a browser they may have uninstalled months ago.
    #[test]
    fn an_orphaned_copy_is_swept() {
        let dir = scratch("sweep-orphan");
        let orphan = dir.join("abc123-History");
        std::fs::write(&orphan, b"old history").expect("written");

        // Nothing is kept for any length of time, so "old enough" is now.
        super::sweep(&dir, &[], Duration::ZERO);

        assert!(!orphan.exists(), "an orphaned copy was kept");
    }

    /// A copy a live profile claims is kept however old it is.
    #[test]
    fn a_claimed_copy_is_kept() {
        let dir = scratch("sweep-claimed");
        let claimed = dir.join("abc123-History");
        std::fs::write(&claimed, b"still in use").expect("written");

        super::sweep(&dir, &[claimed.clone()], Duration::ZERO);

        assert!(claimed.exists(), "a copy still in use was deleted");
    }

    /// A recent orphan is left alone.
    ///
    /// A browser mid-upgrade, or a profile on a drive that is unplugged this
    /// minute, looks gone. The day of grace is what stops a sweep during one
    /// of those moments throwing away a copy that is wanted again in an hour.
    #[test]
    fn a_fresh_orphan_is_given_its_grace() {
        let dir = scratch("sweep-fresh");
        let fresh = dir.join("abc123-History");
        std::fs::write(&fresh, b"copied a moment ago").expect("written");

        super::sweep(&dir, &[], Duration::from_secs(60 * 60 * 24));

        assert!(fresh.exists(), "a copy made moments ago was swept");
    }

    /// The write-ahead log goes with its database rather than on its own.
    #[test]
    fn a_log_is_kept_with_the_database_it_belongs_to() {
        let dir = scratch("sweep-wal");
        let db = dir.join("abc123-History");
        let wal = dir.join("abc123-History-wal");
        std::fs::write(&db, b"database").expect("written");
        std::fs::write(&wal, b"log").expect("written");

        super::sweep(&dir, &[db.clone()], Duration::ZERO);

        assert!(db.exists());
        assert!(
            wal.exists(),
            "the log was swept out from under its database"
        );
    }

    /// And a log whose database is gone goes with it.
    #[test]
    fn a_log_with_no_database_left_is_swept_too() {
        let dir = scratch("sweep-lone-wal");
        let wal = dir.join("abc123-History-wal");
        std::fs::write(&wal, b"log").expect("written");

        super::sweep(&dir, &[], Duration::ZERO);

        assert!(!wal.exists(), "a log with nothing to belong to was kept");
    }
}
