//! Where a Windows switch lands for the words people use to reach it.
//!
//! Ignored by default: it ranks this machine's real index, which is one
//! person's installed software rather than a fixture. It has nothing to assert
//! and everything to show.
//!
//! ```text
//! cargo test --manifest-path src-tauri/Cargo.toml \
//!   --test probe_switch_rank -- --ignored --nocapture
//! ```
use sill_lib::registry::{self, Aliases, CommandRecord, Excluded, Frecency};

const NOW: i64 = 1_756_000_000;

#[test]
#[ignore = "ranks this machine's real index"]
fn where_the_switches_land() {
    let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
        .join("app.winters.sill")
        .join("index-cache.json");
    let raw = std::fs::read_to_string(&path).expect("an index to rank");
    let cached: Vec<CommandRecord> = serde_json::from_str(&raw).expect("an index");

    // Through `one_per_id`, the way the running app loads it. Without that
    // every builtin appears twice, because the cache holds them too, and a
    // reading of this probe says every switch is duplicated when it is not.
    let mut records = registry::builtins();
    records.extend(cached);
    let records = registry::one_per_id(records);
    eprintln!("{} entries\n", records.len());

    for query in [
        "wifi", "wi-fi", "bluetooth", "mute", "dark mode", "volume",
        "audio output", "speakers", "lock",
    ] {
        let found = registry::search_excluding(
            records.iter(),
            query,
            &Frecency::default(),
            &Aliases::default(),
            NOW,
            registry::SEARCH_LIMIT,
            Excluded::none(),
        );

        let at = found
            .iter()
            .position(|r| r.command.mode == "system")
            .map(|at| format!("#{}", at + 1))
            .unwrap_or_else(|| "NOWHERE".to_string());

        eprintln!("{query:?}  switch lands {at} of {}", found.len());

        for (n, r) in found.iter().take(6).enumerate() {
            let class = registry::match_class(query, &r.command);
            eprintln!(
                "   {}{}. {:>18}  {:<34} [{}]",
                if r.command.mode == "system" { ">" } else { " " },
                n + 1,
                format!("{:?}", class),
                r.command.title,
                r.command.mode,
            );
        }
        eprintln!();
    }
}

/// What ordinary queries return, to see whether the phrase rule adds noise.
///
/// Matching a phrase across fields makes multi-word queries find more, which
/// is the point, and the risk is that it makes them find rubbish. This shows
/// what actually comes back so the judgement is made on results rather than on
/// how the rule sounds.
#[test]
#[ignore = "ranks this machine's real index"]
fn what_ordinary_queries_return() {
    let path = std::path::PathBuf::from(std::env::var("APPDATA").unwrap())
        .join("app.winters.sill")
        .join("index-cache.json");
    let raw = std::fs::read_to_string(&path).expect("an index to rank");
    let mut records = registry::builtins();
    records.extend(serde_json::from_str::<Vec<CommandRecord>>(&raw).expect("an index"));
    let records = registry::one_per_id(records);

    for query in [
        "visual studio", "task manager", "control panel", "device manager",
        "open settings", "the file", "new folder", "sound",
    ] {
        let found = registry::search_excluding(
            records.iter(),
            query,
            &Frecency::default(),
            &Aliases::default(),
            NOW,
            registry::SEARCH_LIMIT,
            Excluded::none(),
        );

        eprintln!("{query:?}  {} match", found.len());
        for r in found.iter().take(4) {
            eprintln!(
                "     {:>18}  {:<38} [{}]",
                format!("{:?}", registry::match_class(query, &r.command)),
                r.command.title,
                r.command.mode,
            );
        }
        eprintln!();
    }
}
