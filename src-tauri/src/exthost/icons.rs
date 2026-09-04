//! Which mark the window draws for a Raycast icon name.
//!
//! ## Why the list is here rather than in the window
//!
//! Raycast publishes around two hundred and fifty icon names. An extension
//! writes `Icon.Cog` and means the same picture as `Icon.Gear`, `Icon.Xmark`
//! and `Icon.XMarkCircle` are one drawing with and without a ring, and a name
//! nobody drew has to fall back to something rather than to nothing. That is
//! interpretation of somebody else's vocabulary, and interpretation is Rust's
//! job here the same way parsing a query is.
//!
//! It is also the shape this project keeps losing sessions to: one list of
//! names in Rust and another in TypeScript, agreeing on the day they were
//! written and quietly disagreeing a month later. There is one list, and it is
//! this one. `scripts/verify-source.mjs` reads it and reads
//! `src/lib/components/ExtIcon.svelte`, and **fails in both directions**: a
//! name added here with no drawing, a drawing for a name that is not here, and
//! two names folded onto one mark here but drawn by two different arms there.
//! So the pair cannot drift; it is not a pair that has to be remembered.
//!
//! ## Why the window does not ask for this at runtime
//!
//! It would be one call and then a table held for the life of the process, and
//! that is not the problem with it. `iconOf` runs on every row on every
//! keystroke, and Emoji Search draws six hundred and eighty rows; an await per
//! row, or a module-global primed before the first one, buys nothing over a
//! check that runs before the code ships. The route under `/preview/` that
//! draws these components has no Rust behind it at all, so a table fetched
//! over IPC would turn every mark in the harness into a lettered tile and the
//! screenshots would stop being of the thing.
//!
//! ## What a name with no mark does
//!
//! It draws the letter tile, which is the launcher's existing answer for an
//! application whose icon the shell will not give up. That is a decision and
//! not a gap: the remaining Raycast names are artwork, and artwork is a
//! separate job. A relative path into an extension's own assets arrives as a
//! name too and gets the same tile, because the window does not know where an
//! installed extension lives and the alternative is a broken image per row.

/// Every Raycast icon name Sill draws, and the mark it draws for it.
///
/// The second column is the mark's own name rather than the first column
/// repeated, because several names are one picture: `Gear` and `Cog` are the
/// same drawing, and so are `Warning` and `ExclamationMark`. Folding them here
/// rather than in a chain of `||` in the markup is what makes "these two names
/// are the same icon" a fact with a test on it.
///
/// Sorted by mark, then by name, so a reader can see the groups.
pub const MARKS: &[(&str, &str)] = &[
    ("AppWindow", "window"),
    ("Bolt", "bolt"),
    ("Bookmark", "bookmark"),
    ("Calendar", "calendar"),
    ("Checkmark", "checkmark"),
    ("CheckCircle", "check-circle"),
    ("Circle", "circle"),
    ("Clipboard", "clipboard"),
    ("Clock", "clock"),
    ("Code", "terminal"),
    ("Cog", "gear"),
    ("CopyClipboard", "clipboard"),
    ("Document", "document"),
    ("Dot", "dot"),
    ("Download", "download"),
    ("Envelope", "envelope"),
    ("ExclamationMark", "warning"),
    ("Eye", "eye"),
    ("Folder", "folder"),
    ("Gear", "gear"),
    ("Globe", "globe"),
    ("Heart", "heart"),
    ("House", "house"),
    ("Image", "image"),
    ("Info", "info"),
    ("Key", "key"),
    ("Link", "link"),
    ("List", "list"),
    ("Lock", "lock"),
    ("MagnifyingGlass", "magnifying-glass"),
    ("Minus", "minus"),
    ("Music", "music"),
    ("Pencil", "pencil"),
    ("Person", "person"),
    ("PersonCircle", "person"),
    ("Play", "play"),
    ("Plus", "plus"),
    ("Star", "star"),
    ("StarCircle", "star"),
    ("Tag", "tag"),
    ("Terminal", "terminal"),
    ("Text", "document"),
    ("Trash", "trash"),
    ("Upload", "upload"),
    ("Video", "video"),
    ("Warning", "warning"),
    ("Window", "window"),
    ("Xmark", "xmark"),
    ("XMarkCircle", "xmark-circle"),
];

/// The mark for a name, or nothing when Sill has no drawing for it.
///
/// Nothing rather than a nearest guess. An extension asking for a name Sill
/// does not draw and getting an unrelated picture has been told something
/// untrue about its own row, while one getting the lettered tile has simply
/// not been given a picture, which is the same thing the root list says about
/// an application whose icon it could not read.
pub fn mark_for(name: &str) -> Option<&'static str> {
    MARKS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, mark)| *mark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// A name written twice is a name whose second row nothing reaches.
    ///
    /// `mark_for` takes the first match, so a duplicate with a different mark
    /// is a drawing that can never be chosen and a reader who cannot tell
    /// which of the two is live.
    #[test]
    fn no_name_appears_twice() {
        let mut seen = BTreeSet::new();

        for (name, _) in MARKS {
            assert!(seen.insert(*name), "{name} has two rows in MARKS");
        }
    }

    /// Names are the vocabulary somebody else publishes, so they look like it.
    ///
    /// Raycast writes them in upper camel case, and the string an extension
    /// puts in the prop is the property name itself: `Icon.Star` arrives as
    /// `"Star"`. A row written in any other shape matches nothing at runtime
    /// and would sit here looking correct.
    #[test]
    fn every_name_is_written_the_way_raycast_writes_it() {
        for (name, _) in MARKS {
            assert!(
                name.chars().next().is_some_and(char::is_uppercase)
                    && name.chars().all(char::is_alphanumeric),
                "{name} is not a Raycast icon name",
            );
        }
    }

    /// Mark ids are what the window keys its drawings on, so they are one
    /// shape too: lower case words joined by hyphens, and nothing else.
    #[test]
    fn every_mark_is_written_the_way_the_window_keys_them() {
        for (_, mark) in MARKS {
            assert!(
                !mark.is_empty() && mark.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{mark} is not a mark id",
            );
        }
    }

    /// The folding is the point of the second column.
    ///
    /// If every name had a mark of its own this table would be a list with a
    /// column repeated, and the aliases that make it worth having would have
    /// been lost without anything saying so.
    #[test]
    fn names_that_are_one_picture_share_a_mark() {
        let mut by_mark: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (name, mark) in MARKS {
            by_mark.entry(mark).or_default().push(name);
        }

        assert_eq!(by_mark.get("gear"), Some(&vec!["Cog", "Gear"]));
        assert_eq!(by_mark.get("star"), Some(&vec!["Star", "StarCircle"]));
        assert_eq!(
            by_mark.get("warning"),
            Some(&vec!["ExclamationMark", "Warning"]),
        );
    }

    #[test]
    fn a_name_sill_draws_resolves_and_one_it_does_not_answers_nothing() {
        assert_eq!(mark_for("Cog"), Some("gear"));
        assert_eq!(mark_for("Gear"), Some("gear"));
        // Raycast publishes this one and Sill has no drawing for it.
        assert_eq!(mark_for("Livestream"), None);
        // A relative asset path reaches the window as a name too.
        assert_eq!(mark_for("assets/logo.png"), None);
        assert_eq!(mark_for(""), None);
    }
}
