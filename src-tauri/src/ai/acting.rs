//! Doing things, through the actions that already exist.
//!
//! No second implementation of anything. The model reaches the same registry
//! the action panel reaches, so an action written for a person is available to
//! it unchanged, gated by the capability it already declares and undone by the
//! descriptor it already returns. Nothing about the AI is a back door, because
//! there is no second path for it to take.
//!
//! ## What asks first
//!
//! The capability decides, not a list kept beside it. A read runs silently
//! because the worst it costs is a turn; anything that writes a file, launches
//! a program, types into a window, changes the machine or reaches the network
//! stops and asks. That rule holds for every action written after this one
//! without anybody remembering it.

use crate::action::Capability;
use crate::object::{Object, ObjectKind};

/// Whether this action stops and asks before it runs.
///
/// One `ClipboardWrite` exception, and it is deliberate rather than an
/// oversight: copying replaces what was on the clipboard, which sounds
/// destructive and is not, because Sill keeps the history and the old value is
/// one keystroke away. Asking before every copy would make the common case
/// tedious enough that the card stops being read, which is the way an
/// approval prompt actually fails.
pub fn needs_asking(capabilities: &[Capability]) -> bool {
    capabilities
        .iter()
        .any(|capability| touching(capability).is_some())
}

/// What the card says the action is about to do.
///
/// The capability in the words somebody reading a prompt would use. It is the
/// one line that has to be true: somebody deciding in half a second is
/// deciding on this, not on the action's name.
pub fn what_it_touches(capabilities: &[Capability]) -> &'static str {
    for capability in capabilities {
        if let Some(said) = touching(capability) {
            return said;
        }
    }

    "reads something"
}

/// What one capability changes, or `None` if it changes nothing.
///
/// The single place that decides both what a card says and whether there is a
/// card at all, because those two answers must never disagree: a capability
/// worth stopping for that the card cannot describe would show somebody a
/// prompt saying "reads something" about an action that writes.
///
/// **Exhaustive on purpose, with no `_` arm.** This match used to end in
/// `_ => continue`, which meant a capability added later was silently treated
/// as harmless and never asked about. Naming every one costs a line and makes
/// the compiler ask the only question that matters when the next is added:
/// does this change anything?
fn touching(capability: &Capability) -> Option<&'static str> {
    match capability {
        Capability::FileWrite => Some("changes files on disk"),
        Capability::ProcessLaunch => Some("opens something"),
        Capability::InputInjection => Some("types into whatever is in front"),
        Capability::SystemControl => Some("changes this machine"),
        Capability::WindowControl => Some("moves or closes a window"),
        Capability::Network => Some("sends something over the network"),
        Capability::SelectionRead => Some("reads what you have selected"),
        Capability::ShellExecution => Some("runs a command on this machine"),

        // Nothing leaves the machine and nothing is altered by these, so a
        // card would be a prompt about nothing. They are still declared, and
        // still shown wherever what an extension can reach is listed.
        Capability::ClipboardRead
        | Capability::ClipboardWrite
        | Capability::FileRead
        | Capability::Ui => None,
    }
}

/// The object an action is being asked to act on.
///
/// Worked out from the target rather than demanded from the model, because a
/// model that has just been handed a path by `find_files` knows the path and
/// has no reason to know Sill's word for what kind of thing it is. Asking for
/// one anyway earns a guess, and a guess about the kind runs the wrong action.
///
/// A kind may still be named when it is not guessable, which is anything not
/// on disk: a switch, a window, a piece of loose text.
pub fn object_for(target: &str, named: Option<&str>) -> Result<Object, String> {
    let kind = match named.map(str::trim).filter(|named| !named.is_empty()) {
        Some(named) => {
            kind_named(named).ok_or_else(|| format!("Sill has no kind called {named}."))?
        }
        None => on_disk(target).ok_or_else(|| {
            format!(
                "There is nothing at {target}, so say what kind of thing it is: \
                 text, systemControl, window or url."
            )
        })?,
    };

    Ok(Object {
        kind,
        // Not an index id: nothing here came out of a scan. Naming what it
        // acts on is what a frecency key would have been for, and there is no
        // ranking to feed.
        id: format!("ai:{target}"),
        target: target.to_string(),
        title: title_for(target, kind),
        mode: String::new(),
    })
}

/// Whether the target is a file or a folder that exists.
fn on_disk(target: &str) -> Option<ObjectKind> {
    let path = std::path::Path::new(target);

    if path.is_dir() {
        return Some(ObjectKind::Folder);
    }

    if path.is_file() {
        return Some(ObjectKind::File);
    }

    None
}

/// The kinds a model may name, which is the ones it cannot work out.
///
/// Deliberately not every kind. A model naming `application` or `snippet` is
/// about to act on something it found in an index, and those carry their own
/// identity that a bare string cannot stand in for.
fn kind_named(named: &str) -> Option<ObjectKind> {
    Some(match named {
        "text" => ObjectKind::Text,
        "systemControl" | "system" => ObjectKind::SystemControl,
        "window" => ObjectKind::Window,
        "url" => ObjectKind::Url,
        "file" => ObjectKind::File,
        "folder" => ObjectKind::Folder,
        // A script is a path like a file is, and without this the model could
        // see `sill.script.run` in the registry and had no way to name
        // anything it applied to: `on_disk` calls a `.ps1` a file, and Run is
        // not an action a file accepts. It is the only kind here that is a
        // path and still needs saying, which is why it is the exception.
        "script" => ObjectKind::Script,
        _ => return None,
    })
}

/// What to call it when reporting what happened.
fn title_for(target: &str, kind: ObjectKind) -> String {
    match kind {
        // A script is named by its file too. Reporting "C:\\Users\\...\\deploy.ps1
        // was started" where "deploy.ps1 was started" would do puts a path in
        // a sentence and the useful half at the end of it.
        ObjectKind::File | ObjectKind::Folder | ObjectKind::Script => std::path::Path::new(target)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| target.to_string()),
        // Loose text names itself, cut to something a card can carry.
        ObjectKind::Text => {
            let short: String = target.chars().take(60).collect();
            if target.chars().count() > 60 {
                format!("{short}\u{2026}")
            } else {
                short
            }
        }
        _ => target.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod what_stops_to_ask {
        use super::*;

        #[test]
        fn anything_that_writes_a_file_asks() {
            assert!(needs_asking(&[Capability::FileWrite]));
        }

        #[test]
        fn anything_that_changes_the_machine_asks() {
            assert!(needs_asking(&[Capability::SystemControl]));
            assert!(needs_asking(&[Capability::WindowControl]));
            assert!(needs_asking(&[Capability::InputInjection]));
            assert!(needs_asking(&[Capability::ProcessLaunch]));
            assert!(needs_asking(&[Capability::Network]));
        }

        #[test]
        fn reading_runs_silently() {
            assert!(!needs_asking(&[Capability::FileRead]));
            assert!(!needs_asking(&[Capability::ClipboardRead]));
            assert!(!needs_asking(&[Capability::Ui]));
        }

        /// Deliberate. Copying sounds destructive and is not: Sill keeps the
        /// history, so the old value is one keystroke away. Asking before
        /// every copy is how a card stops being read.
        #[test]
        fn copying_runs_silently() {
            assert!(!needs_asking(&[Capability::ClipboardWrite]));
        }

        /// One is enough. An action that reads a file and then writes it is
        /// an action that writes a file.
        #[test]
        fn one_capability_that_asks_is_enough() {
            assert!(needs_asking(&[Capability::FileRead, Capability::FileWrite]));
        }

        #[test]
        fn an_action_that_declares_nothing_does_not_ask() {
            assert!(!needs_asking(&[]));
        }

        /// The card is the only thing somebody reads before deciding, so
        /// every capability that stops has words of its own rather than
        /// falling through to a default about reading.
        #[test]
        fn everything_that_asks_says_what_it_touches() {
            for capability in [
                Capability::FileWrite,
                Capability::ProcessLaunch,
                Capability::InputInjection,
                Capability::SystemControl,
                Capability::WindowControl,
                Capability::Network,
            ] {
                let said = what_it_touches(&[capability]);
                assert_ne!(said, "reads something", "{capability:?} has no words");
            }
        }
    }

    mod working_out_what_it_acts_on {
        use super::*;

        fn a_file(name: &str) -> std::path::PathBuf {
            let path = std::env::temp_dir().join(format!("sill-acting-{name}"));
            std::fs::write(&path, b"x").expect("written");
            path
        }

        #[test]
        fn a_file_that_exists_is_a_file() {
            let path = a_file("one.txt");
            let object = object_for(&path.to_string_lossy(), None).expect("an object");
            assert_eq!(object.kind, ObjectKind::File);
            assert_eq!(object.title, "sill-acting-one.txt");
        }

        #[test]
        fn a_folder_that_exists_is_a_folder() {
            let dir = std::env::temp_dir().join("sill-acting-dir");
            std::fs::create_dir_all(&dir).expect("a directory");
            let object = object_for(&dir.to_string_lossy(), None).expect("an object");
            assert_eq!(object.kind, ObjectKind::Folder);
        }

        /// A path is guessable and a switch is not, so the one that is not can
        /// be said. Guessing here would run the wrong action.
        #[test]
        fn something_not_on_disk_can_say_what_it_is() {
            let object = object_for("system.mute", Some("systemControl")).expect("an object");
            assert_eq!(object.kind, ObjectKind::SystemControl);
            assert_eq!(object.target, "system.mute");
        }

        /// A script is on disk and still has to be named.
        ///
        /// The one exception to "a path names itself": `on_disk` calls a
        /// `.ps1` a file, and Run is not something a plain file accepts. Until
        /// the model could say this word, `sill.script.run` was in the
        /// registry with nothing the model could point it at.
        #[test]
        fn a_script_is_a_kind_the_model_may_name() {
            let path = a_file("deploy.ps1");
            let object = object_for(&path.to_string_lossy(), Some("script")).expect("an object");

            assert_eq!(object.kind, ObjectKind::Script);
            assert_eq!(object.title, "sill-acting-deploy.ps1");
            assert_eq!(object.target, path.to_string_lossy());
        }

        #[test]
        fn something_not_on_disk_and_unnamed_says_what_is_missing() {
            let refused = object_for("system.mute", None).expect_err("no kind");
            assert!(refused.contains("what kind"), "it said {refused:?}");
        }

        #[test]
        fn a_kind_sill_does_not_have_says_so() {
            let refused = object_for("whatever", Some("nonsense")).expect_err("no such kind");
            assert!(refused.contains("no kind called"), "it said {refused:?}");
        }

        /// A whole paragraph as a title makes a card nobody reads.
        #[test]
        fn a_long_piece_of_text_is_cut_for_the_card() {
            let long = "word ".repeat(60);
            let object = object_for(&long, Some("text")).expect("an object");
            assert!(object.title.chars().count() <= 61, "{}", object.title);
            assert_eq!(object.target, long, "but what it acts on is whole");
        }

        /// A named kind wins over what is on disk. Somebody acting on the text
        /// of a path rather than on the file is doing that on purpose.
        #[test]
        fn naming_a_kind_beats_guessing_one() {
            let path = a_file("two.txt");
            let object = object_for(&path.to_string_lossy(), Some("text")).expect("an object");
            assert_eq!(object.kind, ObjectKind::Text);
        }
    }
}
