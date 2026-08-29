//! What a snippet fills in when it expands.
//!
//! A snippet that can only paste fixed text is a slightly faster way to type
//! something you could have typed. The placeholders are what make one worth
//! keeping: a signature that carries today's date, a template that picks up
//! whatever is on the clipboard, a form that leaves the cursor in the gap.
//!
//! Expansion is a pure function over the text and a supplied context, so the
//! whole grammar is testable without a clock, a clipboard, or a keyboard.

use serde::Serialize;

/// Where the caret should end up, counted in characters from the start of the
/// expanded text.
pub const CURSOR: &str = "cursor";

/// Everything a snippet can ask about the world.
///
/// Passed in rather than read here so a test can pin the date and the caller
/// can decide how much it is willing to pay for: reading the clipboard costs
/// a Win32 round trip, and most snippets never mention it.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub clipboard: String,
    /// Formatted `YYYY-MM-DD`.
    pub date: String,
    /// Formatted `HH:MM`.
    pub time: String,
    pub uuid: String,
    /// Whatever was typed for this run.
    ///
    /// Empty for a snippet, which has nowhere to ask. A quicklink with
    /// `{query}` in it is the reason this exists: it is the difference
    /// between a bookmark and a search.
    pub query: String,
}

/// An expanded snippet, and where to leave the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expansion {
    pub text: String,
    /// Characters from the start, or `None` when the snippet said nothing.
    pub cursor: Option<usize>,
}

/// Fills in every placeholder in `template`.
///
/// An unknown placeholder is left exactly as written. Someone whose snippet
/// legitimately contains `{foo}` should get `{foo}`, and a typo in a
/// placeholder name should be visible rather than silently deleting itself.
pub fn expand(template: &str, context: &Context) -> Expansion {
    expand_with(template, context, &|value| value.to_string())
}

/// Fills in every placeholder, escaping each substituted value.
///
/// The escape applies to what a placeholder *produces*, never to the literal
/// text around it. That distinction is the whole point: a quicklink is a URL
/// whose fixed part contains slashes and question marks that must survive
/// untouched, and a typed query that must not.
pub fn expand_with(
    template: &str,
    context: &Context,
    escape: &dyn Fn(&str) -> String,
) -> Expansion {
    let mut out = String::with_capacity(template.len());
    let mut cursor = None;
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];

        let Some(close) = after.find('}') else {
            // An unclosed brace is just a brace.
            out.push_str(&rest[open..]);
            return Expansion { text: out, cursor };
        };

        let name = &after[..close];
        rest = &after[close + 1..];

        match name.trim().to_ascii_lowercase().as_str() {
            CURSOR => {
                // Only the first one counts. Two carets is not a thing a text
                // field can do, and silently honouring the last would move the
                // caret somewhere the author did not choose.
                if cursor.is_none() {
                    cursor = Some(out.chars().count());
                }
            }
            "clipboard" => out.push_str(&escape(&context.clipboard)),
            "date" => out.push_str(&escape(&context.date)),
            "time" => out.push_str(&escape(&context.time)),
            "uuid" => out.push_str(&escape(&context.uuid)),
            "query" => out.push_str(&escape(&context.query)),
            _ => {
                // Unknown: put it back verbatim.
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
    }

    out.push_str(rest);
    Expansion { text: out, cursor }
}

/// Whether `template` mentions the clipboard.
///
/// Asked before expanding so the clipboard is only read when a snippet
/// actually wants it. Every expansion paying a Win32 round trip for a
/// placeholder almost none of them use would be a poor trade.
pub fn needs_clipboard(template: &str) -> bool {
    mentions(template, "clipboard")
}

/// Whether `template` uses the named placeholder.
///
/// Matched the way `expand` matches, trimmed and case-insensitively, so
/// `{ Query }` is not reported as absent and then quietly filled in.
pub fn mentions(template: &str, name: &str) -> bool {
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return false;
        };
        if after[..close].trim().eq_ignore_ascii_case(name) {
            return true;
        }
        rest = &after[close + 1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> Context {
        Context {
            clipboard: "PASTED".into(),
            date: "2026-08-29".into(),
            time: "14:32".into(),
            uuid: "0189a1f2".into(),
            query: "rust traits".into(),
        }
    }

    #[test]
    fn an_escape_touches_the_values_and_not_the_template() {
        // The fixed part of a URL is full of characters that must survive:
        // escaping the whole result would destroy the link itself.
        let out = expand_with(
            "https://example.com/search?q={query}&t=1",
            &context(),
            &|v| v.replace(' ', "%20"),
        );
        assert_eq!(out.text, "https://example.com/search?q=rust%20traits&t=1");
    }

    #[test]
    fn mentions_matches_the_way_expansion_does() {
        assert!(mentions("a {query} b", "query"));
        assert!(mentions("a { Query } b", "query"));
        assert!(!mentions("a {queryish} b", "query"));
        assert!(!mentions("a {query b", "query"));
    }

    #[test]
    fn every_placeholder_is_filled() {
        let out = expand("{date} {time} {uuid} {clipboard}", &context());
        assert_eq!(out.text, "2026-08-29 14:32 0189a1f2 PASTED");
        assert_eq!(out.cursor, None);
    }

    #[test]
    fn the_caret_is_placed_and_leaves_no_text_behind() {
        let out = expand("Hi {cursor},\n\nBest", &context());
        assert_eq!(out.text, "Hi ,\n\nBest");
        assert_eq!(out.cursor, Some(3));
    }

    #[test]
    fn only_the_first_caret_counts() {
        // Two carets is not a thing a text field can do, and honouring the
        // last would move it somewhere the author did not choose.
        let out = expand("a{cursor}b{cursor}c", &context());
        assert_eq!(out.text, "abc");
        assert_eq!(out.cursor, Some(1));
    }

    #[test]
    fn the_caret_is_counted_in_characters_not_bytes() {
        // A byte offset would land mid-character and put the caret in the
        // wrong place for anyone whose snippet is not pure ASCII.
        let out = expand("héllo{cursor}!", &context());
        assert_eq!(out.cursor, Some(5));
    }

    #[test]
    fn an_unknown_placeholder_survives_verbatim() {
        // A typo should be visible rather than silently deleting itself, and
        // a snippet that legitimately contains braces should still work.
        let out = expand("{nope} and {klipboard}", &context());
        assert_eq!(out.text, "{nope} and {klipboard}");
    }

    #[test]
    fn braces_that_are_not_placeholders_are_left_alone() {
        let out = expand("fn main() { println!(\"hi\"); }", &context());
        assert_eq!(out.text, "fn main() { println!(\"hi\"); }");
    }

    #[test]
    fn an_unclosed_brace_is_just_a_brace() {
        let out = expand("100% of {", &context());
        assert_eq!(out.text, "100% of {");
    }

    #[test]
    fn placeholder_names_ignore_case_and_padding() {
        let out = expand("{ DATE } {Cursor}", &context());
        assert_eq!(out.text, "2026-08-29 ");
        assert_eq!(out.cursor, Some(11));
    }

    #[test]
    fn text_with_no_placeholders_is_returned_unchanged() {
        let out = expand("just some text", &context());
        assert_eq!(out.text, "just some text");
    }

    #[test]
    fn the_clipboard_is_only_read_when_it_is_asked_for() {
        // Every expansion paying a Win32 round trip for a placeholder almost
        // none of them use would be a poor trade.
        assert!(needs_clipboard("see {clipboard}"));
        assert!(needs_clipboard("{CLIPBOARD}"));
        assert!(!needs_clipboard("{date} only"));
        assert!(!needs_clipboard("the word clipboard on its own"));
    }
}
