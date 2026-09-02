//! Looking something up on the web, from the launcher.
//!
//! The row every launcher has and Sill did not: type words that match nothing,
//! and the last thing offered is to search for them.
//!
//! It is deliberately thin. Sill already knows how to turn a template with
//! `{query}` in it into an address to open, because that is what a quicklink
//! is, and that module is where the one genuinely difficult decision lives:
//! **only the text a placeholder produced gets escaped, never the literal URL
//! around it.** Get that backwards and either the `?` and `&` of the query
//! string stop working, or a two-word search sends a raw space.
//!
//! So an engine here is a quicklink that ships with Sill, resolved by the same
//! code, and nothing about encoding is decided twice.

use serde::Serialize;

use crate::quicklinks::resolve;
use crate::snippets::placeholder::Context;

/// A search engine.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Engine {
    /// Stable across renames, because it is what settings store.
    pub id: &'static str,
    pub name: &'static str,
    /// The address, with `{query}` where the words go.
    pub url: &'static str,
}

/// The engines offered.
///
/// A short list on purpose. Anything else somebody wants is a quicklink with
/// `{query}` in it, which already works and already appears in results, so a
/// long menu here would be a second way to do a thing Sill can do.
pub const ENGINES: &[Engine] = &[
    Engine {
        id: "duckduckgo",
        name: "DuckDuckGo",
        url: "https://duckduckgo.com/?q={query}",
    },
    Engine {
        id: "google",
        name: "Google",
        url: "https://www.google.com/search?q={query}",
    },
    Engine {
        id: "bing",
        name: "Bing",
        url: "https://www.bing.com/search?q={query}",
    },
    Engine {
        id: "brave",
        name: "Brave",
        url: "https://search.brave.com/search?q={query}",
    },
    Engine {
        id: "startpage",
        name: "Startpage",
        url: "https://www.startpage.com/sp/search?query={query}",
    },
];

/// The engine settings name, or the default when it names one that is gone.
///
/// An id that no longer exists is not an error worth showing somebody. It
/// happens when settings outlive a build, and falling back to the default
/// quietly is better than a launcher that stops offering to search.
pub fn engine(id: &str) -> &'static Engine {
    ENGINES
        .iter()
        .find(|engine| engine.id == id)
        .unwrap_or(&ENGINES[0])
}

/// The address that searches for `query`.
///
/// `custom` wins when it holds anything, so somebody can point this at an
/// engine Sill has never heard of without waiting for a release. A custom
/// address with no `{query}` in it is still opened: it is then a link rather
/// than a search, which is odd but is what was asked for, and refusing it
/// would mean explaining the rule somewhere nobody is looking.
pub fn url_for(engine_id: &str, custom: &str, query: &str) -> String {
    let template = if custom.trim().is_empty() {
        engine(engine_id).url
    } else {
        custom.trim()
    };

    let context = Context {
        query: query.to_string(),
        ..Context::default()
    };

    resolve::resolve(template, &context)
}

/// What the row says.
///
/// The words are shown as typed rather than as they will be sent. Somebody
/// searching for `a + b` should not be shown `a+%2B+b`, which is the address
/// and not the question.
pub fn title(query: &str) -> String {
    format!("Search for {query}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_engine_is_the_first_one() {
        assert_eq!(engine("duckduckgo").id, ENGINES[0].id);
    }

    /// Settings outlive builds, and an engine that has been removed should not
    /// stop the launcher offering to search.
    #[test]
    fn an_engine_that_no_longer_exists_falls_back_rather_than_failing() {
        assert_eq!(engine("some-engine-from-2019").id, ENGINES[0].id);
        assert_eq!(engine("").id, ENGINES[0].id);
    }

    /// The whole reason this reuses the quicklink resolver.
    #[test]
    fn the_words_are_escaped_and_the_address_around_them_is_not() {
        let url = url_for("google", "", "rust & go");

        assert!(url.starts_with("https://www.google.com/search?q="), "{url}");
        // The literal `?` and `=` survive; the ampersand between the words does
        // not, or it would end the parameter early and the search would be for
        // "rust" alone.
        assert!(
            !url.ends_with("rust & go"),
            "the words were not escaped: {url}"
        );
        assert!(
            url.contains("%26"),
            "the ampersand was left to end the parameter: {url}"
        );
    }

    #[test]
    fn a_space_never_travels_as_a_space() {
        let url = url_for("duckduckgo", "", "two words");

        assert!(!url.contains(' '), "a raw space was sent: {url}");
    }

    #[test]
    fn a_custom_address_is_used_instead_of_the_named_engine() {
        let url = url_for("google", "https://example.com/find?text={query}", "cats");

        assert_eq!(url, "https://example.com/find?text=cats");
    }

    #[test]
    fn a_blank_custom_address_leaves_the_named_engine_alone() {
        let url = url_for("bing", "   ", "cats");

        assert!(url.starts_with("https://www.bing.com/search?q="), "{url}");
    }

    /// Shown as typed, not as sent.
    #[test]
    fn the_row_says_what_was_typed() {
        assert_eq!(title("rust & go"), "Search for rust & go");
    }

    #[test]
    fn every_engine_has_somewhere_to_put_the_words() {
        for engine in ENGINES {
            assert!(
                engine.url.contains("{query}"),
                "{} has nowhere to put the words",
                engine.name,
            );
        }
    }

    #[test]
    fn no_two_engines_share_an_id() {
        let mut seen = std::collections::HashSet::new();

        for engine in ENGINES {
            assert!(seen.insert(engine.id), "{} is listed twice", engine.id);
        }
    }
}

#[cfg(test)]
mod settings_reach_the_window {
    /// A setting the window cannot see is a setting that does not exist.
    ///
    /// Nested structs have bypassed the catch-all here before, so the shape is
    /// asserted rather than assumed: the window reads `webSearch.enabled` and
    /// gets nothing at all if the key is spelled differently or missing.
    #[test]
    fn web_search_is_in_the_json_the_window_reads() {
        let json = serde_json::to_value(crate::preferences::Preferences::default())
            .expect("preferences serialize");

        let section = json
            .get("webSearch")
            .unwrap_or_else(|| panic!("no webSearch key, only: {:?}", keys(&json)));

        assert_eq!(section.get("enabled").and_then(|v| v.as_bool()), Some(true));
        assert!(section.get("engine").is_some(), "no engine: {section:?}");
        assert!(
            section.get("customUrl").is_some(),
            "no customUrl: {section:?}"
        );
    }

    /// The screenshot settings, likewise. Nested structs have bypassed the
    /// catch-all here before, and a setting the window cannot see is a setting
    /// that does not exist.
    #[test]
    fn the_screenshot_settings_reach_the_window() {
        let json = serde_json::to_value(crate::preferences::Preferences::default())
            .expect("preferences serialize");

        let section = json
            .get("screenshot")
            .unwrap_or_else(|| panic!("no screenshot key, only: {:?}", keys(&json)));

        assert_eq!(section.get("after").and_then(|v| v.as_str()), Some("copy"));
        assert_eq!(
            section.get("clickAWindow").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(section.get("tool").is_some(), "no tool: {section:?}");
        assert_eq!(section.get("stepFrom").and_then(|v| v.as_u64()), Some(1));

        let hotkey = json.get("hotkey").expect("hotkey");
        assert!(
            hotkey.get("capture").is_some(),
            "no capture key: {hotkey:?}"
        );
        assert!(
            hotkey.get("captureScreen").is_some(),
            "no whole-screen capture key: {hotkey:?}",
        );
    }

    #[test]
    fn browser_search_is_too() {
        let json = serde_json::to_value(crate::preferences::Preferences::default())
            .expect("preferences serialize");

        let section = json
            .get("browsers")
            .unwrap_or_else(|| panic!("no browsers key, only: {:?}", keys(&json)));

        assert_eq!(
            section.get("enabled").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    fn keys(json: &serde_json::Value) -> Vec<String> {
        json.as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    }
}
