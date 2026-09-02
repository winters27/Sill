//! Ranks a real index, for comparing this ranker against other ones.
//!
//! Ignored by default and driven by an environment variable, because it reads
//! a live index cache: one machine's worth of installed software rather than a
//! fixture, so it has nothing to assert and everything to show.
//!
//! ```text
//! INDEX="$APPDATA/app.winters.sill/index-cache.json" //!   cargo test --manifest-path src-tauri/Cargo.toml //!   --test compare_ranking -- --ignored --nocapture
//! ```
//!
//! Written while answering "should the ranker work the way fzf's does". The
//! answer needed both rankers over the same fifteen hundred entries, and
//! reading two implementations was not going to produce it.
use sill_lib::registry::{self, Aliases, CommandRecord, Excluded, Frecency};

#[test]
#[ignore]
fn rank_the_real_index() {
    let path = std::env::var("INDEX").unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let records: Vec<CommandRecord> = serde_json::from_str(&raw).unwrap();
    eprintln!("index: {} entries", records.len());

    for query in [
        "tada", "note", "term", "disc", "steam", "strm", "sett", "clip",
    ] {
        let found = registry::search_excluding(
            records.iter(),
            query,
            &Frecency::default(),
            &Aliases::default(),
            1_756_000_000,
            registry::SEARCH_LIMIT,
            Excluded::none(),
        );
        eprintln!("\n{query:?}: {} match at all", found.len());
        for r in found.iter().take(5) {
            let class = registry::match_class(query, &r.command);
            eprintln!(
                "   {:>18?}  {}  [{}]",
                class.unwrap(),
                r.command.title,
                r.command.mode
            );
        }
    }
}
