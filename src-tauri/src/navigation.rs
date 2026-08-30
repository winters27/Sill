//! Which keys move around the launcher.
//!
//! The keys themselves are pressed in the window, so the window is what
//! matches them. What lives here is the *answer*: which chord means which
//! movement, given a preset and whatever the user has overridden. That is a
//! decision, not a mechanism, and keeping it in one place is what stops the
//! settings screen and the key handler holding two opinions about Ctrl+N.
//!
//! **A preset adds; it never takes the arrow keys away.** Somebody turning on
//! vim bindings has not asked to stop being able to press Down, and a launcher
//! that punishes them for it is a launcher they turn the setting back off in.
//! Every preset is arrows-plus-something.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One thing a key can do while moving around a list.
///
/// Deliberately about movement rather than about content: opening a result is
/// here because Enter is a movement key in every list ever made, but nothing
/// that acts on the *thing* is, because that is what the action registry is
/// for and a second vocabulary for it would drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Move {
    Next,
    Previous,
    /// A screenful down, for a long list.
    PageDown,
    PageUp,
    First,
    Last,
    /// The next group heading, for a list that has them.
    SectionNext,
    SectionPrevious,
    /// Run the selected thing.
    Open,
    /// Show what else can be done to it.
    Actions,
    /// One step back, and out of the launcher from the top.
    Back,
}

impl Move {
    /// Every movement, in the order settings lists them.
    pub const ALL: [Move; 11] = [
        Move::Next,
        Move::Previous,
        Move::PageDown,
        Move::PageUp,
        Move::First,
        Move::Last,
        Move::SectionNext,
        Move::SectionPrevious,
        Move::Open,
        Move::Actions,
        Move::Back,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Move::Next => "Next result",
            Move::Previous => "Previous result",
            Move::PageDown => "Down a screen",
            Move::PageUp => "Up a screen",
            Move::First => "First result",
            Move::Last => "Last result",
            Move::SectionNext => "Next section",
            Move::SectionPrevious => "Previous section",
            Move::Open => "Open",
            Move::Actions => "Actions",
            Move::Back => "Back",
        }
    }

    /// The chords that always work, whatever the preset.
    ///
    /// These are the keys the shape of a list implies, and no preset removes
    /// them. Turning on vim bindings is asking for Ctrl+J as well, never
    /// instead.
    const fn always(self) -> &'static [&'static str] {
        match self {
            Move::Next => &["Down"],
            Move::Previous => &["Up"],
            Move::PageDown => &["PageDown"],
            Move::PageUp => &["PageUp"],
            Move::First => &["Home"],
            Move::Last => &["End"],
            Move::SectionNext => &["Alt+Down"],
            Move::SectionPrevious => &["Alt+Up"],
            Move::Open => &["Enter"],
            // Two, and the second is not decoration. Vim binds Ctrl+K to
            // Previous, so without an alternative here choosing vim removes
            // the action panel entirely and nothing says so. Alt+Enter is
            // what other launchers use for the same menu.
            Move::Actions => &["Ctrl+K", "Alt+Enter"],
            Move::Back => &["Escape"],
        }
    }
}

/// A named set of extra keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Preset {
    /// Arrows and nothing else.
    #[default]
    Standard,
    /// Ctrl with hjkl, and Ctrl+D and Ctrl+U for a screenful.
    Vim,
    /// Ctrl+N and Ctrl+P, and Ctrl+V and Alt+V for a screenful.
    Emacs,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Standard, Preset::Vim, Preset::Emacs];

    pub const fn title(self) -> &'static str {
        match self {
            Preset::Standard => "Arrows only",
            Preset::Vim => "Vim",
            Preset::Emacs => "Emacs",
        }
    }

    /// What this preset adds on top of the keys that always work.
    ///
    /// Ctrl rather than bare letters throughout, in both presets, and that is
    /// forced rather than chosen: the launcher's search field has focus the
    /// entire time, so a bare `j` is the letter j. There is no normal mode to
    /// be in.
    const fn extra(self, movement: Move) -> &'static [&'static str] {
        match (self, movement) {
            (Preset::Vim, Move::Next) => &["Ctrl+J"],
            (Preset::Vim, Move::Previous) => &["Ctrl+K"],
            (Preset::Vim, Move::PageDown) => &["Ctrl+D"],
            (Preset::Vim, Move::PageUp) => &["Ctrl+U"],
            (Preset::Vim, Move::First) => &["Ctrl+G"],
            (Preset::Vim, Move::Last) => &["Ctrl+Shift+G"],

            (Preset::Emacs, Move::Next) => &["Ctrl+N"],
            (Preset::Emacs, Move::Previous) => &["Ctrl+P"],
            (Preset::Emacs, Move::PageDown) => &["Ctrl+V"],
            (Preset::Emacs, Move::PageUp) => &["Alt+V"],
            (Preset::Emacs, Move::First) => &["Alt+Shift+,"],
            (Preset::Emacs, Move::Last) => &["Alt+Shift+."],
            (Preset::Emacs, Move::Back) => &["Ctrl+G"],

            _ => &[],
        }
    }
}

/// How the launcher is moved around.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Navigation {
    pub preset: Preset,
    /// Ctrl and a digit jumps straight to that row.
    ///
    /// Ctrl rather than bare digits for the same reason the presets use it:
    /// the search field has focus and a bare `3` is the character three.
    pub numeric: bool,
    /// One chord replacing whatever a movement would otherwise have.
    ///
    /// A map rather than a list so a movement cannot be overridden twice with
    /// two different answers and leave the winner to iteration order.
    pub overrides: BTreeMap<Move, String>,
}

/// Every chord that means something, and what it means.
///
/// The shape the window wants: it normalises a key event into one chord string
/// and looks it up, rather than testing eleven movements against every press.
///
/// **Later entries win.** Vim binds Ctrl+K to Previous and it is also the
/// default for Actions, so somebody who chose vim gets Previous; that is the
/// point of choosing it. An override beats both, because it is the most
/// specific thing anybody said.
pub fn chords(navigation: &Navigation) -> BTreeMap<String, Move> {
    let mut map = BTreeMap::new();

    for movement in Move::ALL {
        for chord in movement.always() {
            map.insert((*chord).to_string(), movement);
        }
    }

    for movement in Move::ALL {
        for chord in navigation.preset.extra(movement) {
            map.insert((*chord).to_string(), movement);
        }
    }

    for (movement, chord) in &navigation.overrides {
        let chord = chord.trim();
        if chord.is_empty() {
            continue;
        }

        // An override replaces every chord that movement had, so setting one
        // means "this key, not the others", which is what a person setting one
        // has in mind. The keys that always work are left alone: taking Down
        // away because somebody typed Ctrl+J is not a trade anyone asked for.
        map.retain(|held, m| m != movement || movement.always().contains(&held.as_str()));
        map.insert(chord.to_string(), *movement);
    }

    map
}

/// Every chord that could mean this movement, best first.
fn candidates(navigation: &Navigation, movement: Move) -> Vec<String> {
    let mut out = Vec::new();

    if let Some(chord) = navigation
        .overrides
        .get(&movement)
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
    {
        out.push(chord.to_string());
    }

    out.extend(
        navigation
            .preset
            .extra(movement)
            .iter()
            .map(|c| (*c).to_string()),
    );
    out.extend(movement.always().iter().map(|c| (*c).to_string()));
    out
}

/// What a movement resolves to, for showing in settings.
///
/// **Read back out of the map rather than computed a second way.** The
/// preferred chord is not always the one that ends up meaning this: vim takes
/// Ctrl+K for Previous, so the action panel's first choice is gone and its
/// second is what actually works. Deriving this independently is exactly how a
/// settings screen ends up naming a key that does something else.
pub fn effective(navigation: &Navigation, movement: Move) -> String {
    let map = chords(navigation);

    candidates(navigation, movement)
        .into_iter()
        .find(|chord| map.get(chord) == Some(&movement))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> Navigation {
        Navigation::default()
    }

    fn with(preset: Preset) -> Navigation {
        Navigation {
            preset,
            ..Default::default()
        }
    }

    #[test]
    fn the_arrow_keys_survive_every_preset() {
        // The rule the whole module is built around. Somebody turning on vim
        // bindings has not asked to stop being able to press Down, and taking
        // it away is how a setting gets turned straight back off.
        for preset in Preset::ALL {
            let map = chords(&with(preset));

            assert_eq!(map.get("Down"), Some(&Move::Next), "{preset:?}");
            assert_eq!(map.get("Up"), Some(&Move::Previous), "{preset:?}");
            assert_eq!(map.get("Enter"), Some(&Move::Open), "{preset:?}");
            assert_eq!(map.get("Escape"), Some(&Move::Back), "{preset:?}");
        }
    }

    #[test]
    fn a_preset_adds_its_own_keys() {
        let vim = chords(&with(Preset::Vim));
        assert_eq!(vim.get("Ctrl+J"), Some(&Move::Next));
        assert_eq!(vim.get("Ctrl+D"), Some(&Move::PageDown));

        let emacs = chords(&with(Preset::Emacs));
        assert_eq!(emacs.get("Ctrl+N"), Some(&Move::Next));
        assert_eq!(emacs.get("Ctrl+P"), Some(&Move::Previous));

        // And one preset's keys are not the other's.
        assert_eq!(vim.get("Ctrl+N"), None);
        assert_eq!(emacs.get("Ctrl+J"), None);
    }

    #[test]
    fn the_standard_preset_adds_nothing_at_all() {
        let map = chords(&standard());

        // One more than the movements, because the action panel has two.
        assert_eq!(map.len(), Move::ALL.len() + 1, "{map:?}");
        assert!(map.keys().all(|c| !c.starts_with("Ctrl+") || c == "Ctrl+K"));
    }

    #[test]
    fn vim_takes_ctrl_and_k_from_the_action_panel() {
        // A real collision, and the resolution has to be the deliberate one:
        // Ctrl+K is the action panel by default and Previous in vim. Somebody
        // who chose vim chose that.
        let map = chords(&with(Preset::Vim));

        assert_eq!(map.get("Ctrl+K"), Some(&Move::Previous));
        assert!(
            map.values().any(|m| *m == Move::Actions),
            "the action panel became unreachable"
        );
    }

    #[test]
    fn an_override_beats_the_preset() {
        let navigation = Navigation {
            preset: Preset::Vim,
            overrides: [(Move::Next, "Ctrl+Semicolon".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        let map = chords(&navigation);

        assert_eq!(map.get("Ctrl+Semicolon"), Some(&Move::Next));
        assert_eq!(
            map.get("Ctrl+J"),
            None,
            "the preset's key survived an override that replaced it"
        );
    }

    #[test]
    fn an_override_still_does_not_take_the_arrow_key_away() {
        // The same rule as the presets. Setting a key of your own is asking
        // for that key as well, not asking to lose Down.
        let navigation = Navigation {
            overrides: [(Move::Next, "Ctrl+Semicolon".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };

        let map = chords(&navigation);

        assert_eq!(map.get("Ctrl+Semicolon"), Some(&Move::Next));
        assert_eq!(map.get("Down"), Some(&Move::Next), "Down was taken away");
    }

    #[test]
    fn a_blank_override_is_ignored_rather_than_binding_nothing() {
        // An empty string is what a half-finished recorder leaves behind, and
        // binding it would put a key with no name in the map.
        let navigation = Navigation {
            overrides: [(Move::Next, "   ".to_string())].into_iter().collect(),
            ..Default::default()
        };

        let map = chords(&navigation);

        assert!(!map.contains_key(""));
        assert!(!map.contains_key("   "));
        assert_eq!(map.get("Down"), Some(&Move::Next));
    }

    #[test]
    fn what_settings_shows_is_what_actually_happens() {
        // The row says one chord; pressing it has to do that thing. Two ways
        // of computing "the key for this movement" is exactly how a settings
        // screen ends up lying.
        for preset in Preset::ALL {
            let navigation = with(preset);
            let map = chords(&navigation);

            for movement in Move::ALL {
                let shown = effective(&navigation, movement);
                assert!(!shown.is_empty(), "{movement:?} shows nothing");
                assert_eq!(
                    map.get(&shown),
                    Some(&movement),
                    "{preset:?} shows {shown} for {movement:?}, which does something else"
                );
            }
        }
    }

    #[test]
    fn every_movement_is_reachable_under_every_preset() {
        // A preset that shadowed something into unreachability would leave a
        // movement with no key at all, and nothing on screen would say so.
        for preset in Preset::ALL {
            let map = chords(&with(preset));

            for movement in Move::ALL {
                assert!(
                    map.values().any(|m| *m == movement),
                    "{movement:?} has no key under {preset:?}"
                );
            }
        }
    }

    #[test]
    fn preferences_written_before_this_existed_still_load() {
        // Every existing preferences file has no navigation at all.
        let parsed: Navigation = serde_json::from_str("{}").expect("parses");

        assert_eq!(parsed.preset, Preset::Standard);
        assert!(!parsed.numeric);
        assert!(parsed.overrides.is_empty());
    }
}
