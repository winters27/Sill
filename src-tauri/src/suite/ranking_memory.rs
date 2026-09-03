//! What the ranker remembers, and what the empty list looks like.
//!
//! Two guarantees that had nothing holding them. `Frecency.entries` grew
//! forever, in a file written on every launch and parsed at every start; and
//! the opening list, with nothing typed, was ordered by title length, which is
//! a rule that reads as arbitrary because it is.

use std::collections::HashSet;

use crate::registry::{self, Aliases, CommandRecord, Excluded, Frecency};

const NOW: i64 = 1_756_000_000;
const DAY: i64 = 60 * 60 * 24;

fn command(id: &str, title: &str) -> CommandRecord {
    CommandRecord {
        id: id.to_string(),
        extension: "app".into(),
        extension_title: "Application".into(),
        command: "run".into(),
        title: title.to_string(),
        subtitle: String::new(),
        description: String::new(),
        mode: "app".into(),
        entrypoint: format!("C:/programs/{id}.exe"),
        keywords: Vec::new(),
        icon: None,
        panel: None,
        preferences: serde_json::Value::Null,
        manifest: None,
        toggle: None,
    }
}

fn titles(query: &str, records: &[CommandRecord], ranking: &Frecency) -> Vec<String> {
    registry::search_excluding(
        records.iter(),
        query,
        ranking,
        &Aliases::default(),
        NOW,
        registry::SEARCH_LIMIT,
        Excluded::none(),
        // Nothing pinned: this is the plain ranking, not a root list somebody
        // has arranged. Added when `search_excluding` grew the parameter, and
        // **this file did not compile for however long that took**: it was one
        // of forty-five separate binaries, ordinary work runs `--lib`, and
        // nothing that a person runs ever built it.
        &[],
    )
    .into_iter()
    .map(|found| found.command.title)
    .collect()
}

/// With nothing typed, the list is what you reach for and then the alphabet.
///
/// It used to be what you reach for and then the *shortest name*, because the
/// length tiebreak applied whether or not there was a query to justify it.
/// Length says "the query covers more of this title"; with no query it says
/// nothing, and the opening list read Ai, Cmd, Edge, Gmail down to the longest
/// name on the machine.
#[test]
fn the_opening_list_is_alphabetical_below_what_you_use() {
    let records = vec![
        command("a", "Zed"),
        command("b", "Ai"),
        command("c", "Photoshop"),
        command("d", "Cmd"),
        command("e", "Blender"),
    ];

    let mut ranking = Frecency::default();
    ranking.record("c", NOW);
    ranking.record("c", NOW);

    let order = titles("", &records, &ranking);

    assert_eq!(
        order,
        vec!["Photoshop", "Ai", "Blender", "Cmd", "Zed"],
        "the opening list is not frecency then alphabetical"
    );
}

/// With something typed, the shorter title still wins.
///
/// The tiebreak is not gone, it is conditional. Two titles that match a query
/// equally well are not equally good answers: the one the query covers more of
/// is the better guess, and that is the whole reason the rule exists.
#[test]
fn a_typed_query_still_prefers_the_title_it_covers_more_of() {
    let records = vec![
        command("a", "Note Taking Application"),
        command("b", "Notes"),
    ];

    let order = titles("note", &records, &Frecency::default());

    assert_eq!(
        order.first().map(String::as_str),
        Some("Notes"),
        "the shorter of two equal matches stopped winning"
    );
}

/// Something opened once, three months ago, is forgotten.
///
/// `entries` was the one map with no limit: every application, file, setting
/// and window ever opened stayed for good, most of which no longer exist.
#[test]
fn a_single_launch_from_months_ago_is_forgotten() {
    let mut ranking = Frecency::default();

    ranking.record("opened-once-in-may", NOW - 120 * DAY);
    assert_eq!(ranking.count("opened-once-in-may"), 1);

    // Any later launch is what prompts the sweep, the same way recording a
    // query is what trims the learned map.
    ranking.record("something-else", NOW);

    assert_eq!(
        ranking.count("opened-once-in-may"),
        0,
        "a one-off from four months ago is still remembered"
    );
}

/// Something opened twice is a habit, and habits do not fade.
///
/// Twice is the same threshold `LEARNED_AT` uses to call a query deliberate.
/// A tool used every March would otherwise be forgotten each summer.
#[test]
fn something_used_twice_survives_however_old() {
    let mut ranking = Frecency::default();

    ranking.record("tax-return", NOW - 300 * DAY);
    ranking.record("tax-return", NOW - 299 * DAY);

    ranking.record("something-else", NOW);

    assert_eq!(
        ranking.count("tax-return"),
        2,
        "a habit was forgotten for being old"
    );
}

/// The thing just launched is never the thing forgotten.
///
/// The same intermittent bug `forget_oldest_queries` carries a comment about:
/// times are whole seconds, so everything in the same second ties, and a tie
/// among `HashMap` keys is broken by hash order.
#[test]
fn the_entry_just_recorded_is_never_the_one_dropped() {
    let mut ranking = Frecency::default();

    for n in 0..2_100 {
        ranking.record(&format!("old-{n}"), NOW - 200 * DAY);
    }

    ranking.record("the-one-just-opened", NOW - 200 * DAY);

    assert_eq!(
        ranking.count("the-one-just-opened"),
        1,
        "the launch that triggered the sweep was swept"
    );
}

/// Past the cap, the oldest go and the newest stay.
#[test]
fn the_map_stays_bounded_and_keeps_the_newest() {
    let mut ranking = Frecency::default();

    // Twice each, so the age rule cannot be what does the work here.
    for n in 0..2_500 {
        let at = NOW - i64::from(2_500 - n);
        ranking.record(&format!("id-{n}"), at);
        ranking.record(&format!("id-{n}"), at);
    }

    assert!(
        ranking.len() <= 2_000,
        "the launch history grew to {} entries",
        ranking.len()
    );

    let kept: HashSet<u32> = (0..2_500)
        .filter(|n| ranking.count(&format!("id-{n}")) > 0)
        .collect();

    assert!(
        kept.contains(&2_499),
        "the most recently launched entry was dropped"
    );
    assert!(!kept.contains(&0), "the oldest entry survived a full sweep");
}
