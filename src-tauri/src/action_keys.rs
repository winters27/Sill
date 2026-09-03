//! Which chord runs which action, and what the person may change about it.
//!
//! The sibling of [`crate::navigation`], and deliberately built the same way.
//! That module answers "which chord *moves* where"; this one answers "which
//! chord *does* what to the selected thing". Both keep the answer in Rust so
//! that the settings screen and the key handler cannot hold two opinions, and
//! both resolve an override against a default rather than letting the window
//! decide.
//!
//! # Two vocabularies, one bridge
//!
//! Sill already had two ways of writing a chord down before this module
//! existed, and inventing a third would have been the easy mistake.
//!
//! - **The Raycast shape**, `{ modifiers: ["ctrl"], key: "c" }`, is what an
//!   extension declares, what the action panel draws, and what
//!   `src/lib/exthost/actions.ts` matches a keystroke against.
//! - **The accelerator string**, `Ctrl+Shift+C`, is what `chordFrom` in
//!   `src/lib/settings.ts` produces from a key press and what every recorder
//!   in Settings stores.
//!
//! [`Shortcut`] is the first shape, because that is the one an action has to
//! be *drawn* and *matched* in. [`Shortcut::parse`] and [`Shortcut::chord`]
//! are the bridge to the second, because that is the one a person *records*.
//! There is exactly one bridge and it round-trips, which a test pins.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A key that is held rather than pressed.
///
/// An enum rather than a string because a typo in `"shfit"` is a chord that
/// silently never fires, and because the set is closed: the matcher in the
/// window folds Raycast's `cmd`, `ctrl` and `cmdOrCtrl` into one control key
/// on Windows, and has no separate notion of the Windows key at all. Offering
/// a fourth modifier here would be advertising something nothing can match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
}

impl Modifier {
    /// How the accelerator string spells it, which is what `chordFrom` writes.
    const fn accelerator(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Alt => "Alt",
            Modifier::Shift => "Shift",
        }
    }
}

/// One chord, in the shape the action panel draws and the window matches.
///
/// Modifiers are held in a fixed order (Ctrl, Alt, Shift) rather than in the
/// order they were typed, so two ways of writing the same chord compare equal
/// and the conflict check below is not fooled by `Shift+Ctrl+C`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub modifiers: Vec<Modifier>,
    /// Raycast's own key name: lower case, and `enter` rather than `Enter`.
    pub key: String,
}

/// The keys whose two names differ, accelerator on the left.
///
/// Everything else is a single character, which is upper case in an
/// accelerator and lower case in a Raycast key, or a name the two vocabularies
/// already spell the same way, such as `F5`.
const NAMED: &[(&str, &str)] = &[
    ("Up", "arrowUp"),
    ("Down", "arrowDown"),
    ("Left", "arrowLeft"),
    ("Right", "arrowRight"),
    ("Enter", "enter"),
    ("Escape", "escape"),
    ("Space", "space"),
    ("Tab", "tab"),
    ("Delete", "delete"),
    ("Backspace", "backspace"),
];

impl Shortcut {
    /// A chord from the accelerator string a recorder produced.
    ///
    /// Returns the reason rather than `None`, because every one of them is
    /// something worth putting on screen: a person who has just pressed a key
    /// and been given nothing back has no way to guess which of these it was.
    pub fn parse(chord: &str) -> Result<Self, String> {
        let chord = chord.trim();
        if chord.is_empty() {
            return Err("that is not a key".to_string());
        }

        let mut parts: Vec<&str> = chord.split('+').map(str::trim).collect();
        let Some(key) = parts.pop().filter(|k| !k.is_empty()) else {
            return Err("that is not a key".to_string());
        };

        let mut modifiers = Vec::new();
        for part in parts {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.push(Modifier::Ctrl),
                "alt" | "opt" | "option" => modifiers.push(Modifier::Alt),
                "shift" => modifiers.push(Modifier::Shift),
                // Held and matchable everywhere else in Sill, and matchable
                // nowhere here: the window folds Meta into Ctrl, so a chord
                // saved with Super would fire on the Ctrl version of itself.
                // Refused with a reason rather than accepted and quietly
                // rewritten into a different chord.
                "super" | "win" | "cmd" | "meta" => {
                    return Err("the Windows key cannot run an action".to_string());
                }
                other => return Err(format!("{other} is not a modifier")),
            }
        }

        modifiers.sort();
        modifiers.dedup();

        let key = match NAMED
            .iter()
            .find(|(accel, _)| accel.eq_ignore_ascii_case(key))
        {
            Some((_, raycast)) => (*raycast).to_string(),
            // A single character is the same key under either name; only the
            // case differs, and the Raycast side is lower.
            None if key.chars().count() == 1 => key.to_lowercase(),
            None => key.to_string(),
        };

        Ok(Self { modifiers, key })
    }

    /// The accelerator string, which is what a settings row shows.
    ///
    /// The exact inverse of [`Shortcut::parse`] for everything that parse
    /// accepts, so what settings draws is what a recorder would have written
    /// and the two can be compared as strings.
    pub fn chord(&self) -> String {
        let mut parts: Vec<String> = self
            .modifiers
            .iter()
            .map(|m| m.accelerator().to_string())
            .collect();

        let key = match NAMED.iter().find(|(_, raycast)| *raycast == self.key) {
            Some((accel, _)) => (*accel).to_string(),
            None if self.key.chars().count() == 1 => self.key.to_uppercase(),
            None => self.key.clone(),
        };

        parts.push(key);
        parts.join("+")
    }
}

/// What the person has changed about the action shortcuts.
///
/// A map keyed by action id, for the same reason [`crate::navigation`] uses
/// one: a list would let the same action be overridden twice with two
/// different answers and leave the winner to iteration order.
///
/// The value is the accelerator string rather than a [`Shortcut`], because
/// that is what the recorder in Settings produces and storing what was
/// recorded means a chord this build cannot parse is still on disk for a later
/// one to read rather than having been silently dropped on the way in.
///
/// `#[serde(default)]` on the container, not only on the fields. Three nested
/// structs without it once meant that adding a single field reset everybody's
/// settings on upgrade.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Action id to accelerator. An empty string means "no key at all", which
    /// is how a default is turned off rather than changed.
    pub overrides: BTreeMap<String, String>,
}

/// The chord this action actually runs on, given what the person has set.
///
/// An override wins over the default, including an override of "nothing":
/// somebody who cleared a shortcut asked for it to be gone, and falling back
/// to the default there would make clearing one impossible.
///
/// An override this build cannot parse falls back to the default rather than
/// leaving the action unreachable, which is the same choice
/// `entries_that_can_be_read` makes for a binding: one bad entry costs that
/// entry, never the feature.
pub fn effective(settings: &Settings, id: &str, default: Option<Shortcut>) -> Option<Shortcut> {
    match settings.overrides.get(id) {
        Some(chord) if chord.trim().is_empty() => None,
        Some(chord) => Shortcut::parse(chord).ok().or(default),
        None => default,
    }
}

/// Two actions on the same list wanting the same chord.
///
/// Reported rather than resolved. The window's matcher takes the first of them
/// and there is no order that is right, so the honest thing is to run one
/// (never both) and say on the settings row which other action is contesting
/// it. Silently running whichever was registered first is how somebody
/// concludes their shortcut "does not work".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The action whose row should say something.
    pub id: String,
    /// The chord both of them want.
    pub chord: String,
    /// The other action's title, because that is the thing on screen.
    pub other: String,
}

/// Every clash among one list of actions that are shown together.
///
/// Scoped to a list rather than to the whole registry, and that is the
/// difference between a useful warning and noise: Copy Path and Close Window
/// never appear beside each other, so sharing a chord costs nothing. Two
/// actions on a file both wanting Ctrl+Shift+C costs one of them.
///
/// The later action is the one told, because the first is the one that fires.
pub fn conflicts(shown: &[(String, String, Option<Shortcut>)]) -> Vec<Conflict> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut out = Vec::new();

    for (id, title, shortcut) in shown {
        let Some(shortcut) = shortcut else { continue };
        let chord = shortcut.chord();

        match seen.get(&chord) {
            Some(first) => out.push(Conflict {
                id: id.clone(),
                chord,
                other: (*first).to_string(),
            }),
            None => {
                seen.insert(chord, title.as_str());
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chords `chordFrom` in `src/lib/settings.ts` can produce, paired
    /// with the Raycast key `matchesShortcut` in `src/lib/exthost/actions.ts`
    /// compares against. Both halves of the bridge are pinned by one table.
    const BRIDGE: &[(&str, &[Modifier], &str)] = &[
        ("Ctrl+Shift+C", &[Modifier::Ctrl, Modifier::Shift], "c"),
        ("Ctrl+Enter", &[Modifier::Ctrl], "enter"),
        (
            "Ctrl+Shift+Enter",
            &[Modifier::Ctrl, Modifier::Shift],
            "enter",
        ),
        ("Alt+Up", &[Modifier::Alt], "arrowUp"),
        ("Ctrl+Down", &[Modifier::Ctrl], "arrowDown"),
        ("Ctrl+Left", &[Modifier::Ctrl], "arrowLeft"),
        ("Ctrl+Right", &[Modifier::Ctrl], "arrowRight"),
        ("Ctrl+Space", &[Modifier::Ctrl], "space"),
        ("Ctrl+Tab", &[Modifier::Ctrl], "tab"),
        ("Ctrl+Delete", &[Modifier::Ctrl], "delete"),
        ("Ctrl+Backspace", &[Modifier::Ctrl], "backspace"),
        ("Ctrl+Escape", &[Modifier::Ctrl], "escape"),
        (
            "Ctrl+Alt+Shift+K",
            &[Modifier::Ctrl, Modifier::Alt, Modifier::Shift],
            "k",
        ),
        ("Ctrl+F5", &[Modifier::Ctrl], "F5"),
        ("Ctrl+1", &[Modifier::Ctrl], "1"),
    ];

    #[test]
    fn a_recorded_chord_becomes_the_key_the_window_matches() {
        for (chord, modifiers, key) in BRIDGE {
            let parsed = Shortcut::parse(chord).expect(chord);

            assert_eq!(parsed.modifiers, *modifiers, "{chord}");
            assert_eq!(parsed.key, *key, "{chord}");
        }
    }

    #[test]
    fn what_settings_shows_is_what_was_recorded() {
        // The row has to draw the chord back, or somebody sets Ctrl+Shift+C
        // and the screen says something else. Round-tripping is the only way
        // to know the two directions agree.
        for (chord, _, _) in BRIDGE {
            assert_eq!(Shortcut::parse(chord).expect(chord).chord(), *chord);
        }
    }

    #[test]
    fn the_order_the_modifiers_were_typed_in_does_not_make_a_new_chord() {
        // Otherwise the conflict check misses the case it exists for: two
        // actions on Ctrl+Shift+C, one of them written Shift+Ctrl+C.
        assert_eq!(
            Shortcut::parse("Shift+Ctrl+C"),
            Shortcut::parse("Ctrl+Shift+C")
        );
    }

    #[test]
    fn the_windows_key_is_refused_with_a_reason() {
        // The window folds Meta into Ctrl, so a chord saved with Super would
        // fire on the Ctrl version of itself. Accepting it would be
        // advertising a key that does something else.
        let err = Shortcut::parse("Super+K").expect_err("Super was accepted");
        assert!(err.contains("Windows key"), "{err}");
    }

    #[test]
    fn nonsense_is_refused_rather_than_bound() {
        assert!(Shortcut::parse("").is_err());
        assert!(Shortcut::parse("   ").is_err());
        assert!(Shortcut::parse("Ctrl+").is_err());
        assert!(Shortcut::parse("Hyper+K").is_err());
    }

    fn upper() -> Shortcut {
        Shortcut {
            modifiers: vec![Modifier::Ctrl, Modifier::Shift],
            key: "u".to_string(),
        }
    }

    #[test]
    fn an_override_beats_the_default() {
        let settings = Settings {
            overrides: [("sill.copyPath".to_string(), "Ctrl+Alt+P".to_string())]
                .into_iter()
                .collect(),
        };

        let chosen = effective(&settings, "sill.copyPath", Some(upper())).expect("a chord");
        assert_eq!(chosen.chord(), "Ctrl+Alt+P");
    }

    #[test]
    fn an_empty_override_turns_the_default_off() {
        // The only way to say "this action should have no key". Falling back
        // to the default here would make clearing one impossible.
        let settings = Settings {
            overrides: [("sill.copyPath".to_string(), "  ".to_string())]
                .into_iter()
                .collect(),
        };

        assert_eq!(effective(&settings, "sill.copyPath", Some(upper())), None);
    }

    #[test]
    fn an_unreadable_override_costs_the_override_and_not_the_action() {
        // Written by a later build, or edited by hand. The action keeps the
        // key it shipped with rather than becoming unreachable.
        let settings = Settings {
            overrides: [("sill.copyPath".to_string(), "Hyper+P".to_string())]
                .into_iter()
                .collect(),
        };

        assert_eq!(
            effective(&settings, "sill.copyPath", Some(upper())),
            Some(upper())
        );
    }

    #[test]
    fn an_action_with_no_default_and_no_override_has_no_key() {
        assert_eq!(effective(&Settings::default(), "sill.copyPath", None), None);
    }

    #[test]
    fn two_actions_on_one_list_wanting_one_chord_are_reported() {
        let shown = vec![
            (
                "sill.copyPath".to_string(),
                "Copy Path".to_string(),
                Shortcut::parse("Ctrl+Shift+C").ok(),
            ),
            (
                "sill.copyName".to_string(),
                "Copy Name".to_string(),
                // Written the other way round on purpose: the same chord.
                Shortcut::parse("Shift+Ctrl+C").ok(),
            ),
        ];

        let found = conflicts(&shown);

        assert_eq!(found.len(), 1, "{found:?}");
        // The second is the one told, because the first is the one that fires.
        assert_eq!(found[0].id, "sill.copyName");
        assert_eq!(found[0].chord, "Ctrl+Shift+C");
        assert_eq!(found[0].other, "Copy Path");
    }

    #[test]
    fn actions_without_a_chord_are_never_in_conflict() {
        let shown = vec![
            ("a".to_string(), "A".to_string(), None),
            ("b".to_string(), "B".to_string(), None),
        ];

        assert!(conflicts(&shown).is_empty());
    }

    #[test]
    fn settings_read_from_nothing_at_all() {
        // The upgrade path. A section missing from the file has to arrive as
        // its default rather than failing the whole document.
        let read: Settings = serde_json::from_str("{}").expect("an empty object reads");
        assert!(read.overrides.is_empty());
    }
}
