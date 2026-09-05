//! Looking inside files, when the query asks to.
//!
//! `content:invoice` narrows a file search to the ones that hold the word,
//! and the row then shows the line it was found on rather than the path,
//! because the line is what says whether it is the right file.
//!
//! ## Why this is bounded rather than thorough
//!
//! Sill's index holds names, not contents, and building a content index is a
//! different product: it means reading every file on the machine, keeping a
//! second index the size of the text, and updating it as things change. That
//! is what a desktop search service is, and there is one on this machine
//! already.
//!
//! What this is instead is the question people actually ask, which is "the
//! file I am looking at the name of, does it have this in it". So it reads
//! the files a name search already found, newest first, and stops at three
//! bounds: how many files, how much of each, and how long altogether. Past
//! any of them it answers with what it has rather than with everything.
//!
//! **It also stops the moment the search is overtaken.** Checked between
//! files, the way the browser search checks, so a query that was replaced
//! while this was reading stops within one file rather than reading its two
//! hundred for a keystroke nobody is waiting on.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. There is no operator in an ordinary query, so nothing here runs.

use std::path::Path;
use std::time::{Duration, Instant};

/// How far a content search is allowed to go.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// How many files are opened at most.
    pub files: usize,
    /// How much of each is read.
    pub bytes_each: u64,
    /// How long the whole thing may take.
    pub budget: Duration,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            // Two hundred files at half a megabyte is a hundred megabytes of
            // reading in the worst case, which the deadline below is what
            // actually stops. Both are here because either alone has a bad
            // case: a deadline alone opens unbounded files on a fast disk,
            // and a count alone waits out a slow network drive.
            files: 200,
            bytes_each: 512 * 1024,
            budget: Duration::from_millis(300),
        }
    }
}

/// How much of a matching line is worth showing.
const LINE: usize = 120;

/// How much of the head of a file is looked at before deciding it is binary.
const SNIFF: usize = 8 * 1024;

/// The extensions worth opening.
///
/// An allow-list rather than "anything without a NUL", because the NUL test
/// is a test on bytes that have already been read off a disk, and the point
/// of this list is to not read them. A `.zip` and a `.mp4` are the common
/// large files in anybody's folders and neither has a line in it.
const TEXTUAL: &[&str] = &[
    "txt", "md", "markdown", "rst", "log", "csv", "tsv", "json", "jsonc", "toml", "yaml", "yml",
    "ini", "cfg", "conf", "env", "xml", "svg", "html", "htm", "css", "scss", "less", "js", "mjs",
    "cjs", "ts", "tsx", "jsx", "rs", "go", "py", "rb", "php", "java", "kt", "kts", "c", "h", "cpp",
    "hpp", "cc", "cs", "swift", "sql", "sh", "bash", "zsh", "ps1", "psm1", "bat", "cmd", "lua",
    "vim", "tex", "bib", "gradle", "properties", "gitignore", "editorconfig", "dockerfile",
    "makefile", "srt", "vtt", "patch", "diff",
];

/// Whether a name is one worth opening to look inside.
///
/// A file with no extension is opened: `Makefile`, `Dockerfile` and `LICENSE`
/// are all text and all common, and the binary test below is what catches the
/// ones that are not.
pub fn looks_textual(name: &str) -> bool {
    let Some(dot) = name.rfind('.') else {
        return true;
    };

    // A name beginning with a dot has no extension, it is a name: `.gitignore`
    // and `.env` are both text.
    if dot == 0 {
        return true;
    }

    let extension = name[dot + 1..].to_ascii_lowercase();
    TEXTUAL.contains(&extension.as_str())
}

/// Whether these bytes are a binary file rather than text.
///
/// A NUL in the first few kilobytes, which is the test every tool that has to
/// answer this quickly uses. It is a heuristic and says so: what it is
/// protecting against is a page of control characters in a result row.
pub fn binary(bytes: &[u8]) -> bool {
    bytes.iter().take(SNIFF).any(|byte| *byte == 0)
}

/// The line these bytes hold the needle on, if they hold it at all.
///
/// Case is ignored, on the ASCII letters only: a content search is a search,
/// and somebody typing `todo` means `TODO`. The line comes back trimmed and
/// cut to something a row can hold, with its inner whitespace squashed so a
/// deeply indented line does not arrive as a row of spaces.
pub fn holds(bytes: &[u8], needle: &str) -> Option<String> {
    if needle.is_empty() || binary(bytes) {
        return None;
    }

    let text = String::from_utf8_lossy(bytes);
    let needle = needle.to_ascii_lowercase();

    for line in text.lines() {
        if !line.to_ascii_lowercase().contains(&needle) {
            continue;
        }

        let tidy: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if tidy.is_empty() {
            continue;
        }

        return Some(match tidy.char_indices().nth(LINE) {
            Some((at, _)) => format!("{}…", &tidy[..at]),
            None => tidy,
        });
    }

    None
}

/// The head of a file, up to `most` bytes.
///
/// Only the head, because a match past half a megabyte of one file is not
/// what somebody typing into a launcher is looking for, and reading the whole
/// of a log that happens to be a gigabyte is how a search stops answering.
pub fn head_of(path: &Path, most: u64) -> Option<Vec<u8>> {
    use std::io::Read;

    let file = std::fs::File::open(path).ok()?;
    let mut out = Vec::new();
    file.take(most).read_to_end(&mut out).ok()?;

    Some(out)
}

/// One file that held the needle, and the line it was on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    pub line: String,
}

/// The files among these that hold the needle, with the line each was on.
///
/// The reading is handed in so the bounds can be checked without a disk: the
/// three things worth pinning down here are when it stops, and a test that
/// had to write two hundred files to ask would be a test nobody runs.
pub fn matching(
    paths: &[String],
    needle: &str,
    bounds: Bounds,
    mut read: impl FnMut(&Path, u64) -> Option<Vec<u8>>,
    still_wanted: impl Fn() -> bool,
) -> Vec<Found> {
    let mut found = Vec::new();

    if needle.trim().is_empty() {
        return found;
    }

    let began = Instant::now();
    let mut opened = 0usize;

    for path in paths {
        if opened >= bounds.files || began.elapsed() >= bounds.budget || !still_wanted() {
            break;
        }

        let name = path.rsplit(['\\', '/']).next().unwrap_or(path);
        if !looks_textual(name) {
            continue;
        }

        opened += 1;

        let Some(bytes) = read(Path::new(path), bounds.bytes_each) else {
            continue;
        };

        if let Some(line) = holds(&bytes, needle) {
            found.push(Found {
                path: path.clone(),
                line,
            });
        }
    }

    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(count: usize) -> Vec<String> {
        (0..count).map(|n| format!(r"C:\a\file{n}.txt")).collect()
    }

    fn always() -> bool {
        true
    }

    #[test]
    fn a_match_carries_its_line() {
        let text = b"first line\n    the TODO is here, indented   \nlast line";
        assert_eq!(
            holds(text, "todo").as_deref(),
            Some("the TODO is here, indented")
        );

        assert_eq!(holds(text, "nothing here"), None);
        assert_eq!(holds(text, ""), None);
    }

    #[test]
    fn a_very_long_line_is_cut_rather_than_shown_whole() {
        let long = format!("needle {}", "x".repeat(500));
        let line = holds(long.as_bytes(), "needle").expect("finds it");

        assert!(line.ends_with('…'));
        assert!(line.chars().count() <= LINE + 1);
    }

    #[test]
    fn a_binary_file_is_skipped() {
        let mut bytes = b"needle is in here".to_vec();
        bytes.push(0);

        assert!(binary(&bytes));
        assert_eq!(holds(&bytes, "needle"), None);
    }

    #[test]
    fn only_files_that_could_hold_a_line_are_opened() {
        for yes in ["a.txt", "a.RS", "notes.md", "Makefile", ".gitignore", "a.json"] {
            assert!(looks_textual(yes), "{yes} should be read");
        }

        for no in ["a.zip", "a.mp4", "a.png", "a.exe", "a.pdf", "a.docx"] {
            assert!(!looks_textual(no), "{no} should not be opened");
        }
    }

    #[test]
    fn grep_stops_at_its_file_bound() {
        let opened = std::cell::Cell::new(0);
        let read = |_: &Path, _: u64| {
            opened.set(opened.get() + 1);
            Some(b"nothing".to_vec())
        };

        let bounds = Bounds {
            files: 5,
            ..Bounds::default()
        };
        matching(&paths(50), "needle", bounds, read, always);

        assert_eq!(opened.get(), 5);
    }

    #[test]
    fn grep_stops_at_its_deadline() {
        let opened = std::cell::Cell::new(0);
        let read = |_: &Path, _: u64| {
            opened.set(opened.get() + 1);
            std::thread::sleep(Duration::from_millis(5));
            Some(b"nothing".to_vec())
        };

        let bounds = Bounds {
            budget: Duration::from_millis(12),
            ..Bounds::default()
        };
        matching(&paths(100), "needle", bounds, read, always);

        // A handful rather than a hundred. The exact number is the machine's
        // business; that it stopped early is not.
        assert!(opened.get() < 20, "opened {} files", opened.get());
    }

    #[test]
    fn grep_stops_when_the_search_is_no_longer_current() {
        let opened = std::cell::Cell::new(0);
        let read = |_: &Path, _: u64| {
            opened.set(opened.get() + 1);
            Some(b"nothing".to_vec())
        };

        // Overtaken after the third file.
        let still = || opened.get() < 3;
        matching(&paths(50), "needle", Bounds::default(), read, still);

        assert_eq!(opened.get(), 3);
    }

    #[test]
    fn a_file_that_will_not_open_is_passed_over_rather_than_failing_the_search() {
        let read = |path: &Path, _: u64| {
            let name = path.to_string_lossy().into_owned();
            name.contains("file1").then(|| b"the needle".to_vec())
        };

        let found = matching(&paths(3), "needle", Bounds::default(), read, always);

        assert_eq!(found.len(), 1);
        assert!(found[0].path.contains("file1"));
        assert_eq!(found[0].line, "the needle");
    }

    #[test]
    fn a_file_whose_name_says_it_is_not_text_is_never_opened() {
        let opened = std::cell::Cell::new(0);
        let read = |_: &Path, _: u64| {
            opened.set(opened.get() + 1);
            Some(b"needle".to_vec())
        };

        let mixed = vec![
            r"C:\a\one.zip".to_string(),
            r"C:\a\two.txt".to_string(),
            r"C:\a\three.mp4".to_string(),
        ];
        let found = matching(&mixed, "needle", Bounds::default(), read, always);

        assert_eq!(opened.get(), 1, "only the text file should be opened");
        assert_eq!(found.len(), 1);
    }
}
