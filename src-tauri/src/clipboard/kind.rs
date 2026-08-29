//! What a copied thing actually is.
//!
//! A clipboard history that shows every entry as a line of grey text is a log,
//! not a tool. Knowing that one entry is a colour, another a link and another
//! a path is what lets the list show a swatch, a favicon-shaped row, or a
//! folder, and what lets a search be filtered down to just links.
//!
//! Classification is a pure function over the text so it can be tested
//! exhaustively without a clipboard.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Link,
    Email,
    Color,
    /// A path to something on disk that exists.
    File,
    Image,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Link => "link",
            Kind::Email => "email",
            Kind::Color => "color",
            Kind::File => "file",
            Kind::Image => "image",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "link" => Kind::Link,
            "email" => Kind::Email,
            "color" => Kind::Color,
            "file" => Kind::File,
            "image" => Kind::Image,
            _ => Kind::Text,
        }
    }
}

/// Classifies a copied string.
///
/// Only a single-line value can be anything other than text: a paragraph that
/// happens to begin with `http` is prose about a URL, not a URL.
pub fn classify(text: &str) -> Kind {
    let trimmed = text.trim();

    if trimmed.is_empty() || trimmed.contains(['\n', '\r']) {
        return Kind::Text;
    }

    if is_link(trimmed) {
        return Kind::Link;
    }
    if is_email(trimmed) {
        return Kind::Email;
    }
    if is_color(trimmed) {
        return Kind::Color;
    }
    if is_path(trimmed) {
        return Kind::File;
    }

    Kind::Text
}

fn is_link(text: &str) -> bool {
    // A scheme and something after it. Deliberately not a full URL grammar:
    // this decides which icon a row gets, and being wrong costs an icon.
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };

    !rest.is_empty()
        && !rest.contains(char::is_whitespace)
        && !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// One address, and only one.
///
/// Deliberately crude: this picks a row's icon, and the cost of a false
/// positive is the wrong glyph. A full RFC 5322 grammar would be a great deal
/// of code to decide that.
fn is_email(text: &str) -> bool {
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };

    !local.is_empty()
        && !domain.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !text.contains(char::is_whitespace)
}

/// A CSS colour worth showing a swatch for.
///
/// Hex, `rgb()` and `hsl()`, which between them are what a designer or a
/// stylesheet actually puts on the clipboard.
fn is_color(text: &str) -> bool {
    if let Some(digits) = text.strip_prefix('#') {
        return matches!(digits.len(), 3 | 4 | 6 | 8)
            && digits.chars().all(|c| c.is_ascii_hexdigit());
    }

    let lower = text.to_ascii_lowercase();
    for prefix in ["rgb(", "rgba(", "hsl(", "hsla("] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return rest.ends_with(')') && rest.len() > 1;
        }
    }

    false
}

/// A path to something that is on this machine right now.
///
/// Checked against the filesystem rather than by shape. A string that merely
/// looks like a path is prose about a path; the useful distinction for a
/// clipboard row is whether the thing can be opened.
fn is_path(text: &str) -> bool {
    // Bounded before touching the disk: this runs on everything copied, and
    // a stat call on a paragraph is wasted work.
    if text.len() > 260 || text.len() < 3 {
        return false;
    }

    let looks_absolute = text.starts_with(r"\\")
        || (text.as_bytes().get(1) == Some(&b':')
            && matches!(text.as_bytes().first(), Some(c) if c.is_ascii_alphabetic()));

    looks_absolute && std::path::Path::new(text).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_recognised_by_their_scheme() {
        assert_eq!(classify("https://example.com"), Kind::Link);
        assert_eq!(classify("http://localhost:1425/preview"), Kind::Link);
        assert_eq!(classify("ssh://git@github.com/x/y"), Kind::Link);
    }

    #[test]
    fn prose_about_a_link_is_prose() {
        // A paragraph beginning with a URL is not a URL.
        assert_eq!(classify("see https://example.com for more"), Kind::Text);
        assert_eq!(classify("https://example.com\nand another"), Kind::Text);
        assert_eq!(classify("https://"), Kind::Text);
    }

    #[test]
    fn an_address_is_an_address() {
        assert_eq!(classify("winters.brandon@pm.me"), Kind::Email);
        assert_eq!(classify("a@b.co"), Kind::Email);
    }

    #[test]
    fn a_handle_or_a_sentence_with_an_at_sign_is_not() {
        for value in ["@someone", "a@b", "two words@here.com", "@", "x@.com"] {
            assert_ne!(classify(value), Kind::Email, "{value}");
        }
    }

    #[test]
    fn a_mailto_link_is_a_link_not_an_address() {
        // It has a scheme, so it opens like any other link.
        assert_eq!(classify("mailto://someone@example.com"), Kind::Link);
    }

    #[test]
    fn colours_in_every_form_a_stylesheet_uses() {
        for value in [
            "#fff",
            "#FFFFFF",
            "#12345678",
            "rgb(1, 2, 3)",
            "hsla(0,0%,0%,.5)",
        ] {
            assert_eq!(classify(value), Kind::Color, "{value}");
        }
    }

    #[test]
    fn a_word_beginning_with_a_hash_is_not_a_colour() {
        // Tags and headings are copied far more often than colours.
        assert_eq!(classify("#todo"), Kind::Text);
        assert_eq!(classify("#12345"), Kind::Text, "five digits is no colour");
        assert_eq!(classify("# Heading"), Kind::Text);
    }

    #[test]
    fn a_path_counts_only_when_it_is_really_there() {
        // The useful question for a clipboard row is whether it can be
        // opened, not whether it is shaped like a path.
        assert_eq!(classify(r"C:\Windows"), Kind::File);
        assert_eq!(classify(r"C:\definitely\not\here\at\all"), Kind::Text);
        assert_eq!(classify("just some words"), Kind::Text);
    }

    #[test]
    fn a_long_string_is_never_stat_ed() {
        // This runs on everything copied; a paragraph must not hit the disk.
        let long = "C:\\".to_string() + &"a".repeat(400);
        assert_eq!(classify(&long), Kind::Text);
    }

    #[test]
    fn the_stored_name_round_trips() {
        // The kind is written to SQLite as a string and read back; a rename
        // on one side only would silently turn every row into text.
        for kind in [
            Kind::Text,
            Kind::Link,
            Kind::Email,
            Kind::Color,
            Kind::File,
            Kind::Image,
        ] {
            assert_eq!(Kind::from_str(kind.as_str()), kind);
        }
    }
}
