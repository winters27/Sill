//! Turning a quicklink into the thing to open.
//!
//! One decision carries this module: **a value substituted into a URL has to
//! be percent-encoded, and the same value substituted into a file path must
//! not be.** Getting it backwards is the classic quicklink bug. Encode a path
//! and `C:\Users` becomes `C%3A%5CUsers`, which opens nothing; skip encoding
//! in a URL and a two-word search sends a raw space, which some servers
//! reject and others silently truncate at.
//!
//! So the target is inspected first, and only the parts a placeholder
//! produced are escaped. The literal text around them is never touched, which
//! is what keeps the `?` and `&` of a query string working.

use crate::snippets::placeholder::{self, Context};

/// Whether `link` is a URL rather than a path.
///
/// Matched on a scheme, `name:`, rather than on `://`: `mailto:` and
/// `ms-settings:` are URLs with no authority and would fail the simpler test.
/// A Windows drive letter is the one collision, and `C:` is excluded by
/// requiring at least two characters before the colon.
pub fn is_url(link: &str) -> bool {
    let Some(colon) = link.find(':') else {
        return false;
    };
    let scheme = &link[..colon];

    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Percent-encodes everything a URL does not leave alone.
///
/// The unreserved set from RFC 3986. Encoding more than strictly necessary is
/// safe in a query string and is the right default here, because the value is
/// arbitrary text somebody typed and its meaning must not depend on where in
/// the URL it lands.
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// The link with every placeholder filled in.
pub fn resolve(link: &str, context: &Context) -> String {
    if is_url(link) {
        placeholder::expand_with(link, context, &percent_encode).text
    } else {
        placeholder::expand(link, context).text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(query: &str) -> Context {
        Context {
            clipboard: "on the clipboard".into(),
            date: "2026-08-29".into(),
            time: "14:32".into(),
            uuid: "0189a1f2".into(),
            query: query.into(),
            ..Context::default()
        }
    }

    #[test]
    fn a_url_gets_its_query_encoded_and_keeps_its_own_punctuation() {
        let out = resolve(
            "https://www.google.com/search?q={query}&hl=en",
            &context("rust trait objects"),
        );
        // The `?`, `&` and `=` are the URL's own and must survive; the spaces
        // came from the person typing and must not.
        assert_eq!(
            out,
            "https://www.google.com/search?q=rust%20trait%20objects&hl=en"
        );
    }

    #[test]
    fn a_query_that_is_itself_a_url_survives_being_a_parameter() {
        // The classic failure: an unencoded `://` inside a parameter ends the
        // parameter early and the rest is read as more URL.
        let out = resolve(
            "https://example.com/save?url={query}",
            &context("https://a.test/x?y=1&z=2"),
        );
        assert!(out.ends_with("https%3A%2F%2Fa.test%2Fx%3Fy%3D1%26z%3D2"));
    }

    #[test]
    fn a_file_path_is_left_exactly_as_written() {
        // Encoding this would turn a working path into nonsense.
        let out = resolve(r"C:\Users\Brandon\{query}", &context("Notes 2026"));
        assert_eq!(out, r"C:\Users\Brandon\Notes 2026");
    }

    #[test]
    fn a_drive_letter_is_not_a_scheme() {
        // The one place the scheme test could go wrong, and the one that
        // would silently mangle every Windows path.
        assert!(!is_url(r"C:\Users\Brandon"));
        assert!(!is_url("D:/projects"));
        assert!(is_url("https://example.com"));
        assert!(is_url("mailto:someone@example.com"));
        assert!(is_url("ms-settings:display"));
    }

    #[test]
    fn a_link_with_no_placeholder_is_unchanged() {
        assert_eq!(
            resolve("https://news.ycombinator.com", &context("ignored")),
            "https://news.ycombinator.com"
        );
    }
}
