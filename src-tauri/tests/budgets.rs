//! What the hot paths are allowed to cost.
//!
//! Numbers measured on a real machine, then given room. These are not
//! benchmarks and the exact figures are not the point: they catch a change in
//! *kind*, where something that ran in microseconds starts running in
//! milliseconds because it grew a clone or an allocation per candidate.
//!
//! Generous on purpose. A budget tight enough to fail on a busy machine is a
//! budget somebody switches off, and a switched-off budget catches nothing.
use std::time::Instant;

use sill_lib::registry::{self, Aliases, CommandRecord, Excluded, Frecency};

const NOW: i64 = 1_756_000_000;

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
        toggle: None,
    }
}

/// A corpus the size of a real index, with names that look like real names.
fn corpus(n: usize) -> Vec<CommandRecord> {
    const WORDS: [&str; 16] = [
        "Visual", "Studio", "Code", "Chrome", "Terminal", "Settings", "Manager",
        "Editor", "Player", "Viewer", "Control", "Panel", "Network", "Display",
        "Recovery", "Diagnostics",
    ];

    (0..n)
        .map(|i| {
            let title = format!(
                "{} {} {}",
                WORDS[i % WORDS.len()],
                WORDS[(i / 4) % WORDS.len()],
                WORDS[(i / 16) % WORDS.len()],
            );
            command(&format!("app:{i}"), &title)
        })
        .collect()
}

fn rank(corpus: &[CommandRecord], query: &str) -> u128 {
    let start = Instant::now();

    let found = registry::search_excluding(
        corpus.iter(),
        query,
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        registry::SEARCH_LIMIT,
        Excluded::none(),
    );

    // Used so the work cannot be optimised away.
    assert!(found.len() <= registry::SEARCH_LIMIT);
    start.elapsed().as_micros()
}

/// What one keystroke over a full index may cost.
///
/// Two numbers, because this runs in both builds and `npm run verify` uses the
/// debug one, which measured three times slower than release on the same
/// corpus. A single budget would either fail every verify or be too loose to
/// mean anything in release.
///
/// The corpus these are measured against is deliberately far worse than a real
/// index: every title is built from the same sixteen words, so a query matches
/// nearly everything and the ranker does the most work it can. A real index of
/// the same size answers in a fraction of this.
///
/// Both numbers were measured on a development machine with sixteen cores and
/// nothing else running. A shared build agent is neither: the same corpus took
/// 91 ms there against the 60 ms debug budget, on four cores it does not have
/// to itself. So the budget is multiplied where the machine is unknown, and
/// what it catches there is a change in *kind*, which is what this budget is
/// for. Something that grew a clone per candidate goes to seconds and still
/// fails. `CI` is set by GitHub Actions for exactly this purpose.
fn per_keystroke_us() -> u128 {
    let measured_here = if cfg!(debug_assertions) {
        60_000
    } else {
        20_000
    };

    if std::env::var_os("CI").is_some() {
        measured_here * 5
    } else {
        measured_here
    }
}

#[test]
fn ranking_a_whole_index_stays_within_one_keystroke() {
    let corpus = corpus(1_500);

    // Every prefix of a word somebody would type. The first keystroke is the
    // worst: one letter matches nearly everything.
    for query in ["v", "vi", "vis", "visu", "visual", "visual studio"] {
        let took = rank(&corpus, query);

        assert!(
            took < per_keystroke_us(),
            "{query:?} took {took} us against a budget of {}",
            per_keystroke_us()
        );
    }
}

#[test]
fn a_query_matching_nothing_is_not_the_expensive_case() {
    // Falling through every class to reach "no match" is the longest path
    // through the ranker, and it happens on every keystroke of a word that is
    // not there yet.
    let corpus = corpus(1_500);
    let took = rank(&corpus, "zzqx");

    assert!(took < per_keystroke_us(), "took {took} us");
}

#[test]
fn an_empty_query_does_not_cost_more_than_a_full_one() {
    // The root list. It ranks the whole corpus with no text to match against,
    // and it is drawn every time the launcher opens.
    let corpus = corpus(1_500);
    let took = rank(&corpus, "");

    assert!(took < per_keystroke_us(), "took {took} us");
}

#[test]
fn ranking_grows_with_the_corpus_and_not_faster() {
    // The shape that matters. Linear is fine; anything that starts comparing
    // candidates against each other is not, and would show up here long before
    // anybody noticed it while typing.
    let small = corpus(500);
    let large = corpus(4_000);

    // Warmed, because the first run through pays for page faults.
    rank(&small, "visual");
    rank(&large, "visual");

    let a = (0..5).map(|_| rank(&small, "visual")).min().unwrap_or(0);
    let b = (0..5).map(|_| rank(&large, "visual")).min().unwrap_or(0);

    // Eight times the corpus. Forty times the work would mean the cost is
    // growing faster than the input.
    assert!(
        b <= a.max(1) * 40,
        "500 entries took {a} us and 4,000 took {b}, which is not linear"
    );
}

/// What the hot paths actually cost right now.
///
/// Ignored, so it never fails a build. Run it to refresh the numbers in the
/// budgets note, which is where they are written down.
///
/// ```text
/// cargo test --release --manifest-path src-tauri/Cargo.toml \
///   --test budgets -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn measured() {
    for size in [500, 1_500, 4_000] {
        let corpus = corpus(size);

        for query in ["v", "visual", "zzqx", ""] {
            // Warmed, then the best of five: the slowest run measures the
            // machine's other work rather than this code.
            rank(&corpus, query);
            let best = (0..5).map(|_| rank(&corpus, query)).min().unwrap_or(0);

            println!("  {size:>5} entries, {query:>8?} -> {best:>5} us");
        }
    }
}
