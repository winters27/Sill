//! Finishing a path somebody is part way through typing.
//!
//! Typing `C:\Users\Bra` and pressing Tab should offer `C:\Users\Brandon\`,
//! the way every shell and every address bar on this machine does. Before
//! this, a typed path got a row that opened whatever had been typed, and
//! nothing helped anybody type it.
//!
//! ## Why it completes rather than cycles
//!
//! A first press could either fill in the one match, or step through the
//! matches one at a time. Stepping needs state: which match we are on, and
//! when to forget it. That state is wrong the moment somebody edits the text
//! between presses, and getting it wrong means Tab silently replaces a path
//! with an unrelated sibling.
//!
//! Completing to what every match agrees on has no state at all. One match
//! fills in completely; several fill in as far as they are the same and then
//! stop, which is exactly where the next character somebody types is the one
//! that decides. It is also what `cmd` and bash already do, so it is the
//! behaviour somebody typing a path is expecting.

/// A path split into the folder to read and the part being typed.
///
/// The separator belongs to the folder, so `C:\` reads the root with nothing
/// typed yet.
#[derive(Debug, PartialEq)]
pub struct Typing<'a> {
    pub folder: &'a str,
    pub partial: &'a str,
}

/// Splits at the last separator, or nothing if there is not one.
///
/// Without a separator there is no folder to read. `C:` is a drive-relative
/// path, which means "wherever that drive's current directory happens to be",
/// and that is not something to guess at.
pub fn split(typed: &str) -> Option<Typing<'_>> {
    let at = typed.rfind(['\\', '/'])?;

    Some(Typing {
        // Inclusive, so the separator stays with the folder and the root of a
        // drive does not become the empty string.
        folder: &typed[..=at],
        partial: &typed[at + 1..],
    })
}

/// What the field should say, given what is in the folder.
///
/// `None` when nothing matches, which leaves what was typed alone rather than
/// deleting it. Names are compared without case, because Windows does.
pub fn finish(typing: &Typing<'_>, names: &[String]) -> Option<String> {
    let wanted = typing.partial.to_lowercase();

    let matching: Vec<&String> = names
        .iter()
        .filter(|name| name.to_lowercase().starts_with(&wanted))
        .collect();

    let (first, rest) = matching.split_first()?;

    // As far as every match agrees, which for a single match is the whole
    // name. Cut from the real name, so the case that appears is the case on
    // disk rather than the case somebody typed.
    let mut agreed = first.as_str();
    for name in rest {
        agreed = &agreed[..shared(agreed, name)];
    }

    // Nothing was added, so there is nothing to do. Answering with the same
    // string would make Tab look like it had done something.
    if agreed.len() <= typing.partial.len() {
        return None;
    }

    Some(format!("{}{}", typing.folder, agreed))
}

/// How many bytes two names share, without case, ending on a character
/// boundary.
///
/// Bytes rather than characters because the answer indexes a `str`, and
/// slicing a `str` anywhere else panics.
fn shared(one: &str, two: &str) -> usize {
    let mut at = 0;

    for ((i, a), b) in one.char_indices().zip(two.chars()) {
        if !a.eq_ignore_ascii_case(&b) {
            return i;
        }
        at = i + a.len_utf8();
    }

    at
}

/// What is inside a folder, for completion.
///
/// Folders first, each with a separator already on it, because a folder is
/// almost always a step on the way somewhere rather than the destination, and
/// pressing Tab again should read inside it without typing anything between.
#[cfg(windows)]
pub fn inside(folder: &str) -> Vec<String> {
    let expanded = crate::icons::expand_env(folder);

    let Ok(reading) = std::fs::read_dir(&expanded) else {
        // A folder that cannot be read completes to nothing, which leaves the
        // text alone. Somebody halfway through typing a path names a folder
        // that does not exist yet on almost every keystroke.
        return Vec::new();
    };

    let mut folders = Vec::new();
    let mut files = Vec::new();

    for entry in reading.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };

        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            folders.push(name + "\\");
        } else {
            files.push(name);
        }
    }

    folders.sort_unstable();
    files.sort_unstable();
    folders.extend(files);
    folders
}

#[cfg(not(windows))]
pub fn inside(_folder: &str) -> Vec<String> {
    Vec::new()
}

/// The whole question: what should the field say now.
pub fn complete(typed: &str) -> Option<String> {
    let typing = split(typed.trim())?;
    finish(&typing, &inside(typing.folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(all: &[&str]) -> Vec<String> {
        all.iter().map(|one| one.to_string()).collect()
    }

    #[test]
    fn a_path_splits_at_its_last_separator() {
        let typing = split(r"C:\Users\Bra").expect("a path");
        assert_eq!(typing.folder, r"C:\Users\");
        assert_eq!(typing.partial, "Bra");
    }

    #[test]
    fn the_root_of_a_drive_is_a_folder_with_nothing_typed() {
        let typing = split(r"C:\").expect("a path");
        assert_eq!(typing.folder, r"C:\");
        assert_eq!(typing.partial, "");
    }

    /// `C:` means "wherever that drive is sitting", which is not somewhere to
    /// guess at.
    #[test]
    fn a_drive_with_no_separator_is_not_completed() {
        assert_eq!(split("C:"), None);
        assert_eq!(split("notepad"), None);
    }

    #[test]
    fn one_match_is_filled_in_completely() {
        let typing = split(r"C:\Users\Bra").expect("a path");
        let done = finish(&typing, &names(&[r"Brandon\", r"Public\"]));
        assert_eq!(done.as_deref(), Some(r"C:\Users\Brandon\"));
    }

    /// The half that makes this stateless: several matches fill in as far as
    /// they agree and stop where the next character decides.
    #[test]
    fn several_matches_fill_in_as_far_as_they_agree() {
        let typing = split(r"C:\Pro").expect("a path");
        let done = finish(
            &typing,
            &names(&[r"Program Files\", r"Program Files (x86)\", r"ProgramData\"]),
        );
        assert_eq!(done.as_deref(), Some(r"C:\Program"));
    }

    #[test]
    fn nothing_matching_leaves_what_was_typed_alone() {
        let typing = split(r"C:\zzz").expect("a path");
        assert_eq!(finish(&typing, &names(&[r"Users\", r"Windows\"])), None);
    }

    /// Answering with what is already there would make Tab look like it had
    /// done something.
    #[test]
    fn a_name_already_complete_is_not_offered_again() {
        let typing = split(r"C:\Windows").expect("a path");
        assert_eq!(finish(&typing, &names(&["Windows"])), None);
    }

    /// Completed from the name on disk, so the case that appears is the real
    /// one rather than whatever was typed.
    #[test]
    fn windows_does_not_care_about_case_and_neither_does_this() {
        let typing = split(r"C:\users\bra").expect("a path");
        let done = finish(&typing, &names(&[r"Brandon\"]));
        assert_eq!(done.as_deref(), Some(r"C:\users\Brandon\"));
    }

    #[test]
    fn a_forward_slash_is_a_separator_too() {
        let typing = split("C:/Users/Bra").expect("a path");
        assert_eq!(typing.folder, "C:/Users/");
        assert_eq!(typing.partial, "Bra");
    }

    /// A name that is not ASCII must not be cut in the middle of a character.
    #[test]
    fn agreement_stops_on_a_character_boundary() {
        let typing = split(r"C:\caf").expect("a path");
        let done = finish(&typing, &names(&["caf\u{e9} noir\\", "caf\u{e9} rouge\\"]));
        assert_eq!(done.as_deref(), Some("C:\\caf\u{e9} "));
    }

    /// Against a real folder, because the fixtures above agree with the code
    /// by construction and cannot say whether reading a folder works at all.
    #[test]
    #[cfg(windows)]
    fn a_real_folder_completes() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir(dir.path().join("Brandonshire")).expect("a folder");
        std::fs::create_dir(dir.path().join("Brandonsworth")).expect("another");
        std::fs::write(dir.path().join("elsewhere.txt"), "x").expect("a file");

        // As far as the two agree and no further: the next character is what
        // decides between them. Typed one short of the agreement, since
        // completing to exactly what is already there is refused.
        let typed = format!("{}\\Brandon", dir.path().display());
        let done = complete(&typed).expect("completes");
        assert!(
            done.ends_with("Brandons"),
            "{done} should have stopped where the two names stop agreeing"
        );

        // And at the agreement there is nothing left to add, so Tab does
        // nothing rather than appearing to act.
        let at_the_fork = format!("{}\\Brandons", dir.path().display());
        assert_eq!(complete(&at_the_fork), None);

        // A name only one thing has finishes, with the separator that lets
        // the next press read inside it.
        let one = format!("{}\\Brandonsh", dir.path().display());
        let done = complete(&one).expect("completes");
        assert!(done.ends_with("Brandonshire\\"), "{done}");
    }

    /// A folder with one thing in it completes straight through it, so a deep
    /// path is a few presses rather than a few dozen characters.
    #[test]
    fn a_folder_completes_with_its_separator_so_the_next_press_reads_inside() {
        let typing = split(r"C:\Us").expect("a path");
        let done = finish(&typing, &names(&[r"Users\"])).expect("completes");

        assert!(
            done.ends_with('\\'),
            "{done} should end with a separator so Tab again reads inside it"
        );
    }
}
