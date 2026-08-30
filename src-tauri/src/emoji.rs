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

use serde::{Deserialize, Serialize};

use crate::registry::CommandRecord;

/// Which skin tone the emoji that have one are shown in.
///
/// A setting rather than six entries in the list. Every emoji with a tone has
/// six variants, so listing them all would put six near-identical waving hands
/// in front of somebody who only ever wants one of them, and bury the rest of
/// the set behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    /// The yellow one, which is nobody's skin and is the point of it.
    #[default]
    Default,
    Light,
    MediumLight,
    Medium,
    MediumDark,
    Dark,
}

impl Tone {
    pub const ALL: [Tone; 6] = [
        Tone::Default,
        Tone::Light,
        Tone::MediumLight,
        Tone::Medium,
        Tone::MediumDark,
        Tone::Dark,
    ];

    /// A raised hand in this tone, for showing the choice rather than naming it.
    ///
    /// Naming skin tones in words is both awkward and less clear than the
    /// thing itself: nobody picks "medium-light" off a list, they pick the one
    /// that looks right.
    pub fn swatch(self) -> String {
        emojis::get("\u{270B}")
            .and_then(|hand| hand.with_skin_tone(self.into()))
            .map(|hand| hand.as_str().to_string())
            .unwrap_or_else(|| "\u{270B}".to_string())
    }
}

impl From<Tone> for emojis::SkinTone {
    fn from(tone: Tone) -> Self {
        match tone {
            Tone::Default => emojis::SkinTone::Default,
            Tone::Light => emojis::SkinTone::Light,
            Tone::MediumLight => emojis::SkinTone::MediumLight,
            Tone::Medium => emojis::SkinTone::Medium,
            Tone::MediumDark => emojis::SkinTone::MediumDark,
            Tone::Dark => emojis::SkinTone::Dark,
        }
    }
}

/// What Enter does to an emoji.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Primary {
    /// Put it where the user was typing. What a picker is for.
    #[default]
    Paste,
    /// Leave it on the clipboard, for somewhere Sill cannot paste into.
    Copy,
}

/// The emoji picker's settings.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub tone: Tone,
    pub primary: Primary,
}

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
pub fn records(tone: Tone) -> Vec<CommandRecord> {
    emojis::iter()
        .map(|emoji| {
            // Only the character changes with a tone. The name of a toned
            // variant is "raised hand: dark skin tone", which is not what
            // anyone wants to read down a list or type to find one: the tone
            // is already visible in the character itself.
            //
            // Only some emoji have tones: a waving hand does, a birthday cake
            // does not. `with_skin_tone` says which by returning nothing,
            // which is the difference between "this tone" and "no tone to
            // give".
            let character = emoji.with_skin_tone(tone.into()).unwrap_or(emoji).as_str();

            // The shortcode is what people actually type: somebody reaching
            // for 🎉 types "tada" far more often than "party popper". Both
            // are searchable, the name reads better on the row.
            let shortcode = emoji.shortcode().unwrap_or_default();

            CommandRecord {
                // The base character, not the toned one. Ids are compared
                // for aliases, hotkeys and hiding, and changing the tone must
                // not silently orphan every one of them.
                id: format!("{PREFIX}{}", emoji.as_str()),
                extension: "emoji".to_string(),
                extension_title: group_name(emoji.group()).to_string(),
                command: emoji.name().to_string(),
                title: emoji.name().to_string(),
                // The character itself, large enough to be the point of the
                // row rather than a detail on it.
                subtitle: character.to_string(),
                description: String::new(),
                mode: "emoji".to_string(),
                // What copying an emoji copies.
                entrypoint: character.to_string(),
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
        records(Tone::Default)
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
    fn a_skin_tone_changes_the_emoji_that_have_one() {
        let plain = records(Tone::Default)
            .into_iter()
            .find(|r| r.title == "raised hand")
            .expect("the raised hand is in the set");
        let dark = records(Tone::Dark)
            .into_iter()
            .find(|r| r.title == "raised hand")
            .expect("still there with a tone");

        assert_ne!(plain.entrypoint, dark.entrypoint, "the tone did nothing");
        assert_eq!(plain.title, dark.title, "and it is still the same emoji");
        assert_eq!(
            plain.id, dark.id,
            "changing the tone would orphan every alias and hotkey"
        );
    }

    #[test]
    fn an_emoji_with_no_tone_is_untouched_by_the_setting() {
        // A waving hand has a tone; a birthday cake does not. Asking for one
        // must leave the rest of the set exactly as it was rather than
        // dropping the emoji that cannot answer.
        for tone in Tone::ALL {
            let cake = records(tone)
                .into_iter()
                .find(|r| r.title == "birthday cake")
                .unwrap_or_else(|| panic!("dropped by {tone:?}"));

            assert_eq!(cake.entrypoint, "\u{1F382}", "{tone:?} altered it");
        }
    }

    #[test]
    fn the_set_is_the_same_size_whatever_the_tone() {
        // Choosing a tone picks a variant of the ones that have variants. It
        // must not add the other five to the list or remove anything.
        let plain = records(Tone::Default).len();

        for tone in Tone::ALL {
            assert_eq!(records(tone).len(), plain, "{tone:?} changed the count");
        }
    }

    #[test]
    fn every_tone_has_a_swatch_and_no_two_are_the_same() {
        // The swatch is how the choice is offered, so two identical ones would
        // be two buttons that look like the same answer.
        let mut seen = std::collections::HashSet::new();

        for tone in Tone::ALL {
            let swatch = tone.swatch();
            assert!(!swatch.is_empty(), "{tone:?} has no swatch");
            assert!(seen.insert(swatch.clone()), "{tone:?} repeats {swatch}");
        }
    }

    #[test]
    fn the_default_is_the_yellow_one() {
        // Which is nobody's skin, and is the point of it.
        assert_eq!(Tone::default(), Tone::Default);
        assert_eq!(Tone::Default.swatch(), "\u{270B}");
        assert_eq!(Settings::default().primary, Primary::Paste);
    }

    #[test]
    fn the_shortcode_is_searchable_because_it_is_what_people_type() {
        // Somebody reaching for the party popper types "tada" far more often
        // than "party popper", and nothing in the name would find it.
        let tada = records(Tone::Default)
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
        for record in records(Tone::Default) {
            assert!(
                seen.insert(record.id.clone()),
                "{} appears twice",
                record.id
            );
        }
    }

    #[test]
    fn every_emoji_has_something_to_show_and_something_to_copy() {
        for record in records(Tone::Default) {
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
        assert!(
            records(Tone::Default).len() > 1_000,
            "only {}",
            records(Tone::Default).len()
        );
    }
}
