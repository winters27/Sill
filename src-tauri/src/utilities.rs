//! Small answers to things people type instead of searching for.
//!
//! A UUID, a hash, a piece of text turned inside out. Every launcher grows
//! these because they are the things somebody wants once an hour and opens a
//! website for, and a website that gets shown the text is a worse place to put
//! it than a launcher that never sends it anywhere.
//!
//! ## Why this is beside the calculator rather than inside it
//!
//! It answers the same shape of question, so it produces the same
//! [`calculator::Answer`] and lands in the same row: a thing you asked for,
//! above the things Sill merely found. But the *deciding* is different in
//! kind. The calculator has to guess whether a string is arithmetic at all,
//! and gets it wrong in both directions; this is asked by name. `sha256 hello`
//! is not ambiguous.
//!
//! ## The gate
//!
//! One word, then what to do it to. A bare keyword with nothing after it is
//! somebody searching for an application, and the one that needs no argument
//! has to be typed exactly, with nothing else on the line.
//!
//! This matters more than the transforms do. `json` is a word people search
//! for, and an answer row that pushed `package.json` down the list every time
//! would be a worse launcher, not a better one.

use crate::calculator::Answer;

/// What the transforms are called, and what each does.
///
/// A table rather than a match with a default, for the reason
/// `verify-source.mjs` now refuses a default in the window's own mode table: a
/// keyword nobody named should do nothing, not fall through to something.
const TRANSFORMS: &[(&str, fn(&str) -> Result<String, String>)] = &[
    ("upper", |text| Ok(crate::text::upper(text))),
    ("lower", |text| Ok(crate::text::lower(text))),
    ("title", |text| Ok(crate::text::title_case(text))),
    ("base64", |text| Ok(crate::text::base64_encode(text))),
    ("unbase64", crate::text::base64_decode),
    ("url", |text| Ok(crate::text::url_encode(text))),
    ("unurl", crate::text::url_decode),
    ("json", crate::text::json_pretty),
    ("minjson", crate::text::json_compact),
    ("sha256", |text| Ok(sha256_of(text))),
    ("sha1", |text| Ok(sha1_of(text))),
];

/// The ones that answer with nothing to work from.
const GENERATORS: &[(&str, fn() -> String)] = &[("uuid", new_uuid)];

/// An answer for what was typed, or nothing, which is the usual case.
pub fn evaluate(input: &str) -> Option<Answer> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lowered = trimmed.to_ascii_lowercase();

    /*
     * A generator has to be the whole line.
     *
     * `uuid` is a word somebody might have an application called, and one that
     * appears in file names. Requiring the line to be exactly that word is
     * what keeps this from displacing a real result: nobody types exactly
     * `uuid` and means "find me things about UUIDs" often enough to be worth
     * the other case.
     */
    for (name, make) in GENERATORS {
        if lowered == *name {
            return Some(Answer {
                text: make(),
                input: trimmed.to_string(),
            });
        }
    }

    // Otherwise: a keyword, a space, and something to do it to.
    let (keyword, rest) = trimmed.split_once(char::is_whitespace)?;
    let rest = rest.trim();

    if rest.is_empty() {
        return None;
    }

    let keyword = keyword.to_ascii_lowercase();
    let (_, transform) = TRANSFORMS.iter().find(|(name, _)| *name == keyword)?;

    /*
     * A transform that failed says nothing rather than saying so.
     *
     * `unbase64 hello` is not base64 and `json {` is not finished being typed.
     * Neither is an error the person made: they are both a query on its way to
     * being something, and a red row appearing halfway through typing is worse
     * than no row. What is genuinely broken shows when they stop typing and
     * the row still is not there.
     */
    let answer = transform(rest).ok()?;

    // An answer that repeats the question is not worth a row. `upper HELLO` is
    // the same rule the calculator applies to `2` answering `2`.
    if answer == rest {
        return None;
    }

    Some(Answer {
        text: answer,
        input: trimmed.to_string(),
    })
}

/// A random version 4 UUID, in the form everybody writes them.
fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn sha256_of(text: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex(&hasher.finalize())
}

/// SHA-1, which is offered because things still ask for it.
///
/// A git object name and an old API signature are both SHA-1, and somebody
/// checking one against another needs the same function that produced it.
/// Nothing here is a claim that it is a good hash to choose today.
fn sha1_of(text: &str) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(text.as_bytes());
    hex(&hasher.finalize())
}

/// Lowercase hex, which is what every tool that prints a digest prints.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    bytes.iter().fold(String::new(), |mut out, byte| {
        // Writing to a `String` cannot fail, and the alternative is an
        // allocation per byte.
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::evaluate;

    fn answer(input: &str) -> Option<String> {
        evaluate(input).map(|found| found.text)
    }

    #[test]
    fn a_uuid_is_asked_for_by_name() {
        let first = answer("uuid").expect("uuid answers");

        assert_eq!(first.len(), 36, "{first} is not the shape of a UUID");
        assert_eq!(first.matches('-').count(), 4);

        // Version 4, which is the one that is random rather than derived from
        // the machine or the time.
        assert_eq!(first.chars().nth(14), Some('4'));
    }

    #[test]
    fn two_uuids_are_not_the_same_uuid() {
        assert_ne!(answer("uuid"), answer("uuid"));
    }

    /// The half that matters: a launcher is typed at with words.
    ///
    /// `json` is a word people search for, and an answer row that pushed
    /// `package.json` down the list every time would be a worse launcher.
    #[test]
    fn a_bare_keyword_is_a_search_rather_than_a_request() {
        for typed in ["json", "sha256", "upper", "base64", "url"] {
            assert_eq!(answer(typed), None, "{typed:?} answered on its own");
        }
    }

    /// A keyword and a space is still somebody mid-way through typing.
    ///
    /// The moment after `upper ` and before the first letter of what to
    /// upper-case. There is nothing to transform yet, and an empty answer row
    /// appearing for one keystroke is a flicker rather than an answer.
    #[test]
    fn a_keyword_with_only_a_space_after_it_answers_nothing() {
        for typed in ["upper ", "sha256   ", "json 	"] {
            assert_eq!(answer(typed), None, "{typed:?} answered");
        }
    }

    /// And a word that merely starts with one is nothing to do with it.
    #[test]
    fn a_word_that_begins_with_a_keyword_is_left_alone() {
        for typed in ["uuidgen", "jsonlint", "urlencode", "sha256sum"] {
            assert_eq!(answer(typed), None, "{typed:?} answered");
        }
    }

    #[test]
    fn a_hash_is_the_hash_everything_else_prints() {
        // The digest of "abc", which is in the published test vectors for
        // both algorithms and so is checkable against something other than
        // this implementation.
        assert_eq!(
            answer("sha256 abc").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            answer("sha1 abc").as_deref(),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
    }

    #[test]
    fn the_transforms_are_the_ones_the_actions_already_use() {
        assert_eq!(answer("upper hello").as_deref(), Some("HELLO"));
        assert_eq!(answer("lower HELLO").as_deref(), Some("hello"));
        assert_eq!(answer("base64 hi").as_deref(), Some("aGk="));
        assert_eq!(answer("unbase64 aGk=").as_deref(), Some("hi"));
    }

    /// Something on its way to being valid says nothing, rather than saying it
    /// is wrong.
    ///
    /// A red row appearing halfway through typing is worse than no row.
    #[test]
    fn a_transform_that_cannot_run_yet_is_silent() {
        assert_eq!(answer("json {"), None);
        assert_eq!(answer("unbase64 not base64 at all"), None);
    }

    /// An answer that repeats the question is not worth a row.
    #[test]
    fn an_answer_that_is_the_question_is_not_offered() {
        assert_eq!(answer("upper HELLO"), None);
        assert_eq!(answer("lower hello"), None);
    }

    #[test]
    fn the_keyword_is_recognised_however_it_is_capitalised() {
        assert_eq!(answer("UPPER hi").as_deref(), Some("HI"));
        assert_eq!(
            answer("SHA256 abc").as_deref(),
            answer("sha256 abc").as_deref()
        );
    }
}
