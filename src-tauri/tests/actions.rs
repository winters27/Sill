//! The action registry's invariants.
//!
//! An integration test rather than a unit test, and not by preference: a lib
//! unit-test binary that retains the action vtables also retains the dialog
//! plugin's `TaskDialogIndirect`, which needs a common-controls v6 manifest
//! that only test targets can be given (see `build.rs`). The app binary
//! carries its own and cannot be given a second.
//!
//! Reproduced rather than believed, while everything else moved into the
//! library: with this file in `src/suite/` the library's test binary exits
//! 0xc0000139, `STATUS_ENTRYPOINT_NOT_FOUND`, before running a test. This is
//! the only file in the suite the constraint actually applies to.

use sill_lib::action::Capability;
use sill_lib::actions::builtins;
use sill_lib::object::ObjectKind;

/// Every mode `scripts/build-extension.mjs` or a scan can put in the index.
const INDEX_MODES: &[&str] = &[
    "app",
    "exe",
    "file",
    "folder",
    "view",
    "no-view",
    "setting",
    "sill-setting",
    "builtin",
    "snippet",
    "quicklink",
    "quicklink-arg",
    // Both of them, because whether a script stops to ask for an argument is a
    // fact about its header and not about what can be done to it. The one that
    // asks was the one nothing but the window could run.
    "script",
    "script-arg",
    "answer",
];

#[test]
fn everything_the_index_can_hold_still_has_something_bound_to_enter() {
    // The regression guard for the whole rewrite. Before this, pressing
    // Enter walked a chain of eleven string comparisons; now it is a
    // lookup, and the way a lookup fails is silently, on one kind, for
    // whoever happens to press Enter on it.
    let registry = builtins();

    for mode in INDEX_MODES {
        let kind = ObjectKind::from_mode(mode).expect("a known mode");
        assert!(
            registry.primary(kind).is_some(),
            "{mode} maps to {kind:?}, and nothing is bound to Enter for it"
        );
    }
}

/// A script is reachable by name, which is what a key and the model need.
///
/// A binding names an action id and resolves the thing from the index; the
/// model names an action id and a target. Neither has a window. So an action
/// under a stable id that accepts the kind is the whole of what both of them
/// require, and this is the half of that a test can hold: `ActionCtx` carries
/// a concrete `AppHandle`, so no action body can be run from here.
#[test]
fn running_a_script_is_reachable_by_its_id() {
    let registry = builtins();

    let found = registry
        .get("sill.script.run")
        .expect("a key and the model both bind by this id");

    assert!(found.accepts(ObjectKind::Script));
    assert!(
        found.is_primary(ObjectKind::Script),
        "Enter does not run it"
    );
    assert_eq!(
        found.capabilities(),
        &[Capability::ShellExecution],
        "the model's approval card is raised off this",
    );
}

#[test]
fn no_kind_has_two_actions_claiming_enter() {
    // `primary` returns the first match, so a second claimant does not
    // error, it just never runs. Which of the two you get would then
    // depend on registration order, which is not where that decision
    // should live.
    let registry = builtins();

    for kind in ObjectKind::ALL {
        let claimants: Vec<_> = registry
            .for_kind(*kind)
            .into_iter()
            .filter(|a| a.is_primary(*kind))
            .map(|a| a.id())
            .collect();

        assert!(
            claimants.len() <= 1,
            "{kind:?} has {} actions claiming Enter: {claimants:?}",
            claimants.len()
        );
    }
}

#[test]
fn the_primary_action_is_offered_first() {
    // The panel draws them in this order and Enter runs the first. If the
    // sort ever stops lifting the primary, the two disagree and the panel
    // quietly recommends the wrong thing.
    let registry = builtins();

    for kind in ObjectKind::ALL {
        let offered = registry.for_kind(*kind);
        let Some(first) = offered.first() else {
            continue;
        };
        let Some(primary) = registry.primary(*kind) else {
            continue;
        };
        assert_eq!(
            first.id(),
            primary.id(),
            "{kind:?} offers {} first but Enter runs {}",
            first.id(),
            primary.id()
        );
    }
}

#[test]
fn action_ids_are_unique() {
    // They are what a shortcut, a stored preference or a workflow step
    // refers to. Two actions sharing one means `get` returns whichever
    // was registered first, for ever.
    let registry = builtins();
    let mut ids = registry.ids();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "two actions share an id");
}

#[test]
fn every_action_declares_what_it_touches() {
    // An empty capability list would be a lie for all of these, and the
    // permission model that is coming reads exactly this.
    let registry = builtins();
    for kind in ObjectKind::ALL {
        for action in registry.for_kind(*kind) {
            assert!(
                !action.capabilities().is_empty(),
                "{} declares no capabilities",
                action.id()
            );
        }
    }
}

#[test]
fn a_result_that_only_exists_on_screen_offers_nothing_that_outlives_it() {
    // An answer has no path, no folder and no name worth keeping: it is a
    // number that stops existing when the query changes. Offering "Copy
    // Name" on it would copy the number twice under two labels.
    let registry = builtins();
    let offered: Vec<_> = registry
        .for_kind(ObjectKind::Answer)
        .into_iter()
        .map(|a| a.id())
        .collect();

    assert_eq!(offered, vec!["sill.copyAnswer"]);
}

#[test]
fn things_on_disk_can_have_their_path_copied_and_folder_opened() {
    // The point of `accepts` taking a kind rather than each action being
    // bolted to one: three kinds, one implementation.
    let registry = builtins();

    for kind in [
        ObjectKind::Application,
        ObjectKind::File,
        ObjectKind::Folder,
    ] {
        let offered: Vec<_> = registry
            .for_kind(kind)
            .into_iter()
            .map(|a| a.id())
            .collect();

        assert!(
            offered.contains(&"sill.copyPath"),
            "{kind:?} lost Copy Path"
        );
        assert!(
            offered.contains(&"sill.revealInFolder"),
            "{kind:?} lost Show in Folder"
        );
    }
}

#[test]
fn the_action_panel_has_something_to_show_for_every_result() {
    // Exactly what `actions_for` does for the window: mode to kind to a drawn
    // list. The panel used to be two entries written by hand in the frontend,
    // so this is the contract that replaced them.
    let registry = builtins();

    for mode in INDEX_MODES {
        let kind = ObjectKind::from_mode(mode).expect("a known mode");
        let drawn = registry.describe(kind, &Default::default());

        assert!(
            !drawn.is_empty(),
            "{mode} draws an empty action panel, which reads as the key being dead"
        );

        let primary: Vec<_> = drawn.iter().filter(|a| a.primary).map(|a| a.id).collect();
        assert_eq!(
            primary.len(),
            1,
            "{mode} should have exactly one action on Enter, has {primary:?}"
        );

        assert!(
            drawn[0].primary,
            "{mode} draws {} first but Enter runs {}",
            drawn[0].id, primary[0]
        );
    }
}

/// Kinds that never come from a scan: the window hands these over directly.
const LOOSE_KINDS: &[(&str, ObjectKind)] = &[
    ("clipboard", ObjectKind::ClipboardEntry),
    ("text", ObjectKind::Text),
];

#[test]
fn text_and_clipboard_rows_are_the_same_thing_to_a_transform() {
    // The reason transforms dispatch on a kind rather than on where the text
    // came from: a clipboard row and a selection are both just text, and
    // writing the operation twice is how the two versions drift apart.
    let registry = builtins();

    for (mode, kind) in LOOSE_KINDS {
        assert_eq!(
            ObjectKind::from_mode(mode),
            Some(*kind),
            "{mode} does not reach the window as a kind"
        );

        let offered: Vec<_> = registry
            .for_kind(*kind)
            .into_iter()
            .map(|a| a.id())
            .collect();

        for wanted in [
            "sill.text.upper",
            "sill.text.lower",
            "sill.text.base64Decode",
            "sill.text.jsonPretty",
        ] {
            assert!(
                offered.contains(&wanted),
                "{mode} is missing {wanted}: {offered:?}"
            );
        }
    }
}

#[test]
fn loose_text_has_exactly_one_action_on_enter() {
    // The same invariant the index kinds get. These reach the registry by a
    // different route and are just as capable of ending up with none or two.
    let registry = builtins();

    for (mode, kind) in LOOSE_KINDS {
        let claimants: Vec<_> = registry
            .for_kind(*kind)
            .into_iter()
            .filter(|a| a.is_primary(*kind))
            .map(|a| a.id())
            .collect();

        assert_eq!(
            claimants,
            vec!["sill.clipboard.copy"],
            "{mode} claimants wrong"
        );
        assert!(
            registry.describe(*kind, &Default::default())[0].primary,
            "{mode} draws a non-primary first"
        );
    }
}

#[test]
fn a_transform_is_never_offered_on_something_that_is_not_text() {
    // "Base64 Decode" on an installed application is nonsense, and an action
    // panel full of nonsense is how people stop opening the action panel.
    let registry = builtins();

    for kind in [
        ObjectKind::Application,
        ObjectKind::File,
        ObjectKind::ExtensionCommand,
        ObjectKind::Builtin,
    ] {
        let offered: Vec<_> = registry
            .for_kind(kind)
            .into_iter()
            .map(|a| a.id())
            .collect();
        assert!(
            !offered.iter().any(|id| id.starts_with("sill.text.")),
            "{kind:?} was offered a text transform: {offered:?}"
        );
    }
}

// ------------------------------------------------------------ file actions

#[test]
fn a_file_can_be_acted_on_and_not_only_opened() {
    // Files became findable before they became usable. A search result you can
    // only open is a worse file manager than the one already on the machine.
    let registry = sill_lib::actions::builtins();

    for kind in [ObjectKind::File, ObjectKind::Folder] {
        let offered: Vec<&str> = registry.for_kind(kind).iter().map(|a| a.id()).collect();

        for wanted in ["sill.copyPath", "sill.file.terminal", "sill.file.recycle"] {
            assert!(
                offered.contains(&wanted),
                "{kind:?} cannot {wanted}: {offered:?}"
            );
        }
    }
}

/**
Renaming and moving are reachable by name, like everything else.

**The point of `P1-09`, and the thing that was not true.** Both did their work
inside a Tauri command the window called, and the registry entries were stubs
that refused, so the two were the only actions on a file that nothing but the
page could run: `registry.get` found them and running one could only ever
answer with the sentence saying it had not been told enough.

That `get` answers is the whole of reachability. It is the one call a bound
key, `run_action` and the model's tool all make, and none of them can reach
anything it does not answer for.
*/
#[test]
fn renaming_and_moving_are_reachable_by_name_and_not_only_by_the_page() {
    let registry = builtins();

    for id in ["sill.file.rename", "sill.file.move"] {
        let found = registry
            .get(id)
            .unwrap_or_else(|| panic!("{id} is not in the registry, so nothing can reach it"));

        for kind in [ObjectKind::File, ObjectKind::Folder] {
            assert!(found.accepts(kind), "{id} refuses a {kind:?}");
        }

        assert!(
            found.capabilities().contains(&Capability::FileWrite),
            "{id} changes a file and does not say so"
        );

        // Neither claims Enter. Enter on a file opens it, and a panel whose
        // default moved the thing somewhere else would be a trap.
        assert!(
            !found.is_primary(ObjectKind::File),
            "{id} claims Enter on a file"
        );
    }

    // And both are drawn, so the panel offers what the ids promise.
    for kind in [ObjectKind::File, ObjectKind::Folder] {
        let drawn: Vec<&str> = registry
            .describe(kind, &Default::default())
            .into_iter()
            .map(|a| a.id)
            .collect();

        for id in ["sill.file.rename", "sill.file.move"] {
            assert!(
                drawn.contains(&id),
                "{kind:?} is not offered {id}: {drawn:?}"
            );
        }
    }
}

/// Moving comes back with a way to put it back, and renaming does not.
///
/// The asymmetry is deliberate and worth stating. A move reverses exactly and
/// its token is two paths, so undoing a move of ten gigabytes costs what
/// undoing a move of a text file costs. A rename back is a second rename, and
/// nothing can promise the old name is still free by the time somebody asks.
/// Offering an undo that quietly does nothing is worse than offering none.
#[test]
fn moving_declares_what_it_reads_as_well_as_what_it_writes() {
    let registry = builtins();

    let moving = registry
        .get("sill.file.move")
        .expect("moving is registered");

    assert!(
        moving.capabilities().contains(&Capability::FileRead),
        "moving between two drives copies before it removes, and does not say it reads"
    );

    let renaming = registry
        .get("sill.file.rename")
        .expect("renaming is registered");

    assert!(
        !renaming.capabilities().contains(&Capability::FileRead),
        "renaming reads nothing; a capability nobody needs is one somebody grants"
    );
}

#[test]
fn nothing_that_is_not_a_file_is_offered_a_file_action() {
    // `accepts` is the whole guard. An action listed against a clipboard row
    // or a calculator answer would be offered on a row whose target is not a
    // path at all, and recycling one of those is not a small mistake.
    let registry = sill_lib::actions::builtins();

    for kind in [
        ObjectKind::Answer,
        ObjectKind::ClipboardEntry,
        ObjectKind::Snippet,
        ObjectKind::Quicklink,
        ObjectKind::SystemSetting,
    ] {
        let offered: Vec<&str> = registry.for_kind(kind).iter().map(|a| a.id()).collect();

        assert!(
            !offered.contains(&"sill.file.recycle"),
            "{kind:?} was offered recycling: {offered:?}"
        );
        assert!(
            !offered.contains(&"sill.file.terminal"),
            "{kind:?} was offered a terminal: {offered:?}"
        );
    }
}

#[test]
fn the_destructive_action_asks_for_the_right_permission() {
    // The capability is what a permission screen will read, and one declared
    // wrongly is a thing somebody grants without knowing what they granted.
    let registry = sill_lib::actions::builtins();

    let recycle = registry
        .get("sill.file.recycle")
        .expect("recycling is registered");
    assert!(
        recycle.capabilities().contains(&Capability::FileWrite),
        "recycling does not declare that it writes to the filesystem"
    );

    let terminal = registry
        .get("sill.file.terminal")
        .expect("terminal is registered");
    assert!(
        terminal.capabilities().contains(&Capability::ProcessLaunch),
        "opening a terminal does not declare that it starts a program"
    );
}

#[test]
fn only_the_recoverable_kind_of_deletion_is_offered() {
    // Deleting outright is what a file manager is for. A launcher offering it
    // behind a fuzzy search and one keypress is how somebody loses work they
    // cannot get back, so the only deletion here is the one the system already
    // knows how to undo.
    let registry = sill_lib::actions::builtins();

    for action in registry.for_kind(ObjectKind::File) {
        let title = action.title().to_lowercase();

        if title.contains("delete") || title.contains("remove") {
            assert!(
                title.contains("recycle") || title.contains("bin"),
                "{} deletes without saying it is recoverable",
                action.id()
            );
        }
    }
}

// ------------------------------------------ what the file actions really do

#[test]
fn a_terminal_opens_in_the_folder_and_not_inside_the_file() {
    use sill_lib::actions::folder_of;

    let scratch = tempfile::tempdir().expect("a temp directory");
    let dir = scratch.path();
    let file = dir.join("README.md");
    std::fs::write(&file, "x").unwrap();

    // Nobody means "open a terminal inside README.md".
    assert_eq!(
        folder_of(&file.to_string_lossy()).unwrap(),
        dir.to_string_lossy()
    );

    // A folder is already where it should open.
    assert_eq!(
        folder_of(&dir.to_string_lossy()).unwrap(),
        dir.to_string_lossy()
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn something_with_nowhere_to_open_is_refused_rather_than_guessed_at() {
    use sill_lib::actions::folder_of;

    assert!(folder_of("").is_err());
}

#[test]
fn what_happened_is_reported_by_name_and_not_by_path() {
    use sill_lib::actions::name_of;

    assert_eq!(name_of(r"C:\work\notes.md"), "notes.md");
    assert_eq!(name_of("notes.md"), "notes.md");
    // Nothing to shorten is still worth saying.
    assert_eq!(name_of(r"C:\"), r"C:\");
}

#[cfg(windows)]
#[test]
fn recycling_takes_a_file_away_and_the_bin_still_has_it() {
    // The whole claim of the action: it is the recoverable kind of deletion.
    // Tested against a real file, because a mock would only prove the mock.
    use sill_lib::actions::recycle;

    /*
     * A directory of its own, and it has to be.
     *
     * These recycle a file and then assert it is gone. Named after the test
     * instead, two `cargo test` runs on one machine take turns writing and
     * recycling the same file, and whichever loses recycles something that has
     * already gone. `TempDir` names itself and removes itself on drop.
     */
    let dir = tempfile::tempdir().expect("a temp directory");
    let file = dir.path().join("throwaway.txt");
    std::fs::write(&file, "not wanted").unwrap();
    assert!(file.exists());

    recycle(&file).expect("recycled");

    assert!(!file.exists(), "the file is still where it was");
}

#[cfg(windows)]
#[test]
fn recycling_a_folder_takes_what_is_inside_it_too() {
    use sill_lib::actions::recycle;

    let dir = tempfile::tempdir().expect("a temp directory");
    let inside = dir.path().join("inside");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::write(inside.join("a.txt"), "x").unwrap();

    recycle(&inside).expect("recycled");

    assert!(!inside.exists());
}

#[cfg(windows)]
#[test]
fn recycling_something_that_is_not_there_says_so_rather_than_claiming_success() {
    // Reporting a deletion that did not happen is the one outcome worse than
    // failing, because nobody goes looking for the file afterwards.
    use sill_lib::actions::recycle;

    // Inside a fresh directory, so "not there" is a fact rather than the
    // result of a delete that may not have worked. A shared name would also
    // let another run of this create the file between the delete and the call.
    let dir = tempfile::tempdir().expect("a temp directory");
    let missing = dir.path().join("no-such-file-at-all.txt");

    assert!(recycle(&missing).is_err());
}

// -------------------------------------------------- Windows' own switches

#[test]
fn a_system_switch_is_not_run_by_the_action_that_runs_sill_itself() {
    // The two ask for different things. Opening settings touches Sill;
    // changing the volume touches the machine, and a permission screen should
    // be able to tell somebody which they are agreeing to.
    let registry = builtins();

    let offered: Vec<&str> = registry
        .for_kind(ObjectKind::SystemControl)
        .iter()
        .map(|a| a.id())
        .collect();

    assert!(offered.contains(&"sill.system.run"), "{offered:?}");
    assert!(
        !offered.contains(&"sill.runBuiltin"),
        "Sill's own runner accepts a Windows switch: {offered:?}"
    );
}

#[test]
fn changing_the_machine_asks_for_more_than_drawing_a_window() {
    // Somebody granting a launcher permission to draw its own window has not
    // thereby granted it permission to mute their speakers.
    let registry = builtins();

    let system = registry.get("sill.system.run").expect("registered");
    assert!(system.capabilities().contains(&Capability::SystemControl));
    assert!(
        !system.capabilities().contains(&Capability::Ui),
        "a machine-wide change is declared as if it only touched Sill's window"
    );
}

#[test]
fn nothing_else_is_offered_a_system_switch_action() {
    let registry = builtins();

    for kind in [
        ObjectKind::Builtin,
        ObjectKind::Setting,
        ObjectKind::SystemSetting,
        ObjectKind::File,
        ObjectKind::Answer,
    ] {
        let offered: Vec<&str> = registry.for_kind(kind).iter().map(|a| a.id()).collect();

        assert!(
            !offered.contains(&"sill.system.run"),
            "{kind:?} was offered it: {offered:?}"
        );
    }
}

/// A name that begins another's is listed above it.
///
/// The panel filters by substring, so everything a short title matches, a
/// longer one starting with the same words matches too. That is harmless when
/// the short one is listed first, which is what typing it is trying to reach.
/// It is not harmless the other way round: the longer one is then selected and
/// Enter runs it.
///
/// This is not a tidiness rule. "Move To" was a prefix of "Move to Recycle
/// Bin" **and listed below it**, so typing "move to" selected the action that
/// removes the file. It was found by a test doing exactly that to a file it had
/// made, which is the only reason it was found at all.
///
/// "Open" begins "Open Terminal Here" and is fine, because "Open" is what
/// Enter does and sorts first.
#[test]
fn a_name_that_begins_another_is_listed_above_it() {
    let registry = builtins();

    for kind in ObjectKind::ALL {
        let offered = registry.for_kind(*kind);

        for (at, one) in offered.iter().enumerate() {
            let short = one.title().to_lowercase();

            for (also, other) in offered.iter().enumerate() {
                if at == also {
                    continue;
                }

                if !other.title().to_lowercase().starts_with(&short) {
                    continue;
                }

                assert!(
                    at < also,
                    "{:?}: typing {:?} selects {:?}, which is listed above it",
                    kind,
                    one.title(),
                    other.title(),
                );
            }
        }
    }
}

/// The one action that removes something is offered last.
///
/// The panel is drawn in registration order after the primary, and a file has
/// a dozen actions. The one that takes the file away should not sit among the
/// ones that copy its path, where a mistyped filter reaches it first.
#[test]
fn the_recycle_bin_is_the_last_thing_offered_for_a_file() {
    let registry = builtins();
    let offered = registry.for_kind(ObjectKind::File);

    let last = offered.last().expect("a file has actions");
    assert_eq!(
        last.id(),
        "sill.file.recycle",
        "offered: {:?}",
        offered.iter().map(|a| a.title()).collect::<Vec<_>>()
    );
}

/// A program's own volume is a kind of thing with a full set of actions.
///
/// Enter mutes and unmutes, which is what a mixer gets opened for, and the
/// rest sit in the panel. A kind with one action and four things somebody
/// wants to do would push the other four into the window, where they would be
/// a second implementation of the same thing.
mod a_program_has_more_than_one_volume_action {
    use super::*;

    #[test]
    fn muting_is_what_enter_does() {
        let registry = builtins();
        let primary = registry
            .primary(ObjectKind::AudioSession)
            .expect("something on Enter");

        assert_eq!(primary.id(), "sill.audio.session.mute");
    }

    #[test]
    fn the_panel_offers_a_way_to_move_the_slider() {
        let registry = builtins();
        let offered: Vec<&str> = registry
            .for_kind(ObjectKind::AudioSession)
            .into_iter()
            .map(|action| action.id())
            .collect();

        for wanted in [
            "sill.audio.session.louder",
            "sill.audio.session.quieter",
            "sill.audio.session.half",
            "sill.audio.session.full",
        ] {
            assert!(
                offered.contains(&wanted),
                "{wanted} is not offered: {offered:?}"
            );
        }
    }

    /// Changing how loud one program is changes the machine, not Sill.
    ///
    /// Only the volume actions. "Copy Name" is offered here too and should be:
    /// it is offered on everything that has a name, and copying one touches
    /// the clipboard rather than the machine.
    #[test]
    fn every_volume_action_says_it_touches_the_system() {
        let registry = builtins();

        let ours = registry
            .for_kind(ObjectKind::AudioSession)
            .into_iter()
            .filter(|action| action.id().starts_with("sill.audio.session."));

        let mut checked = 0;
        for action in ours {
            assert!(
                action.capabilities().contains(&Capability::SystemControl),
                "{} does not declare that it changes the machine",
                action.id(),
            );
            checked += 1;
        }

        assert_eq!(checked, 5, "a volume action was added or lost");
    }

    /// A volume action on a file or a window would be nonsense.
    #[test]
    fn nothing_else_is_offered_one() {
        let registry = builtins();

        for kind in ObjectKind::ALL {
            if *kind == ObjectKind::AudioSession {
                continue;
            }

            for action in registry.for_kind(*kind) {
                assert!(
                    !action.id().starts_with("sill.audio.session."),
                    "{:?} is offered {}",
                    kind,
                    action.id(),
                );
            }
        }
    }
}

/// Reading aloud is reachable without anybody having written frontend code.
///
/// The claim the action registry exists to make is that an action written in
/// Rust turns up wherever its kind is offered, with no second list to update.
/// That is worth asserting rather than repeating: the settings panel asks for
/// `text` actions to offer as bindable keys, and the clipboard view asks for
/// `clipboard` ones to draw in its panel, so an action accepting both is
/// reachable in two places the moment it is registered.
///
/// Both spellings are checked through `ObjectKind::from_mode`, because that is
/// the translation the window actually goes through: it sends the mode string,
/// not the kind.
#[test]
fn reading_aloud_reaches_both_places_that_ask_for_text_actions() {
    let registry = sill_lib::actions::builtins();

    for mode in ["text", "clipboard"] {
        let kind = sill_lib::object::ObjectKind::from_mode(mode)
            .unwrap_or_else(|| panic!("{mode} is a mode the window sends"));

        let offered: Vec<&str> = registry
            .describe(kind, &Default::default())
            .into_iter()
            .map(|a| a.id)
            .collect();

        assert!(
            offered.contains(&"sill.text.readAloud"),
            "nothing offers Read Aloud for {mode}, so no key can be bound to it: {offered:?}"
        );
        assert!(
            offered.contains(&"sill.text.stopReading"),
            "Stop Reading is unreachable for {mode}, which leaves no way to stop it: {offered:?}"
        );
    }
}

/// Every action id is namespaced, because a binding stores the id forever.
///
/// A binding written to preferences names the action by id, so an id is a
/// stored format rather than an internal label: renaming one silently breaks
/// whatever key somebody bound to it. The prefix is what keeps them from
/// colliding with an extension's own ids once extensions can register actions.
///
/// Written after two arrived without it. Thirty-nine of forty-one followed a
/// convention nothing was enforcing, which is exactly how long that lasts.
#[test]
fn every_action_id_is_namespaced() {
    let stray: Vec<&str> = sill_lib::actions::builtins()
        .ids()
        .into_iter()
        .filter(|id| !id.starts_with("sill."))
        .collect();

    assert!(stray.is_empty(), "these ids are not namespaced: {stray:?}");
}

/// Every built-in the extension host performs on an extension's behalf names
/// what it reaches.
///
/// These bypass the API layer by design: `Action.CopyToClipboard` and friends
/// carry no callback, so the launcher does the work. For a while that meant
/// they bypassed the permission layer too, and an extension refused the
/// clipboard could render one and have Sill copy for it. `Action.Paste` was
/// the sharp end: it injects keystrokes into whatever window is in front.
#[test]
fn every_extension_builtin_declares_what_it_reaches() {
    use sill_lib::commands::launch::builtin_needs;

    for tag in [
        "Action.CopyToClipboard",
        "Action.OpenInBrowser",
        "Action.Open",
        "Action.Paste",
    ] {
        assert!(
            !builtin_needs(tag).is_empty(),
            "{tag} is performed for an extension and asks for no permission, \
             so it is a way round the gate"
        );
    }

    assert!(
        builtin_needs("Action.SomethingNobodyImplemented").is_empty(),
        "an unknown tag must not inherit anybody's permissions"
    );
}

/// Every kind can say what it is, and no two say the same thing.
///
/// The model is told what it is looking at rather than shown a `mode` string.
/// That description used to be a match on the mode with `_ => "result"`, and
/// nine kinds fell into it: a script, an emoji, a program's volume, a running
/// process, a saved arrangement, a web search and a remembered page were all
/// "result". A model cannot ask about what it cannot name.
#[test]
fn every_kind_describes_itself_distinctly() {
    use sill_lib::object::ObjectKind;

    let mut said: Vec<&str> = ObjectKind::ALL.iter().map(|kind| kind.plainly()).collect();

    for word in &said {
        assert!(!word.is_empty(), "a kind describes itself as nothing");
        assert_ne!(
            *word, "result",
            "a kind still describes itself the way the catch-all used to"
        );
    }

    let total = said.len();
    said.sort_unstable();
    said.dedup();

    assert_eq!(
        said.len(),
        total,
        "two kinds describe themselves identically, so the model cannot tell \
         them apart: {said:?}"
    );
}

/// The safe half of the pair is what Enter runs, and the other one is below it.
///
/// `WM_CLOSE` lets a program put up "save changes?" and write out what it has.
/// `TerminateProcess` does not, and somebody who meant to close a document and
/// lost an afternoon's work will not be consoled by either of them being one
/// key away. So this is checked as an ordering rather than trusted to the
/// order the two are written in: the panel draws what `describe` returns, Enter
/// runs what `primary` answers, and both have to agree that it is the one that
/// asks.
#[test]
fn quit_is_what_enter_does_to_a_process_and_force_quit_is_never() {
    let registry = builtins();

    let primary = registry
        .primary(ObjectKind::Process)
        .expect("a process row has something bound to Enter");

    assert_eq!(
        primary.id(),
        "sill.process.quit",
        "Enter on a process row runs {} rather than the one that asks",
        primary.id()
    );

    let drawn = registry.describe(ObjectKind::Process, &Default::default());

    let quit = drawn
        .iter()
        .position(|action| action.id == "sill.process.quit")
        .expect("Quit is offered on a process");
    let force = drawn
        .iter()
        .position(|action| action.id == "sill.process.forceQuit")
        .expect("Force Quit is offered on a process");

    assert_eq!(quit, 0, "the panel does not open on Quit: {drawn:?}");
    assert!(
        quit < force,
        "Force Quit is drawn above Quit, so the entry that destroys unsaved \
         work is the one under the cursor"
    );
    assert!(
        !drawn[force].primary,
        "Force Quit claims Enter, which is the one key nobody thinks about \
         before pressing"
    );
}

/// Ending a program is offered on a process and on nothing else.
///
/// Both actions parse their target as a process id, so a kind that reached
/// them would be handing a path or a panel name to something that ends
/// whatever number it managed to read out of it.
#[test]
fn nothing_but_a_running_process_can_be_ended() {
    let registry = builtins();

    for kind in ObjectKind::ALL {
        if *kind == ObjectKind::Process {
            continue;
        }

        let offered: Vec<_> = registry
            .for_kind(*kind)
            .into_iter()
            .map(|action| action.id())
            .filter(|id| id.starts_with("sill.process."))
            .collect();

        assert!(
            offered.is_empty(),
            "{kind:?} is offered {offered:?}, which parse a process id out of \
             whatever the row happens to carry"
        );
    }
}

/// Uninstalling is offered on an application, is not what Enter does, and is
/// last.
///
/// The panel is drawn in registration order after the primary is lifted, and
/// the entry that removes a program should not sit above the ones that open it
/// or copy its path. Somebody arrowing down a panel quickly should reach every
/// harmless thing before they reach this.
#[test]
fn uninstalling_is_offered_on_an_application_and_is_the_last_thing_offered() {
    let registry = builtins();

    let drawn = registry.describe(ObjectKind::Application, &Default::default());

    let at = drawn
        .iter()
        .position(|action| action.id == "sill.app.uninstall")
        .expect("an application can be uninstalled");

    assert!(
        !drawn[at].primary,
        "Enter on an application runs its uninstaller rather than opening it"
    );
    assert_eq!(
        at,
        drawn.len() - 1,
        "Uninstall is drawn at {at} of {}, above something harmless: {drawn:?}",
        drawn.len()
    );

    // Nothing else gets it. A file, a folder and a setting are not programs
    // with an entry in the Uninstall hives, and offering it there would be
    // offering an action that can only fail.
    for kind in ObjectKind::ALL {
        if *kind == ObjectKind::Application {
            continue;
        }

        assert!(
            registry
                .for_kind(*kind)
                .into_iter()
                .all(|action| action.id() != "sill.app.uninstall"),
            "{kind:?} is offered an uninstaller"
        );
    }
}

// ------------------------------------------------------- the keys they run on

/// Every action that ships with a key, and the key it ships with.
///
/// Written out rather than read off the registry, which would assert nothing:
/// a typo in a declared chord leaves the action with no key at all, and a test
/// that asked the registry what it declared would agree with the typo.
const DECLARED: &[(&str, &str)] = &[
    ("sill.copyPath", "Ctrl+Shift+C"),
    ("sill.copyName", "Ctrl+Shift+N"),
    ("sill.revealInFolder", "Ctrl+Shift+E"),
    ("sill.file.terminal", "Ctrl+Shift+T"),
    ("sill.copyUrl", "Ctrl+Shift+C"),
    ("sill.text.readAloud", "Ctrl+Shift+S"),
];

#[test]
fn every_declared_shortcut_is_the_chord_it_names() {
    let registry = builtins();
    let all = registry.all();

    for (id, chord) in DECLARED {
        let (_, _, shortcut) = all
            .iter()
            .find(|(found, _, _)| found == id)
            .unwrap_or_else(|| panic!("{id} is not a registered action"));

        let shortcut = shortcut
            .as_ref()
            .unwrap_or_else(|| panic!("{id} declares no key, so its accelerator did not parse"));

        assert_eq!(&shortcut.chord(), chord, "{id}");
    }

    // And nothing else has one. A default key is a decision, so an action
    // acquiring one has to arrive here rather than only in a diff.
    let carrying: Vec<&str> = all
        .iter()
        .filter(|(_, _, shortcut)| shortcut.is_some())
        .map(|(id, _, _)| *id)
        .collect();

    assert_eq!(carrying.len(), DECLARED.len(), "{carrying:?}");
}

#[test]
fn nothing_destructive_ships_with_a_key() {
    // A key that recycles a file, quits a program or uninstalls one, given
    // rather than chosen, is worse than no key. Everything here can still be
    // bound by hand in Settings; none of it happens by default.
    let registry = builtins();

    for (id, _, shortcut) in registry.all() {
        if shortcut.is_none() {
            continue;
        }

        for word in ["delete", "recycle", "quit", "uninstall", "forget", "close"] {
            assert!(
                !id.to_ascii_lowercase().contains(word),
                "{id} ships with a key and sounds like it destroys something"
            );
        }
    }
}

#[test]
fn no_two_actions_on_one_list_want_the_same_key() {
    // Two actions on one panel sharing a chord means the second never fires
    // and nothing on screen says which. The panel matcher takes the first, so
    // this is the check that stops one shipping.
    let registry = builtins();

    for kind in ObjectKind::ALL {
        let shown: Vec<(String, String, Option<sill_lib::action_keys::Shortcut>)> = registry
            .describe(*kind, &Default::default())
            .into_iter()
            .map(|a| (a.id.to_string(), a.title.to_string(), a.shortcut))
            .collect();

        let clashes = sill_lib::action_keys::conflicts(&shown);
        assert!(clashes.is_empty(), "{kind:?}: {clashes:?}");
    }
}

#[test]
fn no_default_key_is_one_the_launcher_already_uses_to_move() {
    // The action matcher is asked before the chord map, so an action claiming
    // Ctrl+J would take Next away from anybody using vim bindings, silently.
    let registry = builtins();

    for preset in sill_lib::navigation::Preset::ALL {
        let navigation = sill_lib::navigation::Navigation {
            preset,
            ..Default::default()
        };
        let moves = sill_lib::navigation::chords(&navigation);

        for (id, _, shortcut) in registry.all() {
            let Some(shortcut) = shortcut else { continue };
            let chord = shortcut.chord();

            assert!(
                !moves.contains_key(&chord),
                "{id} ships with {chord}, which is how {preset:?} moves around the list"
            );
        }
    }
}

#[test]
fn a_key_set_in_settings_is_the_one_the_panel_draws() {
    // The whole point of the setting. The registry resolves the override, so
    // the panel and the matcher are looking at one answer rather than at the
    // default plus a correction applied somewhere else.
    let registry = builtins();

    let keys = sill_lib::action_keys::Settings {
        overrides: [
            ("sill.copyPath".to_string(), "Ctrl+Alt+P".to_string()),
            // Cleared rather than changed, which has to be possible.
            ("sill.copyName".to_string(), String::new()),
        ]
        .into_iter()
        .collect(),
    };

    let drawn = registry.describe(ObjectKind::File, &keys);

    let path = drawn
        .iter()
        .find(|a| a.id == "sill.copyPath")
        .expect("Copy Path is offered on a file");
    assert_eq!(
        path.shortcut.as_ref().map(|s| s.chord()),
        Some("Ctrl+Alt+P".to_string())
    );

    let name = drawn
        .iter()
        .find(|a| a.id == "sill.copyName")
        .expect("Copy Name is offered on a file");
    assert!(name.shortcut.is_none(), "a cleared key came back anyway");
}

#[test]
fn a_conflict_somebody_creates_is_reported() {
    // Somebody is allowed to set two actions to one key; what must not happen
    // is both appearing to work. One fires, and the settings row for the other
    // has to be able to say so.
    let registry = builtins();

    let keys = sill_lib::action_keys::Settings {
        // Copy Name onto the key Copy Path already has, on every list the two
        // of them share.
        overrides: [("sill.copyName".to_string(), "Ctrl+Shift+C".to_string())]
            .into_iter()
            .collect(),
    };

    let shown: Vec<(String, String, Option<sill_lib::action_keys::Shortcut>)> = registry
        .describe(ObjectKind::File, &keys)
        .into_iter()
        .map(|a| (a.id.to_string(), a.title.to_string(), a.shortcut))
        .collect();

    let clashes = sill_lib::action_keys::conflicts(&shown);

    assert_eq!(clashes.len(), 1, "{clashes:?}");
    assert_eq!(clashes[0].chord, "Ctrl+Shift+C");
    assert_eq!(clashes[0].other, "Copy Path");
}

#[test]
fn a_key_set_in_settings_is_still_the_key_after_a_restart() {
    // The whole chain in one test, because each half passing separately is
    // what `P0-01` looked like: the panel wrote a settings object, the object
    // was correct, and nothing reached disk. So this saves a preferences file,
    // reads it back the way startup does, and asks the registry what the
    // action panel would draw from what came off disk.
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("preferences.json");

    let mut prefs = sill_lib::preferences::Preferences::default();
    prefs
        .action_keys
        .overrides
        .insert("sill.copyPath".to_string(), "Ctrl+Alt+P".to_string());
    prefs.save(&path).expect("saved");

    // Nothing of the in-memory object survives past here. Everything below
    // comes from the bytes on disk.
    drop(prefs);
    let reloaded = sill_lib::preferences::Preferences::load(&path);

    let registry = builtins();
    let drawn = registry.describe(ObjectKind::File, &reloaded.action_keys);

    let path_action = drawn
        .iter()
        .find(|a| a.id == "sill.copyPath")
        .expect("Copy Path is offered on a file");

    let shortcut = path_action
        .shortcut
        .as_ref()
        .expect("Copy Path came back with no key at all");

    assert_eq!(shortcut.chord(), "Ctrl+Alt+P");
    // And in the shape the window matches a keystroke against, which is the
    // half a chord string alone would not prove.
    assert_eq!(shortcut.key, "p");
    assert_eq!(
        shortcut.modifiers,
        vec![
            sill_lib::action_keys::Modifier::Ctrl,
            sill_lib::action_keys::Modifier::Alt
        ]
    );
}

// ---------------------------------------------------------- the store shelf

/**
A row in the extension store has a panel, and Enter is not in it.

The last list that said "no actions here", and the worst place for it: a shelf
of code somebody is deciding whether to run, where the one thing anybody wants
is somewhere to go and read it first.

**Nothing claims Enter, deliberately.** What Enter does to a listing is fetch
the source, read it and show what it appears to be able to do before a line of
it runs. That is a conversation across two screens, not an action, and a
registry entry that skipped it would be a way to install somebody else's code
without the one screen written to stop exactly that. Same shape as the
conversation list, where Enter reopens and the panel offers the rest.
*/
#[test]
fn a_store_listing_has_a_panel_and_nothing_in_it_installs() {
    let registry = builtins();
    let drawn = registry.describe(ObjectKind::StoreListing, &Default::default());

    assert!(
        !drawn.is_empty(),
        "the store draws an empty panel, which reads as Ctrl+K being dead"
    );

    assert!(
        registry.primary(ObjectKind::StoreListing).is_none(),
        "something claims Enter on a store listing, which is the install \
         confirmation being skipped: {drawn:?}"
    );

    let ids: Vec<&str> = drawn.iter().map(|action| action.id).collect();

    for wanted in ["sill.store.copySource", "sill.store.remove"] {
        assert!(
            ids.contains(&wanted),
            "a listing is not offered {wanted}: {ids:?}"
        );
    }

    // Last, for the reason Uninstall is last on an application and the recycle
    // bin is last on a file: the panel is drawn in registration order once the
    // primary is lifted, and the entry that removes something should not be
    // the one under the cursor when the panel opens.
    assert_eq!(
        ids.last().copied(),
        Some("sill.store.remove"),
        "the entry that removes an extension is not the last one offered: {ids:?}"
    );
}

/// Nothing but a listing is offered a store action.
///
/// Both parse their target as an extension's name, so a kind that reached them
/// would be handing a path, a window handle or a process id to something that
/// deletes a directory named after whatever it read.
#[test]
fn nothing_but_a_store_listing_can_be_removed_from_the_store() {
    let registry = builtins();

    for kind in ObjectKind::ALL {
        if *kind == ObjectKind::StoreListing {
            continue;
        }

        let offered: Vec<&str> = registry
            .for_kind(*kind)
            .into_iter()
            .map(|action| action.id())
            .filter(|id| id.starts_with("sill.store."))
            .collect();

        assert!(
            offered.is_empty(),
            "{kind:?} is offered {offered:?}, which read an extension's name \
             out of whatever the row happens to carry"
        );
    }
}

/// A listing is not an extension command, and cannot be run.
///
/// The distinction the kind exists for. An extension command is installed and
/// has an entrypoint; a listing is a row in somebody else's catalogue that may
/// have no files on this machine at all. Offering to run one would be offering
/// an action that can only fail.
#[test]
fn a_store_listing_is_never_offered_a_way_to_run_itself() {
    let registry = builtins();

    let offered: Vec<&str> = registry
        .for_kind(ObjectKind::StoreListing)
        .into_iter()
        .map(|action| action.id())
        .collect();

    for wanted in ["sill.runExtensionCommand", "sill.launch"] {
        assert!(
            !offered.contains(&wanted),
            "a listing is offered {wanted}, which needs an entrypoint it does not have"
        );
    }
}

/// Removing an extension says it writes files, and asks for nothing else.
///
/// It deletes a directory and forgets what that extension was allowed to
/// reach. It does not launch anything, touch the clipboard or reach the
/// network, and a capability nobody needs is one somebody grants without
/// knowing what they granted.
#[test]
fn removing_an_extension_asks_only_to_write_files() {
    let registry = builtins();

    let removing = registry
        .get("sill.store.remove")
        .expect("removing an extension is registered");

    assert_eq!(removing.capabilities(), &[Capability::FileWrite]);

    let copying = registry
        .get("sill.store.copySource")
        .expect("copying the source link is registered");

    assert_eq!(copying.capabilities(), &[Capability::ClipboardWrite]);
}
