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

#[cfg(test)]
mod tests {
    use super::{looks_like_attributes, parse_line, verdict, Missing};

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

    #[test]
    fn the_remedy_matches_what_is_actually_wrong() {
        // The whole reason there are two variants. "Install this" to somebody
        // who already has it reads as the launcher being broken.
        assert_eq!(verdict(true, 0, false, false, true), Some(Missing::Asleep));
        assert_eq!(verdict(true, 0, false, false, false), Some(Missing::Absent));
    }
}
