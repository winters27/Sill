//! The action registry's invariants.
//!
//! An integration test rather than a unit test, and not by preference: a lib
//! unit-test binary that retains the action vtables also retains the dialog
//! plugin's `TaskDialogIndirect`, which needs a common-controls v6 manifest
//! that only test targets can be given (see `build.rs`). The app binary
//! carries its own and cannot be given a second.

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
        let drawn = registry.describe(kind);

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
            registry.describe(*kind)[0].primary,
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

        for wanted in [
            "sill.copyPath",
            "sill.file.terminal",
            "sill.file.recycle",
        ] {
            assert!(offered.contains(&wanted), "{kind:?} cannot {wanted}: {offered:?}");
        }
    }
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

    let dir = std::env::temp_dir().join("sill-folder-of");
    std::fs::create_dir_all(&dir).unwrap();
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

    let dir = std::env::temp_dir().join("sill-recycle-one");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("throwaway.txt");
    std::fs::write(&file, "not wanted").unwrap();
    assert!(file.exists());

    recycle(&file).expect("recycled");

    assert!(!file.exists(), "the file is still where it was");

    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(windows)]
#[test]
fn recycling_a_folder_takes_what_is_inside_it_too() {
    use sill_lib::actions::recycle;

    let dir = std::env::temp_dir().join("sill-recycle-folder");
    std::fs::create_dir_all(dir.join("inside")).unwrap();
    std::fs::write(dir.join("inside").join("a.txt"), "x").unwrap();

    recycle(&dir.join("inside")).expect("recycled");

    assert!(!dir.join("inside").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(windows)]
#[test]
fn recycling_something_that_is_not_there_says_so_rather_than_claiming_success() {
    // Reporting a deletion that did not happen is the one outcome worse than
    // failing, because nobody goes looking for the file afterwards.
    use sill_lib::actions::recycle;

    let missing = std::env::temp_dir().join("sill-no-such-file-at-all.txt");
    std::fs::remove_file(&missing).ok();

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
            assert!(offered.contains(&wanted), "{wanted} is not offered: {offered:?}");
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
