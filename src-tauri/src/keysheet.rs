//! The keyboard reference, assembled from the keys that actually run.
//!
//! ## Why this is not a written list
//!
//! A reference somebody types out is wrong the first time a key changes, and
//! the person reading it has no way to tell. This project has been bitten
//! four times by a hand-kept list quietly disagreeing with the thing it
//! describes, so the sheet is built from the same sources the keys
//! themselves come from: the summon key, the other global hotkeys and the
//! bindings, the movement preset, and the action shortcuts. Nothing here
//! invents a chord.
//!
//! ## Why the assembly is here rather than in the window
//!
//! It is a decision about what to show and in what order, and decisions
//! belong where they can be tested. The window draws what this returns, and
//! the shortcuts panel's keyboard map is drawn from the same answer, so the
//! sheet, the map and the tray menu cannot disagree about which key does what.

use serde::Serialize;

/// One line of the reference: a key and what pressing it does.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyLine {
    /// The accelerator as it is written everywhere else, so it reads the same
    /// here as it does in Settings.
    pub chord: String,
    pub does: String,
    /// Set by hand rather than coming from the preset or the shipped default.
    pub changed: bool,
    /// Another key wants this chord and gets it, so this line is a lie about
    /// what will happen. Said rather than hidden: a reference that quietly
    /// omits a broken binding is how somebody spends ten minutes wondering
    /// why a key does nothing.
    pub contested: bool,
    /// Windows refused to register this chord, because another application
    /// already holds it. The key is set and does nothing, which is the same
    /// shape as `contested` with a different cause and a different fix.
    pub refused: bool,
}

/// A group of lines under a heading.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct KeySection {
    pub title: &'static str,
    pub keys: Vec<KeyLine>,
}

/// The section titles, in the order they are drawn. Named once so the window's
/// recorders can say which section a key lives in without spelling it.
pub const OPENING: &str = "Opening Sill";
pub const ANYWHERE: &str = "From anywhere";
pub const MOVING: &str = "Moving around";
pub const ACTING: &str = "Acting on a row";

/// What the reference shows, given what the keys currently are.
///
/// Pure, so the ordering and the omissions are testable without an
/// application. Takes what each source already answers rather than reaching
/// for preferences itself.
///
/// `summon` is the one key that is not in any list, and it leads, because it
/// is the only one somebody needs before they can read any of the others.
/// `summon_refused` says Windows would not register it, and the line says so
/// rather than promising a key that does nothing; the surfaces that must not
/// name a dead key (the tray menu, the welcome) read that flag and stay quiet.
/// `anywhere` is every other key that works whatever application is in front:
/// `(chord, what it does, refused by Windows)`. `moving` is
/// `(chord, movement, changed)` and `acting` is
/// `(chord, action, changed, contested)`.
pub fn reference(
    summon: &str,
    summon_refused: bool,
    anywhere: &[(String, String, bool)],
    moving: &[(String, String, bool)],
    acting: &[(String, String, bool, bool)],
) -> Vec<KeySection> {
    let mut sections = Vec::new();

    if !summon.trim().is_empty() {
        sections.push(KeySection {
            title: OPENING,
            keys: vec![KeyLine {
                chord: summon.to_string(),
                does: "Summon the launcher".to_string(),
                changed: false,
                contested: false,
                refused: summon_refused,
            }],
        });
    }

    // A global key that is off is not a line. A refused one is: it is set,
    // somebody expects it to work, and the reference is where they look.
    let anywhere: Vec<KeyLine> = anywhere
        .iter()
        .filter(|(chord, _, _)| !chord.trim().is_empty())
        .map(|(chord, does, refused)| KeyLine {
            chord: chord.clone(),
            does: does.clone(),
            changed: false,
            contested: false,
            refused: *refused,
        })
        .collect();

    if !anywhere.is_empty() {
        sections.push(KeySection {
            title: ANYWHERE,
            keys: anywhere,
        });
    }

    let moving: Vec<KeyLine> = moving
        .iter()
        .filter(|(chord, _, _)| !chord.trim().is_empty())
        .map(|(chord, does, changed)| KeyLine {
            chord: chord.clone(),
            does: does.clone(),
            changed: *changed,
            contested: false,
            refused: false,
        })
        .collect();

    if !moving.is_empty() {
        sections.push(KeySection {
            title: MOVING,
            keys: moving,
        });
    }

    // An action with no key is not a line. The panel is where you find those,
    // and listing them here with an empty column would make the reference
    // mostly blank space.
    let acting: Vec<KeyLine> = acting
        .iter()
        .filter(|(chord, _, _, _)| !chord.trim().is_empty())
        .map(|(chord, does, changed, contested)| KeyLine {
            chord: chord.clone(),
            does: does.clone(),
            changed: *changed,
            contested: *contested,
            refused: false,
        })
        .collect();

    if !acting.is_empty() {
        sections.push(KeySection {
            title: ACTING,
            keys: acting,
        });
    }

    sections
}

/// Something that already runs on a key: what, and in which section.
///
/// The answer to "is this chord free", asked by a recorder before it saves.
/// Three separate conflict checks used to exist in the window, none aware of
/// the others: the global keys were checked against each other, the bindings
/// against each other, the action keys against the actions on one list. A key
/// could pass all three and still do two things. This reads the same sheet a
/// person reads, so what it calls free is what they would call free.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KeyOwner {
    pub chord: String,
    pub does: String,
    pub section: &'static str,
}

/// Every line of the reference whose chord is the same key as `accelerator`.
///
/// The same key rather than the same string: `Shift+Ctrl+K` and `Ctrl+Shift+K`
/// are one chord, and a recorder writes the modifiers in whatever order they
/// were pressed. Chords the action-key grammar can read are compared through
/// it, which sorts the modifiers; anything else (a chord with the Windows
/// key, which that grammar refuses) is compared as sorted parts, case-blind.
pub fn owners_of(sections: &[KeySection], accelerator: &str) -> Vec<KeyOwner> {
    let wanted = same_key(accelerator);
    if wanted.is_empty() {
        return Vec::new();
    }

    sections
        .iter()
        .flat_map(|section| {
            section.keys.iter().map(move |line| (section.title, line))
        })
        .filter(|(_, line)| same_key(&line.chord) == wanted)
        .map(|(section, line)| KeyOwner {
            chord: line.chord.clone(),
            does: line.does.clone(),
            section,
        })
        .collect()
}

/// A chord reduced to what identifies the key, whatever order it was written in.
fn same_key(chord: &str) -> String {
    if let Ok(parsed) = crate::action_keys::Shortcut::parse(chord) {
        return parsed.chord().to_ascii_lowercase();
    }

    let mut parts: Vec<String> = chord
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect();
    parts.sort();
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anywhere() -> Vec<(String, String, bool)> {
        vec![
            ("Ctrl+Alt+W".into(), "Open the window switcher".into(), false),
            // A key another application already had.
            ("Ctrl+Shift+S".into(), "Take a screenshot".into(), true),
            // A key that is off.
            (String::new(), "Copy every screen".into(), false),
        ]
    }

    fn moving() -> Vec<(String, String, bool)> {
        vec![
            ("Down".into(), "Next".into(), false),
            ("Up".into(), "Previous".into(), false),
            // A movement the preset leaves unbound.
            (String::new(), "Back".into(), false),
        ]
    }

    fn acting() -> Vec<(String, String, bool, bool)> {
        vec![
            ("Ctrl+Shift+C".into(), "Copy Path".into(), false, false),
            // An action with no key at all.
            (String::new(), "Move to Folder".into(), false, false),
        ]
    }

    #[test]
    fn the_summon_key_leads_because_nothing_else_is_reachable_without_it() {
        let sheet = reference("Alt+Space", false, &anywhere(), &moving(), &acting());
        assert_eq!(sheet[0].title, OPENING);
        assert_eq!(sheet[0].keys[0].chord, "Alt+Space");
    }

    /// A key that is not bound is not a line.
    ///
    /// Listing it with an empty column would fill the reference with blank
    /// space and say nothing.
    #[test]
    fn a_movement_with_no_key_is_left_out() {
        let sheet = reference("Alt+Space", false, &[], &moving(), &acting());
        let moving = sheet
            .iter()
            .find(|one| one.title == MOVING)
            .expect("a section for moving");

        assert_eq!(moving.keys.len(), 2, "the unbound movement was listed");
        assert!(!moving.keys.iter().any(|line| line.does == "Back"));
    }

    #[test]
    fn an_action_with_no_key_is_left_out() {
        let sheet = reference("Alt+Space", false, &[], &moving(), &acting());
        let acting = sheet
            .iter()
            .find(|one| one.title == ACTING)
            .expect("a section for acting");

        assert_eq!(acting.keys.len(), 1);
        assert_eq!(acting.keys[0].chord, "Ctrl+Shift+C");
    }

    /// A global key that is switched off is not a line either.
    #[test]
    fn a_global_key_that_is_off_is_left_out() {
        let sheet = reference("Alt+Space", false, &anywhere(), &[], &[]);
        let anywhere = sheet
            .iter()
            .find(|one| one.title == ANYWHERE)
            .expect("a section for global keys");

        assert_eq!(anywhere.keys.len(), 2, "the key that is off was listed");
        assert!(!anywhere.keys.iter().any(|line| line.does == "Copy every screen"));
    }

    /// A key Windows refused is shown and says so, because it is set and the
    /// person who set it expects it to work.
    #[test]
    fn a_global_key_windows_refused_is_shown_and_marked() {
        let sheet = reference("Alt+Space", false, &anywhere(), &[], &[]);
        let anywhere = sheet.iter().find(|one| one.title == ANYWHERE).expect("a section");
        let screenshot = anywhere
            .keys
            .iter()
            .find(|line| line.does == "Take a screenshot")
            .expect("the refused key is a line");

        assert!(screenshot.refused);
        assert!(!anywhere.keys[0].refused, "the working key was marked refused");
    }

    /// The summon key Windows refused is still the first line, and says so.
    ///
    /// The reference is where somebody looks when the key does nothing, so it
    /// has to be there with the reason. Surfaces that must not name a dead key
    /// read the flag: the tray menu draws no cap for it.
    #[test]
    fn a_refused_summon_key_is_shown_and_marked() {
        let sheet = reference("Alt+Space", true, &[], &[], &[]);
        assert_eq!(sheet[0].title, OPENING);
        assert!(sheet[0].keys[0].refused);
    }

    /// A section with nothing in it is not drawn at all.
    #[test]
    fn an_empty_section_is_not_a_heading_over_nothing() {
        let sheet = reference("", false, &[], &[], &[]);
        assert!(sheet.is_empty());
    }

    /// A summon key that was refused leaves no line rather than an empty one.
    ///
    /// It has been refused on this machine for weeks (`P1-11`), and a
    /// reference whose first row is a blank chord reads as a bug in the sheet
    /// rather than as the key not being registered.
    #[test]
    fn no_summon_key_means_no_row_for_it() {
        let sheet = reference("   ", false, &[], &moving(), &acting());
        assert!(!sheet.iter().any(|one| one.title == OPENING));
    }

    /// A contested chord is shown as contested rather than quietly dropped.
    ///
    /// The alternative is somebody spending ten minutes wondering why a key
    /// the reference promised does nothing.
    #[test]
    fn a_key_another_action_wins_is_still_shown_and_marked() {
        let acting = vec![("Ctrl+Shift+C".into(), "Copy Path".into(), false, true)];
        let sheet = reference("Alt+Space", false, &[], &[], &acting);

        let line = &sheet
            .iter()
            .find(|one| one.title == ACTING)
            .expect("a section")
            .keys[0];

        assert!(line.contested, "the clash was not carried through");
    }

    /// The order is the order somebody needs it in.
    #[test]
    fn the_sections_read_in_the_order_somebody_meets_them() {
        let sheet = reference("Alt+Space", false, &anywhere(), &moving(), &acting());
        let titles: Vec<&str> = sheet.iter().map(|one| one.title).collect();

        assert_eq!(titles, [OPENING, ANYWHERE, MOVING, ACTING]);
    }

    /// The modifiers a recorder writes come in the order they were pressed.
    #[test]
    fn a_chord_owns_the_same_key_whatever_order_its_modifiers_come_in() {
        let sheet = reference("Alt+Space", false, &anywhere(), &moving(), &acting());

        let owners = owners_of(&sheet, "Shift+Ctrl+C");
        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].does, "Copy Path");
        assert_eq!(owners[0].section, ACTING);

        assert_eq!(owners_of(&sheet, "ctrl+shift+c").len(), 1, "case is not identity");
    }

    /// The Windows key is outside the action-key grammar and still has to be
    /// found: the summon key carries it more often than not.
    #[test]
    fn a_chord_with_the_windows_key_is_compared_by_its_parts() {
        let sheet = reference("Super+Space", false, &[], &[], &[]);

        assert_eq!(owners_of(&sheet, "Space+Super")[0].section, OPENING);
        assert!(owners_of(&sheet, "Super+Shift+Space").is_empty());
    }

    #[test]
    fn a_free_key_has_no_owners_and_an_empty_chord_owns_nothing() {
        let sheet = reference("Alt+Space", false, &anywhere(), &moving(), &acting());

        assert!(owners_of(&sheet, "Ctrl+Alt+Q").is_empty());
        assert!(owners_of(&sheet, "").is_empty());
        assert!(owners_of(&sheet, "+").is_empty());
    }
}
