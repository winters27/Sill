//! File search, delegated to Everything.
//!
//! Windows has no fast general file index. Its own Search service is slow and
//! partial, and walking the filesystem per keystroke is out of the question.
//! Everything reads the NTFS Master File Table directly and answers in
//! milliseconds, which is the same technique Raycast built for itself.
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

    Some("es".to_string())
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

        let hits = ipc::search_with(query, limit, flags);
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
    use super::{looks_like_attributes, parse_line};

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
}
