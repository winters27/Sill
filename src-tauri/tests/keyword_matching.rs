//! What a query is allowed to match outside a title.
//!
//! A snippet's whole body is one of its keywords, and keywords were matched by
//! an unbounded subsequence. A long enough string contains the letters of very
//! nearly anything in order, so every snippet matched every query and sat in
//! the results under whatever was actually being looked for.

use sill_lib::registry::{self, CommandRecord, MatchClass};

fn record(title: &str, keywords: &[&str]) -> CommandRecord {
    CommandRecord {
        id: title.to_lowercase(),
        extension: "snippets".into(),
        extension_title: "Snippet".into(),
        command: "expand".into(),
        title: title.to_string(),
        subtitle: String::new(),
        description: String::new(),
        mode: "snippet".into(),
        entrypoint: title.to_lowercase(),
        keywords: keywords.iter().map(|k| (*k).to_string()).collect(),
        icon: None,
        panel: None,
        preferences: serde_json::Value::Null,
        manifest: None,
        toggle: None,
    }
}

/// A real snippet body, of the length people actually save.
const BODY: &str = "Thanks for getting in touch. I have had a look at what you \
                    sent over and it all seems fine to me, so go ahead whenever \
                    you are ready. Let me know if anything else comes up and I \
                    will take another look at it.";

/// The bug, stated as the query that found it.
///
/// Every one of these letters appears in that paragraph in order, so the old
/// unbounded subsequence matched all of them. None of them is anything a
/// person searching for that snippet would type.
#[test]
fn a_long_snippet_body_no_longer_matches_anything_at_all() {
    let snippet = record("Reply", &["reply", BODY, "Email"]);

    for query in ["tada", "steam", "notepad", "figma", "chrome", "settings"] {
        assert_eq!(
            registry::match_class(query, &snippet),
            None,
            "{query:?} matched a snippet because its body contains those letters"
        );
    }
}

/// And the snippet is still findable by what is in it.
///
/// This is the reason the body is a keyword in the first place: somebody
/// remembers a phrase from the snippet rather than the name they gave it.
#[test]
fn a_phrase_from_the_body_still_finds_the_snippet() {
    let snippet = record("Reply", &["reply", BODY, "Email"]);

    for query in ["getting in touch", "go ahead", "another look"] {
        assert!(
            registry::match_class(query, &snippet).is_some(),
            "{query:?} is in the snippet and no longer finds it"
        );
    }
}

/// Initials of a keyword still work, which is how short queries reach things.
///
/// `vm` for "volume mixer" is a scattered match with a six character jump in
/// the middle, so a gap limit alone would have taken it away.
#[test]
fn the_initials_of_a_keyword_still_match() {
    let mixer = record("Sound Panel", &["volume mixer", "audio"]);

    assert_eq!(
        registry::match_class("vm", &mixer),
        Some(MatchClass::Elsewhere),
        "the initials of a keyword stopped matching"
    );
}

/// A keyword the query is simply inside still matches.
#[test]
fn a_keyword_containing_the_query_still_matches() {
    let mixer = record("Adjust Volume", &["volume mixer", "audio"]);

    assert_eq!(
        registry::match_class("mixer", &mixer),
        Some(MatchClass::Elsewhere)
    );
    assert_eq!(
        registry::match_class("audio", &mixer),
        Some(MatchClass::KeywordExact)
    );
}

/// A scattered match across a keyword is held to the title's discipline.
///
/// Letters found together count wherever they sit, which is why `mail` finds
/// the keyword "email". Letters found scattered have to start where a word
/// starts, which is the same rule the title tier applies and for the same
/// reason: the first character somebody types is the one carrying the intent.
#[test]
fn a_scattered_keyword_match_has_to_start_where_a_word_starts() {
    let mixer = record("Sound Panel", &["volume mixer"]);

    // Together, mid-word: a match.
    assert_eq!(
        registry::match_class("olum", &mixer),
        Some(MatchClass::Elsewhere)
    );

    // Scattered, starting mid-word: not a match.
    assert_eq!(
        registry::match_class("oum", &mixer),
        None,
        "a scattered match starting mid-word in a keyword is still accepted"
    );
}
