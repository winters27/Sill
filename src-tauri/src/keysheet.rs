//! The keyboard reference, assembled from the keys that actually run.
//!
//! ## Why this is not a written list
//!
//! A reference somebody types out is wrong the first time a key changes, and
//! the person reading it has no way to tell. This project has been bitten
//! four times by a hand-kept list quietly disagreeing with the thing it
//! describes, so the sheet is built from the same three sources the keys
//! themselves come from: the movement preset, the action shortcuts, and the
//! summon key. Nothing here invents a chord.
//!
//! ## Why the assembly is here rather than in the window
//!
//! It is a decision about what to show and in what order, and decisions
//! belong where they can be tested. The window draws what this returns.

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
}

/// A group of lines under a heading.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct KeySection {
    pub title: &'static str,
    pub keys: Vec<KeyLine>,
}

/// What the reference shows, given what the keys currently are.
///
/// Pure, so the ordering and the omissions are testable without an
/// application. Takes what each source already answers rather than reaching
/// for preferences itself.
///
/// `summon` is the one key that is not in either list, and it leads, because
/// it is the only one somebody needs before they can read any of the others.
pub fn reference(
    summon: &str,
    moving: &[(String, String, bool)],
    acting: &[(String, String, bool, bool)],
) -> Vec<KeySection> {
    let mut sections = Vec::new();

    if !summon.trim().is_empty() {
        sections.push(KeySection {
            title: "Opening Sill",
            keys: vec![KeyLine {
                chord: summon.to_string(),
                does: "Summon the launcher".to_string(),
                changed: false,
                contested: false,
            }],
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
        })
        .collect();

    if !moving.is_empty() {
        sections.push(KeySection {
            title: "Moving around",
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
        })
        .collect();

    if !acting.is_empty() {
        sections.push(KeySection {
            title: "Acting on a row",
            keys: acting,
        });
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let sheet = reference("Alt+Space", &moving(), &acting());
        assert_eq!(sheet[0].title, "Opening Sill");
        assert_eq!(sheet[0].keys[0].chord, "Alt+Space");
    }

    /// A key that is not bound is not a line.
    ///
    /// Listing it with an empty column would fill the reference with blank
    /// space and say nothing.
    #[test]
    fn a_movement_with_no_key_is_left_out() {
        let sheet = reference("Alt+Space", &moving(), &acting());
        let moving = sheet
            .iter()
            .find(|one| one.title == "Moving around")
            .expect("a section for moving");

        assert_eq!(moving.keys.len(), 2, "the unbound movement was listed");
        assert!(!moving.keys.iter().any(|line| line.does == "Back"));
    }

    #[test]
    fn an_action_with_no_key_is_left_out() {
        let sheet = reference("Alt+Space", &moving(), &acting());
        let acting = sheet
            .iter()
            .find(|one| one.title == "Acting on a row")
            .expect("a section for acting");

        assert_eq!(acting.keys.len(), 1);
        assert_eq!(acting.keys[0].chord, "Ctrl+Shift+C");
    }

    /// A section with nothing in it is not drawn at all.
    #[test]
    fn an_empty_section_is_not_a_heading_over_nothing() {
        let sheet = reference("", &[], &[]);
        assert!(sheet.is_empty());
    }

    /// A summon key that was refused leaves no line rather than an empty one.
    ///
    /// It has been refused on this machine for weeks (`P1-11`), and a
    /// reference whose first row is a blank chord reads as a bug in the sheet
    /// rather than as the key not being registered.
    #[test]
    fn no_summon_key_means_no_row_for_it() {
        let sheet = reference("   ", &moving(), &acting());
        assert!(!sheet.iter().any(|one| one.title == "Opening Sill"));
    }

    /// A contested chord is shown as contested rather than quietly dropped.
    ///
    /// The alternative is somebody spending ten minutes wondering why a key
    /// the reference promised does nothing.
    #[test]
    fn a_key_another_action_wins_is_still_shown_and_marked() {
        let acting = vec![("Ctrl+Shift+C".into(), "Copy Path".into(), false, true)];
        let sheet = reference("Alt+Space", &[], &acting);

        let line = &sheet
            .iter()
            .find(|one| one.title == "Acting on a row")
            .expect("a section")
            .keys[0];

        assert!(line.contested, "the clash was not carried through");
    }

    /// The order is the order somebody needs it in.
    #[test]
    fn the_sections_read_in_the_order_somebody_meets_them() {
        let sheet = reference("Alt+Space", &moving(), &acting());
        let titles: Vec<&str> = sheet.iter().map(|one| one.title).collect();

        assert_eq!(titles, ["Opening Sill", "Moving around", "Acting on a row"]);
    }
}
