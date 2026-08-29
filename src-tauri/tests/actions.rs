//! The action registry's invariants.
//!
//! An integration test rather than a unit test, and not by preference: a lib
//! unit-test binary that retains the action vtables also retains the dialog
//! plugin's `TaskDialogIndirect`, which needs a common-controls v6 manifest
//! that only test targets can be given (see `build.rs`). The app binary
//! carries its own and cannot be given a second.

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
