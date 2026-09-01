//! What an extension has to be allowed to do before the host does it.
//!
//! ## Why this is not a second vocabulary
//!
//! `exthost` already carried a `Capabilities` struct with three booleans,
//! `browser_extension`, `window_management` and `file_search`. It was set to
//! its default at every call site and read nowhere, and none of its three
//! names appear in `action::Capability`, which is what the rest of Sill uses
//! to decide whether something needs asking about. Two vocabularies, neither
//! agreeing with the other, one of them inert.
//!
//! So extensions declare the same [`Capability`] an action declares. An
//! extension reading the clipboard and a Sill action reading the clipboard are
//! the same permission, described the same way, and shown to somebody in the
//! same words.
//!
//! ## One row per method, and no method without a row
//!
//! The table below is the only place that says what a call costs. The test at
//! the bottom reads `api.rs` and fails if the two ever disagree, in either
//! direction: a method the API answers with no row here, or a row here for a
//! method that no longer exists.
//!
//! That test is the point of the file. Without it this is a second list that
//! has to be kept up to date by remembering to, and the way it fails is a new
//! method quietly reaching the clipboard with nothing declared.

use crate::action::Capability;

/// What each method the host can be asked for actually touches.
///
/// An empty slice means the call reaches nothing outside the extension: its
/// own view, or its own storage. Those are not permissions and asking about
/// them would teach people to click through the ones that matter.
pub const NEEDED: &[(&str, &[Capability])] = &[
    // The extension's own view. Drawing into the space it was given.
    ("UI/render", &[]),
    // Sill's own surface, and the state of the window the extension is in.
    ("UI/showToast", &[Capability::Ui]),
    ("UI/updateToast", &[Capability::Ui]),
    ("UI/hideToast", &[Capability::Ui]),
    ("UI/showHud", &[Capability::Ui]),
    ("UI/confirmAlert", &[Capability::Ui]),
    ("UI/setSearchText", &[Capability::Ui]),
    ("UI/popToRoot", &[Capability::Ui]),
    ("UI/closeMainWindow", &[Capability::Ui]),
    // Somebody else's text, taken without them doing anything.
    ("UI/getSelectedText", &[Capability::SelectionRead]),
    // The clipboard. Reading is its own permission from writing, because the
    // history holds what was copied out of a password manager an hour ago.
    ("Clipboard/readContent", &[Capability::ClipboardRead]),
    ("Clipboard/copy", &[Capability::ClipboardWrite]),
    ("Clipboard/clear", &[Capability::ClipboardWrite]),
    // Pasting is a write and a keystroke, which is how `sill.emoji.paste`
    // declares it too. The chord lands in whatever is in front, so it can type
    // into a window the extension was never shown.
    (
        "Clipboard/paste",
        &[Capability::ClipboardWrite, Capability::InputInjection],
    ),
    // Starting a program is the loudest thing on this list.
    ("Application/open", &[Capability::ProcessLaunch]),
    // Reading what is installed. Not a launch, but it is a list of what
    // somebody has on their machine, which is worth declaring.
    ("Application/list", &[Capability::FileRead]),
    ("Application/getDefault", &[Capability::FileRead]),
    // The extension's own store, scoped to it and reachable by nothing else.
    ("Storage/get", &[]),
    ("Storage/set", &[]),
    ("Storage/remove", &[]),
    ("Storage/clear", &[]),
    ("Storage/list", &[]),
];

/// What one method needs, or `None` if nothing here has heard of it.
///
/// `None` is not "needs nothing". A method with no row is a method nobody has
/// decided about, and the caller must refuse it rather than run it: the whole
/// failure this file exists to prevent is a new call reaching the clipboard
/// because no row was written for it.
pub fn needed(method: &str) -> Option<&'static [Capability]> {
    NEEDED
        .iter()
        .find(|(name, _)| *name == method)
        .map(|(_, needs)| *needs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every `"Word/word"` string in the API layer, which is how a method is
    /// written both in its match arm and in the host that calls it.
    fn methods_in_api() -> BTreeSet<String> {
        let text = include_str!("api.rs");
        let mut found = BTreeSet::new();

        for piece in text.split('"').skip(1).step_by(2) {
            let mut halves = piece.split('/');

            let (Some(left), Some(right), None) = (halves.next(), halves.next(), halves.next())
            else {
                continue;
            };

            let word = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic());

            if word(left) && word(right) && left.starts_with(char::is_uppercase) {
                found.insert(piece.to_string());
            }
        }

        found
    }

    /// The two lists, in the direction that lets something through.
    #[test]
    fn every_method_the_api_answers_has_been_decided_about() {
        let undecided: Vec<String> = methods_in_api()
            .into_iter()
            .filter(|method| needed(method).is_none())
            .collect();

        assert!(
            undecided.is_empty(),
            "these methods reach the machine with no capability declared: {undecided:?}",
        );
    }

    /// And the direction that leaves a lie behind.
    #[test]
    fn no_row_describes_a_method_that_no_longer_exists() {
        let live = methods_in_api();
        let stray: Vec<&str> = NEEDED
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| !live.contains(*name))
            .collect();

        assert!(
            stray.is_empty(),
            "these rows describe methods the API does not answer: {stray:?}",
        );
    }

    /// A method nobody wrote a row for is refused, not waved through.
    #[test]
    fn an_unknown_method_needs_a_decision_rather_than_nothing() {
        assert!(needed("Shell/run").is_none());
        assert!(needed("").is_none());
    }

    /// Storage and rendering must stay free of permissions. If either grows
    /// one, every extension starts asking on startup and people learn to say
    /// yes without reading, which costs more than it buys.
    #[test]
    fn an_extensions_own_view_and_own_storage_ask_for_nothing() {
        for method in ["UI/render", "Storage/get", "Storage/set", "Storage/list"] {
            assert_eq!(needed(method), Some(&[][..]), "{method} grew a permission");
        }
    }
}
