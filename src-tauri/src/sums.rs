//! Sums the calculator has answered, kept so they can be found again.
//!
//! An answer is copied and the launcher closes, and a minute later the
//! question is what the exchange rate was, or what that conversion came to.
//! So every answer somebody pressed Enter on is remembered, fifty at most,
//! and the word `sums` brings them back as rows that copy the way the answer
//! did.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. The file is not opened at startup and not opened on a keystroke:
//! [`asked`] is a comparison against three words, and [`matched`] takes the
//! reading as a closure, so a query that is not one of them never reads it.
//! An answer being copied is the only other time the file is touched, and
//! that is a keypress.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::json_store;

/// One answer somebody pressed Enter on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Past {
    /// The sum as it was typed.
    pub input: String,
    /// What it came to, which is what Enter copies again.
    pub text: String,
    /// When, in seconds since the epoch.
    pub at: i64,
}

/// How many are kept. Enough for a week of sums; nobody scrolls past that.
pub const KEPT: usize = 50;

/// How many are shown for one query.
const MOST_ROWS: usize = 12;

/// Only these words, and only first.
const ASKED_BY: &[&str] = &["sums", "calc", "calculator"];

/// How the file is kept. See `json_store` for what each part buys.
const SCHEMA: json_store::Schema = json_store::Schema {
    version: 1,
    shape: json_store::Shape::Around,
    layout: json_store::Layout::Readable,
    unreadable: json_store::Unreadable::KeepAside,
    what: "calculator history",
};

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("sums.json")
}

/// The remembered sums, read from disk the first time anybody wants them.
///
/// Managed state rather than a global, and lazy rather than loaded at
/// startup: the launcher opens whether or not anybody ever adds two numbers.
#[derive(Default)]
pub struct Sums {
    held: Mutex<Option<Vec<Past>>>,
}

impl Sums {
    /// Everything remembered, newest first.
    pub fn recall(&self, path: &Path) -> Vec<Past> {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        held.get_or_insert_with(|| json_store::load_list(path, &SCHEMA))
            .clone()
    }

    /// Remembers one, and writes the file.
    pub fn remember(&self, path: &Path, input: &str, text: &str, now: i64) -> std::io::Result<()> {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let list = held.get_or_insert_with(|| json_store::load_list(path, &SCHEMA));

        remember_in(list, input, text, now);
        json_store::save_atomic(path, list, &SCHEMA)
    }
}

/// Puts one sum at the front, taking any earlier copy of it out first.
///
/// The same sum asked twice is one row moved up, not two rows saying the
/// same thing. Sameness ignores case and spacing, because `2+2` and `2 + 2`
/// are one sum to the person who typed them.
pub fn remember_in(list: &mut Vec<Past>, input: &str, text: &str, now: i64) {
    let input = input.trim();
    if input.is_empty() {
        return;
    }

    let same = key_of(input);
    list.retain(|past| key_of(&past.input) != same);

    list.insert(
        0,
        Past {
            input: input.to_string(),
            text: text.to_string(),
            at: now,
        },
    );
    list.truncate(KEPT);
}

fn key_of(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// What the query asks for, if it asks for past sums at all.
///
/// `Some("")` is the whole list; `Some("usd")` is the ones mentioning it.
/// `None` is every other query, which is nearly all of them.
pub fn asked(query: &str) -> Option<&str> {
    let query = query.trim_start();
    let word = query.split_whitespace().next()?;

    if !ASKED_BY.contains(&word.to_ascii_lowercase().as_str()) {
        return None;
    }

    Some(query[word.len()..].trim())
}

/// The past sums a query asks for, reading them only if it does.
pub fn matched(query: &str, read: impl FnOnce() -> Vec<Past>) -> Vec<Past> {
    let Some(filter) = asked(query) else {
        return Vec::new();
    };

    let filter = filter.to_ascii_lowercase();
    read()
        .into_iter()
        .filter(|past| {
            filter.is_empty()
                || past.input.to_ascii_lowercase().contains(&filter)
                || past.text.to_ascii_lowercase().contains(&filter)
        })
        .take(MOST_ROWS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remembered(list: &[(&str, &str)]) -> Vec<Past> {
        let mut kept = Vec::new();
        for (at, (input, text)) in list.iter().enumerate() {
            remember_in(&mut kept, input, text, at as i64);
        }
        kept
    }

    #[test]
    fn fifty_is_the_most_kept() {
        let mut kept = Vec::new();
        for n in 0..(KEPT + 20) {
            remember_in(&mut kept, &format!("{n} + 1"), &format!("{}", n + 1), n as i64);
        }

        assert_eq!(kept.len(), KEPT);
        // Newest first, and the oldest twenty are the ones that went.
        assert_eq!(kept[0].input, format!("{} + 1", KEPT + 19));
        assert_eq!(kept[KEPT - 1].input, "20 + 1");
    }

    #[test]
    fn the_same_sum_is_moved_up_not_duplicated() {
        let kept = remembered(&[("2 + 2", "4"), ("3 * 3", "9"), ("2+2", "4")]);

        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].input, "2+2");
        assert_eq!(kept[1].input, "3 * 3");
    }

    #[test]
    fn an_empty_sum_is_not_remembered() {
        let mut kept = Vec::new();
        remember_in(&mut kept, "   ", "4", 0);
        assert!(kept.is_empty());
    }

    #[test]
    fn the_word_is_the_gate() {
        assert_eq!(asked("sums"), Some(""));
        assert_eq!(asked("Calc usd"), Some("usd"));
        assert_eq!(asked("  calculator  "), Some(""));

        for not in ["", "sum", "sumatra", "calcium", "my sums", "notepad"] {
            assert_eq!(asked(not), None, "{not:?} asked for past sums");
        }
    }

    #[test]
    fn a_word_after_it_narrows() {
        let read = || remembered(&[("100 usd to eur", "92 EUR"), ("2 + 2", "4")]);

        let all = matched("sums", read);
        assert_eq!(all.len(), 2);

        let some = matched("sums eur", read);
        assert_eq!(some.len(), 1);
        assert_eq!(some[0].input, "100 usd to eur");
    }

    /// The reading is a closure so a keystroke that is not the word never
    /// opens the file. Counted rather than assumed.
    #[test]
    fn nothing_is_read_unless_asked() {
        let reads = std::cell::Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            Vec::new()
        };

        assert!(matched("notepad", read).is_empty());
        assert!(matched("2 + 2", read).is_empty());
        assert_eq!(reads.get(), 0);

        matched("sums", read);
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn at_most_twelve_are_shown() {
        let read = || {
            (0..30)
                .map(|n| Past {
                    input: format!("{n} + 1"),
                    text: format!("{}", n + 1),
                    at: n,
                })
                .collect()
        };

        assert_eq!(matched("sums", read).len(), MOST_ROWS);
    }

    #[test]
    fn the_file_round_trips_and_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = path(dir.path());
        let sums = Sums::default();

        sums.remember(&path, "2 + 2", "4", 10).unwrap();
        sums.remember(&path, "sqrt(16)", "4", 11).unwrap();

        let again = Sums::default();
        let recalled = again.recall(&path);
        assert_eq!(recalled.len(), 2);
        assert_eq!(recalled[0].input, "sqrt(16)");
        assert_eq!(recalled[1].text, "4");
    }
}
