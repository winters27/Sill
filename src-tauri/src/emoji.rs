//! Emoji, as things the launcher can find and copy.
//!
//! Not a picker in the sense of a grid to browse. Sill already has a list you
//! type into and a set of actions for text, so an emoji is text with a name,
//! and the thing that makes one findable is the name rather than a new screen.
//!
//! **Not in the main index.** Three thousand seven hundred entries would
//! nearly quadruple a fifteen-hundred-entry corpus that is ranked on every
//! keystroke, to make "smile" find an emoji as well as an application. They
//! live behind their own command instead, which is the same shape the
//! clipboard history uses and for the same reason.

use crate::registry::CommandRecord;

/// The prefix an emoji's id carries.
///
/// Ids are compared for aliases, hotkeys and hiding, so they have to be stable
/// across restarts and distinct from everything else. The character itself is
/// stable in a way an index into a table is not: the table grows with every
/// Unicode release, and an alias pointing at position 412 would silently start
/// meaning something else.
pub const PREFIX: &str = "emoji:";

/// What a group is called on the row.
///
/// Written out rather than derived from the variant name, because the variant
/// names are `SmileysAndEmotion` and a person reads "Smileys and Emotion".
const fn group_name(group: emojis::Group) -> &'static str {
    use emojis::Group;

    match group {
        Group::SmileysAndEmotion => "Smileys and Emotion",
        Group::PeopleAndBody => "People and Body",
        Group::AnimalsAndNature => "Animals and Nature",
        Group::FoodAndDrink => "Food and Drink",
        Group::TravelAndPlaces => "Travel and Places",
        Group::Activities => "Activities",
        Group::Objects => "Objects",
        Group::Symbols => "Symbols",
        Group::Flags => "Flags",
    }
}

/// Everything with a name, as launcher entries.
///
/// Built on demand rather than held: the whole set is static data already
/// compiled into the binary, and a second copy of it shaped as records would
/// be a permanent cost for a list nobody is looking at most of the time.
pub fn records() -> Vec<CommandRecord> {
    emojis::iter()
        .map(|emoji| {
            // The shortcode is what people actually type: somebody reaching
            // for 🎉 types "tada" far more often than "party popper". Both
            // are searchable, the name reads better on the row.
            let shortcode = emoji.shortcode().unwrap_or_default();

            CommandRecord {
                id: format!("{PREFIX}{}", emoji.as_str()),
                extension: "emoji".to_string(),
                extension_title: group_name(emoji.group()).to_string(),
                command: emoji.name().to_string(),
                title: emoji.name().to_string(),
                // The character itself, large enough to be the point of the
                // row rather than a detail on it.
                subtitle: emoji.as_str().to_string(),
                description: String::new(),
                mode: "emoji".to_string(),
                // What copying an emoji copies.
                entrypoint: emoji.as_str().to_string(),
                keywords: shortcode
                    .split('_')
                    .filter(|word| !word.is_empty())
                    .map(str::to_string)
                    .collect(),
                icon: None,
                panel: None,
                preferences: serde_json::Value::Null,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(name: &str) -> CommandRecord {
        records()
            .into_iter()
            .find(|r| r.title == name)
            .unwrap_or_else(|| panic!("{name} is in the set"))
    }

    #[test]
    fn an_emoji_carries_the_character_as_the_thing_to_copy() {
        let grin = find("grinning face");

        assert_eq!(grin.entrypoint, "\u{1F600}");
        assert_eq!(grin.subtitle, "\u{1F600}");
        assert_eq!(grin.mode, "emoji");
    }

    #[test]
    fn the_id_is_the_character_rather_than_a_position() {
        // Ids are compared for aliases, hotkeys and hiding, so they outlive
        // the process. A position in the table would move with every Unicode
        // release and an alias would silently start meaning something else.
        let grin = find("grinning face");
        assert_eq!(grin.id, "emoji:\u{1F600}");
    }

    #[test]
    fn the_shortcode_is_searchable_because_it_is_what_people_type() {
        // Somebody reaching for the party popper types "tada" far more often
        // than "party popper", and nothing in the name would find it.
        let tada = records()
            .into_iter()
            .find(|r| r.entrypoint == "\u{1F389}")
            .expect("the party popper is in the set");

        assert!(
            tada.keywords.iter().any(|k| k == "tada"),
            "{:?}",
            tada.keywords
        );
        assert_eq!(tada.title, "party popper");
    }

    #[test]
    fn no_two_emoji_share_an_id() {
        // A duplicate would make one of them unreachable, and would make an
        // alias ambiguous about which it meant.
        let mut seen = std::collections::HashSet::new();
        for record in records() {
            assert!(
                seen.insert(record.id.clone()),
                "{} appears twice",
                record.id
            );
        }
    }

    #[test]
    fn every_emoji_has_something_to_show_and_something_to_copy() {
        for record in records() {
            assert!(!record.title.is_empty(), "{} has no name", record.id);
            assert!(
                !record.entrypoint.is_empty(),
                "{} copies nothing",
                record.id
            );
        }
    }

    #[test]
    fn the_set_is_the_whole_set_rather_than_a_handful() {
        // A picker missing the one you want is worse than none, and this is
        // the cheapest possible guard against the source quietly changing.
        assert!(records().len() > 1_000, "only {}", records().len());
    }
}
