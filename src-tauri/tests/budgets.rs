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
        manifest: None,
        toggle: None,
    }
}

/// A corpus the size of a real index, with names that look like real names.
fn corpus(n: usize) -> Vec<CommandRecord> {
    const WORDS: [&str; 16] = [
        "Visual",
        "Studio",
        "Code",
        "Chrome",
        "Terminal",
        "Settings",
        "Manager",
        "Editor",
        "Player",
        "Viewer",
        "Control",
        "Panel",
        "Network",
        "Display",
        "Recovery",
        "Diagnostics",
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
        // Nothing pinned. Added when `search_excluding` grew the parameter,
        // and this file did not compile from then until it was found.
        &[],
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
/// 91 ms there against the 60 ms debug budget. So the budget is multiplied
/// where the machine is unknown, the measurement is printed on every run
/// whether it passes or not, and what the looser number catches is a change in
/// *kind*, which is what this budget is for. Something that grew a clone per
/// candidate goes to seconds and still fails. `CI` is set by GitHub Actions
/// for exactly this purpose.
///
/// **This is not about how many cores the machine has.** Ranking is a single
/// sequential pass and takes no thread pool, so it cannot go faster on a large
/// machine or slower on a small one. Measured on the release build by pinning
/// the process, best of five over 1,500 entries for `"visual"`: **4117 us on
/// one core, 4204 on two, 3621 on four, 4710 on all sixteen.** The whole
/// spread is noise, and the sixteen-core run is the slowest of them. What a
/// build agent has less of is single-core speed and exclusive use of it.
fn per_keystroke_us() -> u128 {
    /*
     * The debug number has headroom because a full `cargo test` is itself a
     * busy machine.
     *
     * Cargo runs test binaries in parallel, so these time themselves while
     * everything else in the suite is competing for the same cores. Measured
     * both ways on the same machine: the store browse takes 23 to 30 ms when
     * run alone and 61.5 ms during a full run, which failed a 60 ms budget
     * while nothing about the code had changed.
     *
     * The note at the top of this file already says a budget tight enough to
     * fail on a busy machine is one somebody switches off. This is that,
     * applied to the number rather than only written down. What the budget is
     * for is a change of *kind*: something that grew a clone per candidate
     * goes to seconds and still fails this by an order of magnitude. Release
     * stays tight, because release is the build the claim is about and it is
     * the one measured in `docs/budgets.md`.
     */
    let measured_here = if cfg!(debug_assertions) {
        150_000
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

        // Said on every run rather than only when it breaks. A budget that
        // speaks only on failure cannot show which way the number is moving,
        // and this one is close enough to a real limit to be worth watching
        // before it is crossed. Run with `-- --nocapture` to see them.
        println!(
            "  rank {query:>14?} -> {took:>6} us  (budget {})",
            per_keystroke_us()
        );

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

// --------------------------------------------------------------- the store

/// What opening the store and typing in it costs.
///
/// The store holds a catalogue of a different order to the launcher's index:
/// 2,183 listings, each with a title, a description, an author, categories,
/// platforms, a commit and a list of commands. Ranking it happens on every
/// keystroke exactly as the root list does, so it answers to the same budget.
///
/// The reason this has its own section is that the store has a second cost the
/// root list does not: **the catalogue arrives as JSON on disk and has to be
/// read before anything can be ranked.** That is the cost somebody feels as
/// "it loads from scratch every time", and a budget on ranking alone would
/// never see it.
mod the_store {
    use super::*;

    use sill_lib::store::{self, catalog::Catalog, ListedCommand, Listing, Query};

    /// A catalogue the size of the real one, with strings the length of real
    /// ones.
    ///
    /// The lengths matter more than the words. What is being measured is
    /// allocation and copying, and a corpus of `"a"` would make every clone
    /// look free.
    fn catalogue(n: usize) -> Catalog {
        const CATEGORIES: [&str; 6] = [
            "Developer Tools",
            "Productivity",
            "Media",
            "Web",
            "System",
            "AI Extensions",
        ];

        let listings = (0..n)
            .map(|i| Listing {
                name: format!("extension-number-{i}"),
                folder: format!("extensions/extension-number-{i}"),
                title: format!("Extension Number {i}"),
                description: "Does a useful thing without opening a browser, and says so at about \
                     the length a real store listing says it at."
                    .to_string(),
                author: format!("author{}", i % 400),
                categories: vec![CATEGORIES[i % CATEGORIES.len()].to_string()],
                platforms: vec!["macOS".to_string(), "Windows".to_string()],
                revision: "6939fc298cd701b66a652b5bcc6d1c763252391e".to_string(),
                downloads: (i as u64) * 13,
                icon: format!("https://files.raycast.com/kk4xwj4wh7m4sko2t1ui{i:04}"),
                commands: (0..3)
                    .map(|c| ListedCommand {
                        name: format!("command-{c}"),
                        title: format!("Command {c}"),
                        description: "Copies the result to the clipboard.".to_string(),
                        mode: "view".to_string(),
                    })
                    .collect(),
            })
            .collect();

        Catalog {
            format: 2,
            fetched_at: NOW,
            listings,
        }
    }

    fn browse_us(catalog: &Catalog, text: &str) -> u128 {
        let query = Query {
            text: text.to_string(),
            hide_blocked: true,
            ..Default::default()
        };

        let at = Instant::now();
        let out = store::browse(&catalog.listings, |_| None, &query, NOW);
        std::hint::black_box(&out);
        at.elapsed().as_micros()
    }

    /// The real size, measured against the live index on 2026-09-01.
    const REAL: usize = 2_183;

    /// Typing in the store answers to the same budget as typing in the root
    /// list, because it is the same act.
    #[test]
    fn typing_in_the_store_stays_within_one_keystroke() {
        let catalog = catalogue(REAL);

        for query in ["e", "ex", "ext", "extension", "extension number 9", ""] {
            browse_us(&catalog, query);
            let best = (0..3)
                .map(|_| browse_us(&catalog, query))
                .min()
                .unwrap_or(0);

            println!(
                "  browse {query:>20?} -> {best:>6} us  (budget {})",
                per_keystroke_us()
            );

            assert!(
                best < per_keystroke_us(),
                "{query:?} took {best} us against a budget of {}",
                per_keystroke_us()
            );
        }
    }

    /// **The one that catches a clone per keystroke.**
    ///
    /// The window asks Rust for a screen on every keystroke, and the catalogue
    /// it ranks is held in a service. If getting hold of it copies it, every
    /// keystroke deep-copies 2,183 listings and roughly fifty thousand strings,
    /// which is the shape that turns a browse from microseconds into tens of
    /// milliseconds and reads as the store being slow.
    ///
    /// Reaching for the held catalogue must be a pointer copy. This measures a
    /// real deep clone next to it and asserts the gap, so the day somebody
    /// takes the `Arc` off, this says so rather than the store merely getting
    /// worse.
    #[test]
    fn reaching_for_the_catalogue_does_not_copy_it() {
        let catalog = std::sync::Arc::new(catalogue(REAL));

        let at = Instant::now();
        for _ in 0..1_000 {
            std::hint::black_box(std::sync::Arc::clone(&catalog));
        }
        let shared = at.elapsed().as_micros();

        let at = Instant::now();
        let copied = std::hint::black_box((*catalog).clone());
        let deep = at.elapsed().as_micros();
        std::hint::black_box(copied);

        println!("  1000 shared handles -> {shared} us, one deep copy -> {deep} us");

        assert!(
            shared < deep,
            "a thousand shared handles cost {shared} us and one deep copy cost {deep} us, \
             so the catalogue is being copied rather than shared"
        );
    }

    /// Reading the catalogue off disk, which is what happens when the store is
    /// opened and nothing is held.
    ///
    /// Not held to the keystroke budget: it happens once per open, not once
    /// per letter. It is measured because it is the cost somebody feels as the
    /// store loading from scratch, and because it is the number that decides
    /// whether holding the catalogue between opens is worth doing at all.
    #[test]
    fn reading_the_catalogue_off_disk_is_reported() {
        let catalog = catalogue(REAL);
        let json = serde_json::to_string(&catalog).expect("serialises");

        println!("  catalogue on disk: {} KB", json.len() / 1024);

        let at = Instant::now();
        let parsed: Catalog = serde_json::from_str(&json).expect("parses");
        let took = at.elapsed().as_micros();
        std::hint::black_box(&parsed);

        println!("  parsing it        -> {took:>6} us");

        // Loose on purpose. This is a report, and the assertion only catches a
        // change in kind: a parse that starts taking a second is a parse
        // somebody notices every time they open the store.
        assert!(took < 2_000_000, "parsing the catalogue took {took} us");
    }
}
