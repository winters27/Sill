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
    // Saying how deep its own view stack is. Nothing outside the command can
    // hear this and nothing outside the command is touched by it.
    ("UI/navigation", &[]),
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
        // `UI/navigation` is here for the same reason `UI/render` is: it says
        // how deep the command is in its own view stack, which is a fact about
        // a tree the extension already owns. Asking somebody to agree to an
        // extension pushing its own second screen would be asking them about
        // nothing, and every such question makes the real ones cheaper.
        for method in [
            "UI/render",
            "UI/navigation",
            "Storage/get",
            "Storage/set",
            "Storage/list",
        ] {
            assert_eq!(needed(method), Some(&[][..]), "{method} grew a permission");
        }
    }
}

/// Whether an extension has to be granted this before it gets it.
///
/// Wider than [`crate::ai::acting::needs_asking`], which decides the same
/// question for Sill's own AI, and deliberately so. Those two thresholds are
/// different because the trust is different, not because nobody joined them
/// up: the AI reads the clipboard because somebody asked it a question and
/// expects it to look at what is in front of them, while an extension is
/// somebody else's code that happens to be installed. The same word for the
/// permission, a different bar for handing it over.
///
/// Only [`Capability::Ui`] is free, because it is Sill's own surface and an
/// extension drawing a toast in the window it was opened in has reached
/// nothing. Everything else is asked about once and then remembered.
pub fn needs_granting(capability: &Capability) -> bool {
    !matches!(capability, Capability::Ui)
}

/// The permission in the words somebody deciding would use.
///
/// Exhaustive with no `_` arm, for the same reason `touching` is: a capability
/// added later must not quietly become "something" on a card somebody is being
/// asked to agree to.
pub fn plainly(capability: &Capability) -> &'static str {
    match capability {
        Capability::ClipboardRead => "read your clipboard, including its history",
        Capability::ClipboardWrite => "change what is on your clipboard",
        Capability::SelectionRead => "read whatever you have selected",
        Capability::ShellExecution => "run any command on this machine",
        Capability::FileRead => "read files and see what is installed",
        Capability::FileWrite => "change files on disk",
        Capability::ProcessLaunch => "open programs and links",
        Capability::InputInjection => "type into whatever window is in front",
        Capability::Network => "send things over the network",
        Capability::SystemControl => "change this machine's settings",
        Capability::WindowControl => "move and close other programs' windows",
        Capability::Ui => "draw in Sill's own window",
    }
}

/// Whether this extension may do a thing, asked once and then remembered.
///
/// A trait so the API layer can be tested without a window to answer a card,
/// and so that what an extension is allowed to do is decided in one place
/// rather than at each of the twenty-two call sites.
#[async_trait::async_trait]
pub trait Permits: Send + Sync {
    /// `Ok(())` if every capability is granted, `Err(why)` if any is not.
    ///
    /// `why` is shown to the extension, so it says which permission was
    /// refused. An extension that gets a flat "denied" cannot tell somebody
    /// what to turn on.
    async fn allow(&self, extension: &str, needs: &[Capability]) -> Result<(), String>;
}

/// Everything is allowed and nothing is recorded.
///
/// For tests that are about what a method does rather than about whether it
/// was permitted, and for nothing else.
pub struct AllowAll;

#[async_trait::async_trait]
impl Permits for AllowAll {
    async fn allow(&self, _extension: &str, _needs: &[Capability]) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(test)]
mod granting {
    use super::*;

    /// Drawing in Sill's own window is not a permission. If it became one,
    /// every extension would ask on startup for nothing, and the asking that
    /// matters would be trained through.
    #[test]
    fn only_sills_own_surface_is_free() {
        assert!(!needs_granting(&Capability::Ui));

        for capability in [
            Capability::ClipboardRead,
            Capability::ClipboardWrite,
            Capability::FileRead,
            Capability::FileWrite,
            Capability::SelectionRead,
            Capability::ProcessLaunch,
            Capability::InputInjection,
            Capability::Network,
            Capability::SystemControl,
            Capability::WindowControl,
        ] {
            assert!(needs_granting(&capability), "{capability:?} is free");
        }
    }

    /// The bar for an extension is at least as high as the bar for Sill's own
    /// AI. It may be higher, and is; it must never be lower, because that
    /// would mean somebody else's code reaching something Sill's own model has
    /// to ask about.
    #[test]
    fn nothing_the_ai_must_ask_about_is_free_to_an_extension() {
        for capability in [
            Capability::FileWrite,
            Capability::ProcessLaunch,
            Capability::InputInjection,
            Capability::SystemControl,
            Capability::WindowControl,
            Capability::Network,
            Capability::SelectionRead,
        ] {
            assert!(
                crate::ai::acting::needs_asking(&[capability]),
                "{capability:?} no longer needs asking, so this test is stale",
            );
            assert!(needs_granting(&capability), "{capability:?} is free");
        }
    }
}
