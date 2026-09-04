//! File search, delegated to Everything.
//!
//! Windows has no fast general file index. Its own Search service is slow and
//! partial, and walking the filesystem per keystroke is out of the question.
//! Everything reads the NTFS Master File Table directly and answers in
//! milliseconds, which is the only approach fast enough to run per keystroke.
//!
//! Queries go straight to Everything's IPC window, in `everything_ipc`. That
//! is the same protocol `Everything64.dll` speaks, so it costs no third-party
//! binary and no process per keystroke.
//!
//! `es.exe`, Everything's command line client, remains as a fallback for the
//! case where the IPC window cannot be reached. It is correct but spawns a
//! process per query, and `CreateProcess` is tens of milliseconds, which is
//! why it is not the first choice.

use serde::Serialize;

/// A file Everything matched.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHit {
    /// Base name, which is what a person recognises.
    pub name: String,
    /// Full path, used to open it and shown as the subtitle.
    pub path: String,
    /// Directories are worth showing differently from documents.
    pub is_dir: bool,
}

/// Where `es.exe` lives.
///
/// Tried on `PATH` first, then beside Everything itself. Returning the bare
/// name lets the shell resolve it, including through the WindowsApps alias.
fn client() -> Option<String> {
    for candidate in [
        r"C:\Program Files\Everything\es.exe",
        r"C:\Program Files (x86)\Everything\es.exe",
    ] {
        if std::path::Path::new(candidate).is_file() {
            return Some(candidate.to_string());
        }
    }

    /*
     * Then `PATH`, actually looked along.
     *
     * This used to end `Some("es".to_string())`, on the reasoning that the
     * shell would resolve it. The shell would, and it also meant this function
     * **never returned `None`**, so `available()` was always true, so
     * `missing()` could never report that file search was absent or asleep,
     * and the row that says so and offers to fix it was unreachable on every
     * machine. A fallback that is always taken is not a fallback.
     */
    let path = std::env::var_os("PATH")?;

    std::env::split_paths(&path)
        .map(|dir| dir.join("es.exe"))
        .find(|candidate| candidate.is_file())
        .map(|found| found.to_string_lossy().into_owned())
}

/// Why file search cannot answer.
///
/// The two cases need different words and different remedies, and guessing
/// wrong is worse than saying nothing: telling somebody to install what they
/// already have reads as the launcher being broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Missing {
    /// Sill is still reading the folders it was told to index.
    ///
    /// Not a fault and nothing to act on, but the difference between "no
    /// results yet" and "no results" is the difference between waiting a
    /// second and going to look for a setting.
    Indexing,
    /// Nothing to search: Sill indexes nothing of its own, and no
    /// whole-volume indexer is on the machine either.
    Absent,
    /// On the machine, but not running. Every route to it talks to the
    /// process, so an installed copy sitting closed answers nothing.
    Asleep,
}

/// What is standing between a typed query and a list of files.
///
/// `None` means file search works. Asked when the launcher is summoned rather
/// than per keystroke: it is one window lookup, but the answer only changes
/// when a program starts or stops, which is not something typing does.
pub fn missing(enabled: bool, indexed: usize, building: bool) -> Option<Missing> {
    verdict(enabled, indexed, building, available(), installed())
}

/// The rule itself, with the machine taken out of it.
///
/// Separated because the three inputs are facts about one particular Windows
/// install and the rule about them is not. This is the part worth pinning
/// down: **switched off is not a problem to report**, and telling somebody
/// their file search is broken when they turned it off themselves is the kind
/// of thing that makes a launcher feel like it is nagging.
fn verdict(
    enabled: bool,
    indexed: usize,
    building: bool,
    running: bool,
    installed: bool,
) -> Option<Missing> {
    // Switched off is not a problem to report: somebody turned it off.
    if !enabled {
        return None;
    }

    // Sill has files of its own to search. That a whole-volume indexer is
    // absent is then not worth a word: it would have found more, and nobody
    // needs telling about results they were never going to see.
    if indexed > 0 || running {
        return None;
    }

    if building {
        return Some(Missing::Indexing);
    }

    Some(if installed {
        Missing::Asleep
    } else {
        Missing::Absent
    })
}

/// Where the program itself lives, when it does.
///
/// The standard install locations, which is where the package manager puts it
/// too. **A portable copy in a folder of somebody's own reads as absent**, and
/// the remedy offered then is an install that the package manager will decline
/// as already present. That is a worse answer than the truth and a better one
/// than silence.
fn installed() -> bool {
    [
        r"C:\Program Files\Everything\Everything.exe",
        r"C:\Program Files (x86)\Everything\Everything.exe",
        r"C:\Program Files\Everything\es.exe",
        r"C:\Program Files (x86)\Everything\es.exe",
    ]
    .iter()
    .any(|candidate| std::path::Path::new(candidate).is_file())
}

/// Starts the installed copy.
///
/// Started detached and left alone. It puts itself in the notification area
/// and builds its index on its own; waiting on it here would block a command
/// the window is waiting for, to learn nothing it cannot learn by asking again.
pub fn start() -> Result<(), String> {
    let program = [
        r"C:\Program Files\Everything\Everything.exe",
        r"C:\Program Files (x86)\Everything\Everything.exe",
    ]
    .into_iter()
    .find(|candidate| std::path::Path::new(candidate).is_file())
    .ok_or_else(|| "Cannot find it to start.".to_string())?;

    std::process::Command::new(program)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not start file search: {e}"))
}

/// Whether file search can work at all.
///
/// Everything must be running, not merely installed: every route to it talks
/// to the running process.
pub fn available() -> bool {
    #[cfg(windows)]
    if crate::everything_ipc::available() {
        return true;
    }
    client().is_some()
}

/// Files matching a query, in Everything's own relevance order.
///
/// Returns nothing rather than an error when Everything is absent. File search
/// is an enhancement, and a launcher that refuses to search commands because a
/// third-party indexer is missing would be worse than one that quietly offers
/// less.
pub fn search(query: &str, limit: usize) -> Vec<FileHit> {
    search_with(query, limit, false, false, false)
}

/// Narrows a query to a set of folders, using Everything's own syntax.
///
/// `path:` matches anywhere in the full path, and the alternatives are grouped
/// so the folder restriction applies to the whole query rather than only to
/// its last term. Returns the query untouched when no folders are set, which
/// is the common case and must not pay for this.
pub fn scope(query: &str, folders: &[String]) -> String {
    let folders: Vec<&str> = folders
        .iter()
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
        .collect();

    if folders.is_empty() {
        return query.to_string();
    }

    let clause = folders
        .iter()
        // A quoted path is one term however many spaces it contains.
        .map(|f| format!("path:\"{}\"", f.replace('"', "")))
        .collect::<Vec<_>>()
        .join("|");

    format!("{query} <{clause}>")
}

/// A search with the user's match settings applied.
pub fn search_with(
    query: &str,
    limit: usize,
    match_path: bool,
    match_case: bool,
    regex: bool,
) -> Vec<FileHit> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    // Direct IPC first: no process, no bundled DLL.
    #[cfg(windows)]
    {
        use crate::everything_ipc as ipc;

        let mut flags = 0;
        if match_path {
            flags |= ipc::MATCH_PATH;
        }
        if match_case {
            flags |= ipc::MATCH_CASE;
        }
        if regex {
            flags |= ipc::REGEX;
        }

        // Supersedable: this is the keystroke path, and the window keeps only
        // the answer to the last one anybody typed.
        let hits = ipc::search_newest(query, limit, flags);
        if !hits.is_empty() {
            return hits;
        }
    }

    search_via_client(query, limit)
}

/// The fallback path, spawning Everything's command line client.
fn search_via_client(query: &str, limit: usize) -> Vec<FileHit> {
    let Some(client) = client() else {
        return Vec::new();
    };

    let output = std::process::Command::new(client)
        .args([
            "-n",
            &limit.to_string(),
            // Ask for the full path, and for the attributes that identify a
            // directory, so the UI does not have to stat every result.
            "-p",
            "-attributes",
            query,
        ])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    // Everything's output is not guaranteed UTF-8 on every locale.
    let text = String::from_utf8_lossy(&output.stdout);

    text.lines().filter_map(parse_line).take(limit).collect()
}

/// Parses one `es -attributes` line: attributes, whitespace, then the path.
fn parse_line(line: &str) -> Option<FileHit> {
    let line = line.trim_end_matches('\r');
    if line.trim().is_empty() {
        return None;
    }

    // With -attributes the row is "<hex or letters> <path>". Without it, the
    // whole line is the path, so a line that does not split is still valid.
    let (attributes, path) = match line.split_once(char::is_whitespace) {
        Some((first, rest)) if looks_like_attributes(first) => (first, rest.trim_start()),
        _ => ("", line),
    };

    if path.is_empty() {
        return None;
    }

    let name = std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    Some(FileHit {
        name,
        path: path.to_string(),
        // "D" is the directory attribute in Everything's output.
        is_dir: attributes.contains('D'),
    })
}

/// Whether a leading token is an attribute column rather than part of a path.
///
/// A path always contains a separator or a drive colon; an attribute column is
/// only letters. Without this check a file at the filesystem root would have
/// its first path segment eaten.
fn looks_like_attributes(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| c.is_ascii_alphabetic())
        && !token.contains('\\')
        && !token.contains('/')
}

// ------------------------------------------------------- what was open lately

/// How long a reading of the Recent folder is good for.
///
/// The folder changes when somebody opens a file, which is not something that
/// happens while they are typing into the launcher. Reading it per keystroke
/// would be a directory listing per character for an answer that is the same
/// every time; reading it once and holding it for the length of a summon is
/// the whole of what this costs.
///
/// Short enough that a file opened, and then looked for a moment later, is
/// there. Long enough that a query typed one character at a time reads the
/// folder once.
pub const RECENT_FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(5);

/// How many shortcuts are kept.
///
/// The folder holds a few hundred on a machine that has been used for a while
/// and Windows prunes it itself. Newest first and capped, so the memory is
/// stated rather than however many shortcuts happen to be there: at roughly a
/// hundred and eighty bytes each this is **under sixty kilobytes**, and it is
/// let go of when the launcher hides.
pub const RECENT_MOST: usize = 300;

/// One shortcut in the Recent folder, before anything has been opened.
///
/// The name is the shortcut's own, without `.lnk`, which is the name of the
/// file it points at. That is the whole reason a listing is enough to match
/// against: **nothing is read until a row is going to be shown**, so a query
/// that matches two of three hundred shortcuts opens two files and not three
/// hundred.
#[derive(Debug, Clone)]
pub struct Trace {
    pub name: String,
    pub at: std::path::PathBuf,
}

/// Where Windows keeps a shortcut to everything recently opened.
pub fn recent_folder() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let folder = std::path::PathBuf::from(appdata).join(r"Microsoft\Windows\Recent");

    folder.is_dir().then_some(folder)
}

/// The shortcuts in one folder, newest first.
///
/// The write time comes from the directory listing, which on Windows is
/// already in hand from the scan, so ordering three hundred of them costs no
/// disk at all. Only the ordering needs it, so it is not kept.
pub fn traces(folder: &std::path::Path, most: usize) -> Vec<Trace> {
    let Ok(listing) = std::fs::read_dir(folder) else {
        return Vec::new();
    };

    let mut found: Vec<(std::time::SystemTime, Trace)> = Vec::new();

    for entry in listing.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };

        // `.lnk` and nothing else. The folder also holds `AutomaticDestinations`
        // and `CustomDestinations`, which are jump list databases rather than
        // shortcuts, and a `.url` is a web page rather than a file.
        let Some(stem) = strip_extension(name, "lnk") else {
            continue;
        };

        let when = entry
            .metadata()
            .and_then(|md| md.modified())
            .unwrap_or(std::time::UNIX_EPOCH);

        found.push((
            when,
            Trace {
                name: stem.to_string(),
                at: entry.path(),
            },
        ));
    }

    found.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    found.truncate(most);
    found.into_iter().map(|(_, trace)| trace).collect()
}

/// A file name with one particular extension taken off, when it has it.
fn strip_extension<'a>(name: &'a str, extension: &str) -> Option<&'a str> {
    let dot = name.rfind('.')?;

    (dot != 0 && name[dot + 1..].eq_ignore_ascii_case(extension)).then(|| &name[..dot])
}

/// The recently opened files a query matches.
///
/// Ranked by the same code that ranks everything else, so a recent file sorts
/// against the rest of the list rather than by its own idea of a good match.
///
/// Shortcuts are opened only for the rows that are going to be returned, which
/// is what keeps this a listing rather than three hundred file reads. A
/// shortcut whose target has since been deleted is dropped rather than offered:
/// a row that cannot be opened is worse than one that is not there.
pub fn from_recent(
    traces: &[Trace],
    query: &str,
    limit: usize,
    only_in: &[String],
) -> Vec<FileHit> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let needle: Vec<char> = query.to_lowercase().chars().collect();

    let mut scored: Vec<(crate::registry::MatchClass, usize, &Trace)> = traces
        .iter()
        .filter_map(|trace| {
            let (class, _) = crate::registry::match_name(&needle, &trace.name)?;
            Some((class, trace.name.chars().count(), trace))
        })
        .collect();

    scored.sort_unstable_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.name.cmp(&b.2.name))
    });

    let mut out = Vec::new();

    for (_, _, trace) in scored {
        if out.len() >= limit {
            break;
        }

        let Some(target) = crate::lnk::target_of(&trace.at) else {
            continue;
        };

        if !crate::catalog::inside_any(&target, only_in) {
            continue;
        }

        if !still_there(&target) {
            continue;
        }

        let path = std::path::Path::new(&target);
        let is_dir = path.is_dir();

        out.push(FileHit {
            name: crate::files_ops::name_of(path),
            path: target,
            is_dir,
        });
    }

    out
}

/// Whether the file a shortcut points at is still on the machine.
///
/// A share is believed without being asked. `exists()` on a UNC path that is
/// not reachable blocks until SMB gives up, which is tens of seconds, and this
/// runs while somebody is typing. The catalog's own root check makes the same
/// trade for the same reason. Offering a row that turns out to be gone is a
/// far smaller fault than a launcher that stops responding.
fn still_there(target: &str) -> bool {
    if target.starts_with("\\\\") || target.starts_with("//") {
        return true;
    }

    std::path::Path::new(target).exists()
}

#[cfg(test)]
mod tests {
    use super::{
        from_recent, looks_like_attributes, parse_line, strip_extension, traces, verdict, Missing,
    };

    #[test]
    fn a_bare_path_line_parses() {
        let hit = parse_line(r"C:\Users\me\notes.md").expect("a path is a result");
        assert_eq!(hit.name, "notes.md");
        assert_eq!(hit.path, r"C:\Users\me\notes.md");
        assert!(!hit.is_dir);
    }

    #[test]
    fn an_attribute_column_is_stripped() {
        let hit = parse_line(r"RASD C:\Users\me\Documents").expect("a path is a result");
        assert_eq!(hit.name, "Documents");
        assert_eq!(hit.path, r"C:\Users\me\Documents");
        assert!(hit.is_dir, "the D attribute marks a directory");
    }

    #[test]
    fn a_path_is_never_mistaken_for_attributes() {
        // The guard that keeps a rooted path intact.
        assert!(!looks_like_attributes(r"C:\Windows"));
        assert!(!looks_like_attributes("/usr"));
        assert!(looks_like_attributes("RASD"));
    }

    #[test]
    fn blank_lines_are_ignored() {
        assert!(parse_line("").is_none());
        assert!(parse_line("   ").is_none());
    }

    #[test]
    fn switched_off_is_not_a_problem_to_report() {
        // Somebody turned it off. Offering to install a file indexer they
        // deliberately stopped using is nagging, not helping.
        for building in [true, false] {
            for running in [true, false] {
                for installed in [true, false] {
                    assert_eq!(
                        verdict(false, 0, building, running, installed),
                        None,
                        "complained while switched off"
                    );
                }
            }
        }
    }

    #[test]
    fn our_own_index_is_enough_and_nothing_else_is_mentioned() {
        // The point of having one. A machine with no whole-volume indexer at
        // all still has working file search, and telling somebody to install
        // one would be advertising rather than helping.
        assert_eq!(verdict(true, 40_000, false, false, false), None);
        assert_eq!(verdict(true, 1, false, false, false), None);
    }

    #[test]
    fn a_first_run_says_it_is_working_rather_than_that_it_is_broken() {
        // The walk takes over a second. Offering to install something during
        // it would be wrong twice over: nothing is missing, and by the time
        // anybody read the row it would be gone.
        assert_eq!(
            verdict(true, 0, true, false, false),
            Some(Missing::Indexing)
        );
        assert_eq!(verdict(true, 0, true, false, true), Some(Missing::Indexing));
    }

    #[test]
    fn a_running_indexer_is_never_a_problem() {
        // Including the case where it is running from somewhere the install
        // probe cannot see, which is the portable copy `installed` admits it
        // misses. Running is the fact that matters; installed is only how the
        // remedy is worded.
        assert_eq!(verdict(true, 0, false, true, false), None);
        assert_eq!(verdict(true, 0, false, true, true), None);
    }

    // ------------------------------------------------- what was open lately

    /// The smallest shortcut the parser will read, pointing where told.
    ///
    /// A real one carries a target ID list, a volume record and a description;
    /// none of that is read here, so none of it is written. See `lnk` for the
    /// layout: a fixed header, then a `LinkInfo` whose ANSI path sits at the
    /// offset its own header names.
    fn a_shortcut_to(target: &str) -> Vec<u8> {
        const HAS_LINK_INFO: u32 = 1 << 1;
        const INFO_HEADER: u32 = 0x1C;

        let mut out = vec![0u8; 0x4C];
        out[0..4].copy_from_slice(&0x4Cu32.to_le_bytes());
        out[0x14..0x18].copy_from_slice(&HAS_LINK_INFO.to_le_bytes());

        let path = target.as_bytes();
        let size = INFO_HEADER + path.len() as u32 + 1;

        for field in [size, INFO_HEADER, 0, 0, INFO_HEADER, 0, 0] {
            out.extend_from_slice(&field.to_le_bytes());
        }

        out.extend_from_slice(path);
        out.push(0);
        out
    }

    #[test]
    fn a_shortcut_is_named_after_the_file_it_points_at() {
        // Which is what makes a listing enough to match against, and so what
        // stops a keystroke opening three hundred files.
        assert_eq!(
            strip_extension("budget.xlsx.lnk", "lnk"),
            Some("budget.xlsx")
        );
        assert_eq!(strip_extension("notes.LNK", "lnk"), Some("notes"));
        assert_eq!(strip_extension("notes.url", "lnk"), None);
        assert_eq!(strip_extension("nodots", "lnk"), None);
        // A name that is nothing but an extension is a dotfile, not a
        // shortcut called nothing.
        assert_eq!(strip_extension(".lnk", "lnk"), None);
    }

    #[test]
    fn only_shortcuts_are_read_from_the_recent_folder() {
        // The folder also holds the jump list databases, which are not
        // shortcuts and do not parse as any.
        let dir = tempfile::tempdir().expect("temp dir");

        for name in [
            "budget.xlsx.lnk",
            "notes.md.lnk",
            "somewhere.url",
            "f01b4d95cf55d32a.automaticDestinations-ms",
        ] {
            std::fs::write(dir.path().join(name), b"anything").expect("write");
        }

        let mut found: Vec<String> = traces(dir.path(), 10)
            .into_iter()
            .map(|trace| trace.name)
            .collect();
        found.sort();

        assert_eq!(found, vec!["budget.xlsx", "notes.md"]);
    }

    #[test]
    fn the_recent_listing_is_capped_and_takes_the_newest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let now = std::time::SystemTime::now();

        for age in 0..10u64 {
            let at = dir.path().join(format!("file{age}.txt.lnk"));
            std::fs::write(&at, b"anything").expect("write");

            let file = std::fs::File::options()
                .write(true)
                .open(&at)
                .expect("open");
            file.set_modified(now - std::time::Duration::from_secs(age * 60))
                .expect("set the write time");
        }

        let found = traces(dir.path(), 3);

        assert_eq!(
            found.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            vec!["file0.txt", "file1.txt", "file2.txt"],
            "the cap should keep the newest, not whichever three the \
             filesystem listed first"
        );
    }

    #[test]
    fn a_recently_opened_file_is_found_by_its_name() {
        let dir = tempfile::tempdir().expect("temp dir");
        let real = dir.path().join("budget.xlsx");
        std::fs::write(&real, b"a spreadsheet").expect("write");

        let recent = dir.path().join("recent");
        std::fs::create_dir(&recent).expect("a folder");
        std::fs::write(
            recent.join("budget.xlsx.lnk"),
            a_shortcut_to(&real.to_string_lossy()),
        )
        .expect("write");

        let found = from_recent(&traces(&recent, 10), "budget", 5, &[]);

        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "budget.xlsx");
        assert_eq!(found[0].path, real.to_string_lossy());
    }

    /// A shortcut outlives the file it points at, and a dead row is worse than
    /// a missing one.
    ///
    /// Windows leaves the shortcut behind when a document is deleted or moved,
    /// so the Recent folder on any machine that has been used for a while is
    /// partly a list of things that are not there.
    #[test]
    fn a_shortcut_whose_file_is_gone_is_not_offered() {
        let dir = tempfile::tempdir().expect("temp dir");
        let recent = dir.path().join("recent");
        std::fs::create_dir(&recent).expect("a folder");

        std::fs::write(
            recent.join("deleted.docx.lnk"),
            a_shortcut_to(&dir.path().join("deleted.docx").to_string_lossy()),
        )
        .expect("write");

        assert!(
            from_recent(&traces(&recent, 10), "deleted", 5, &[]).is_empty(),
            "a row that cannot be opened was offered"
        );
    }

    /// And the folder setting narrows these as well as the index.
    ///
    /// It says "only show results in". A source that ignored it would be a
    /// setting that half works, which is worse than one that is not there.
    #[test]
    fn the_folder_setting_narrows_what_was_open_lately() {
        let dir = tempfile::tempdir().expect("temp dir");
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("a folder");

        let real = elsewhere.join("budget.xlsx");
        std::fs::write(&real, b"a spreadsheet").expect("write");

        let recent = dir.path().join("recent");
        std::fs::create_dir(&recent).expect("a folder");
        std::fs::write(
            recent.join("budget.xlsx.lnk"),
            a_shortcut_to(&real.to_string_lossy()),
        )
        .expect("write");

        let listing = traces(&recent, 10);

        assert_eq!(
            from_recent(
                &listing,
                "budget",
                5,
                &[elsewhere.to_string_lossy().into_owned()]
            )
            .len(),
            1,
            "the folder it is in should not have excluded it"
        );

        assert!(
            from_recent(
                &listing,
                "budget",
                5,
                &[dir.path().join("nowhere").to_string_lossy().into_owned()]
            )
            .is_empty(),
            "a file outside the only folder asked for was offered anyway"
        );
    }

    #[test]
    fn the_remedy_matches_what_is_actually_wrong() {
        // The whole reason there are two variants. "Install this" to somebody
        // who already has it reads as the launcher being broken.
        assert_eq!(verdict(true, 0, false, false, true), Some(Missing::Asleep));
        assert_eq!(verdict(true, 0, false, false, false), Some(Missing::Absent));
    }
}
