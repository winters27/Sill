//! Against the Steam library that is actually on this machine.
//!
//! The fixtures in `games.rs` agree with the parser by construction. They
//! cannot say whether a real `libraryfolders.vdf`, written by a real Steam
//! install, reads at all, and they cannot find the thing a fixture author
//! never thought of. The redistributables filter exists because this probe
//! found `Steamworks Common Redistributables` sitting in the library looking
//! exactly like a game.
//!
//! Ignored, because a build agent has no Steam:
//!
//! ```text
//! cargo test --lib real_games -- --ignored --nocapture
//! ```

#[test]
#[ignore]
#[cfg(windows)]
fn the_steam_library_on_this_machine_reads() {
    let Some(root) = crate::games::steam_root() else {
        println!("Steam is not installed here");
        return;
    };

    println!("steam: {}", root.display());

    let found = crate::games::scan();

    // The claim: a real library yields games rather than nothing, which is
    // what happens when the escaping or the depth rule is wrong.
    assert!(
        !found.is_empty(),
        "the real Steam library parsed to no games at all"
    );

    for game in &found {
        println!(
            "  {:<40} {}  {}",
            game.name,
            game.path,
            game.icon_source.as_deref().unwrap_or("(no cached icon)")
        );

        // Every row has to survive the round trip to a launch, because a row
        // that cannot be launched is worse than a row that is not there.
        let (exe, args) = crate::games::command(
            &game.path,
            crate::games::steam_root().as_deref(),
            crate::games::epic_launcher().as_deref(),
        )
        .unwrap_or_else(|err| panic!("{} could not be turned into a command: {err}", game.name));

        assert!(
            exe.is_file(),
            "{} launches nothing: {}",
            game.name,
            exe.display()
        );
        assert_eq!(args.len(), 2, "{} took the wrong argument list", game.name);
    }

    // The one entry every Steam install has and nobody launches.
    assert!(
        !found
            .iter()
            .any(|one| one.name == "Steamworks Common Redistributables"),
        "the redistributables bundle was listed as a game"
    );
}

/// What the five existing sources see of the same games.
///
/// The reason this whole source exists, and the thing worth re-reading if
/// somebody ever proposes dropping it: the answer on this machine was none of
/// them.
#[test]
#[ignore]
#[cfg(windows)]
fn the_existing_sources_cannot_see_these_games() {
    let games = crate::games::scan();

    if games.is_empty() {
        println!("no games installed here");
        return;
    }

    let everything_else: Vec<String> = crate::apps::scan_shortcuts()
        .into_iter()
        .chain(crate::apps::scan_apps_folder())
        .map(|one| one.name.to_lowercase())
        .collect();

    let missed: Vec<&str> = games
        .iter()
        .map(|one| one.name.as_str())
        .filter(|name| !everything_else.contains(&name.to_lowercase()))
        .collect();

    println!(
        "{} of {} games are reachable by no other source: {:?}",
        missed.len(),
        games.len(),
        missed
    );
}
