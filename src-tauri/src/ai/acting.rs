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

/// What stands between the model and the action.
///
/// Three answers rather than a boolean, because the third one is the whole of
/// this decision: a machine that cannot check a person still has a person at
/// it, and the honest thing is the card plus a sentence saying the stronger
/// gate was not available. See [`gate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// Nothing. The action reads, and the worst it costs is a turn.
    Straight,
    /// A card in Sill's own window, answered by a keypress.
    Card,
    /// The same card, and the reason it is not Windows Hello.
    ///
    /// Drawn identically except for one extra line, because the person still
    /// has to decide and the decision has not changed. What has changed is how
    /// much the answer proves, and that is worth one sentence rather than
    /// silence.
    CardInstead(crate::hello::Availability),
    /// Windows Hello, **instead of** the card rather than after it.
    ///
    /// One prompt, not two. The card's three lines fit in Hello's message, its
    /// Allow is a fingerprint and its Refuse is Cancel, so a second dialog
    /// asking the same question would only be a thing to click through. It is
    /// also the better surface over MCP, where Sill may have no window on
    /// screen at all: a system credential prompt is visible by construction,
    /// which is the problem [`super::approval::raise`] has to open a window to
    /// solve.
    Hello,
}

/**
Whether a keypress is enough, or whether a person has to be there.

**The card proves a key was pressed. It does not prove who pressed it.**
`SendInput` is one call away for anything running as the same account, Sill
itself declares [`Capability::InputInjection`] because it makes keystrokes, and
a prompt-injected model that can reach any program that types has an Allow
button it can press for itself. Windows Hello is the one answer in the box that
no software path can manufacture.

**Two capabilities, not all six that stop to ask.** Running a command and
writing a file are the two that change the machine in ways nothing takes back:
a shell is every other capability at once, and a file written over is a file
that was there. Moving a window is undoable and asking for a fingerprint before
each one is how a prompt stops being read, which is the way this kind of gate
actually fails.

`hello` is `None` when Windows was never asked, which is what the setting being
off means. Passing the availability in rather than reading it here is what
makes this a function over values: every case below is reachable in a test on a
machine with no reader, including the ones this machine cannot produce.
*/
pub fn gate(capabilities: &[Capability], hello: Option<crate::hello::Availability>) -> Gate {
    if !needs_asking(capabilities) {
        return Gate::Straight;
    }

    if !wants_a_person(capabilities) {
        return Gate::Card;
    }

    match hello {
        // Not asked, so nothing was promised and there is nothing to explain.
        None => Gate::Card,
        Some(had) if had.ready() => Gate::Hello,

        /*
         * The third answer, and the one this item turns on.
         *
         * Failing open would run the action, which makes the setting theatre.
         * Failing closed would mean the AI panel does not work at all on a
         * machine with no enrolled Hello credential, with nothing on screen
         * saying why, and that is most machines: this one included, where
         * Windows answers `DeviceNotPresent`.
         *
         * So it degrades to the strongest gate the machine actually has, and
         * says so. The card is not nothing. It is the protection that existed
         * before this item and it is still a person deciding; what it cannot
         * do is prove which person, and the line on the card is what stops
         * somebody believing otherwise.
         */
        Some(had) => Gate::CardInstead(had),
    }
}

/// Whether any of these is heavy enough to want a person rather than a key.
///
/// Public because the one caller has to know whether to ask Windows about the
/// reader at all, and asking on every window move would be a WinRT call bought
/// for an answer nothing reads. One function rather than a list repeated at
/// the call site, so the two cannot drift.
pub fn wants_a_person(capabilities: &[Capability]) -> bool {
    capabilities.iter().any(heavy)
}

/// Whether one capability is one of the two.
///
/// **Exhaustive on purpose, with no `_` arm**, for the reason [`touching`]
/// is: a capability added later must make the compiler ask whether a keypress
/// is enough for it, rather than being answered `false` by a default nobody
/// revisits.
fn heavy(capability: &Capability) -> bool {
    match capability {
        Capability::ShellExecution | Capability::FileWrite => true,

        Capability::ClipboardRead
        | Capability::ClipboardWrite
        | Capability::FileRead
        | Capability::ProcessLaunch
        | Capability::InputInjection
        | Capability::Network
        | Capability::Ui
        | Capability::LauncherDismiss
        | Capability::SystemControl
        | Capability::SelectionRead
        | Capability::WindowControl => false,
    }
}

/// The one line Windows Hello shows above the fingerprint reader.
///
/// The card's three fields in a sentence, because the system prompt has room
/// for one and somebody holding their finger over a sensor is deciding on it.
/// It names the caller as well, which the card does not have to: the card is
/// drawn inside a conversation somebody is looking at, and this arrives on top
/// of whatever they were doing instead.
pub fn hello_message(title: &str, subject: &str, touches: &str) -> String {
    format!("{title}: {subject}. Sill's AI asked for this, and it {touches}.")
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
        //
        // `LauncherDismiss` is here and is **not** free to an extension, which
        // is the difference this file's own threshold is for. Sill's model
        // hiding the launcher is the ordinary end of the thing the person just
        // asked it to do; somebody else's extension doing it is a window
        // disappearing for a reason nobody chose.
        Capability::ClipboardRead
        | Capability::ClipboardWrite
        | Capability::FileRead
        | Capability::LauncherDismiss
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
        /*
         * A reminder, which is a sentence and nothing else.
         *
         * Here because a scheduled timer names it on its own command line:
         * `sill run sill.reminder.show <message> --kind reminder` reaches
         * `outside.rs`, which builds the object through this function. Without
         * this line the reminder Windows starts would arrive as "Sill has no
         * kind called reminder" and nothing would say why.
         *
         * Safe to let the model name too. The only thing that accepts it puts
         * a sentence on screen in a window Sill already owns.
         */
        "reminder" => ObjectKind::Reminder,
        // A note, by the id a row carries, so a trigger can put yesterday's
        // scratchpad on screen when you sign in. Refused by the action itself
        // when notes are switched off, which is where that decision lives.
        "note" => ObjectKind::Note,
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
        // Loose text names itself, cut to something a card can carry. A
        // reminder is the same thing: the message is all there is of it, and a
        // paragraph of it in a log line or a card is a paragraph too many.
        ObjectKind::Text | ObjectKind::Reminder => {
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

    /// The whole of the Windows Hello decision, as a function over values.
    ///
    /// Every case here is reachable on a machine with no reader, which is what
    /// the availability being a parameter rather than a call buys. This machine
    /// answers `DeviceNotPresent`, so without these fixtures four of the five
    /// arms below could never be exercised at all.
    mod what_needs_a_person {
        use super::*;
        use crate::hello::Availability;

        /// A read runs, whatever the machine can prove.
        #[test]
        fn reading_never_reaches_the_gate() {
            for machine in [
                None,
                Some(Availability::Ready),
                Some(Availability::NoDevice),
            ] {
                assert_eq!(gate(&[Capability::FileRead], machine), Gate::Straight);
                assert_eq!(gate(&[], machine), Gate::Straight);
            }
        }

        /// The two the item names, and only those two.
        #[test]
        fn running_something_and_writing_a_file_want_a_person() {
            assert_eq!(
                gate(&[Capability::ShellExecution], Some(Availability::Ready)),
                Gate::Hello
            );
            assert_eq!(
                gate(&[Capability::FileWrite], Some(Availability::Ready)),
                Gate::Hello
            );
        }

        /// Everything else that stops still stops, and still stops at a card.
        ///
        /// The line this draws is the reason the feature is bearable: a
        /// fingerprint before every window move is a prompt somebody turns off.
        #[test]
        fn the_lighter_ones_are_still_only_a_card() {
            for lighter in [
                Capability::ProcessLaunch,
                Capability::InputInjection,
                Capability::SystemControl,
                Capability::WindowControl,
                Capability::Network,
                Capability::SelectionRead,
            ] {
                assert_eq!(
                    gate(&[lighter], Some(Availability::Ready)),
                    Gate::Card,
                    "{lighter:?} asked for a fingerprint"
                );
            }
        }

        /// One heavy capability in a set is enough. An action that reads a file
        /// and then writes it is an action that writes a file.
        #[test]
        fn one_heavy_capability_is_enough() {
            assert_eq!(
                gate(
                    &[Capability::FileRead, Capability::FileWrite],
                    Some(Availability::Ready)
                ),
                Gate::Hello
            );
        }

        /**
        The answer this item turns on, and the case this machine is in.

        Not open, which would run a shell command on nothing but a model's
        say-so. Not closed, which would break the AI panel outright on every
        machine with no enrolled Hello credential, silently. The card, which is
        a person deciding, plus the reason it could not be more than that.
        */
        #[test]
        fn a_machine_without_hello_falls_back_to_the_card_and_says_why() {
            for missing in [
                Availability::NoDevice,
                Availability::NotSetUp,
                Availability::Blocked,
                Availability::Busy,
                Availability::Unknown,
            ] {
                let decided = gate(&[Capability::ShellExecution], Some(missing));

                assert_eq!(
                    decided,
                    Gate::CardInstead(missing),
                    "{missing:?} did not fall back"
                );
                assert_ne!(decided, Gate::Straight, "{missing:?} failed open");
                assert!(
                    matches!(decided, Gate::CardInstead(had) if had.why().is_some()),
                    "{missing:?} fell back without saying why",
                );
            }
        }

        /// The setting being off is a plain card with nothing to explain.
        ///
        /// Told apart from the fallback deliberately: a card apologising for a
        /// reader on a machine where nobody asked for one is noise, and the
        /// difference between "we could not" and "you said not to" is the
        /// difference between a bug report and a preference.
        #[test]
        fn turning_it_off_is_not_the_same_as_not_having_it() {
            assert_eq!(gate(&[Capability::ShellExecution], None), Gate::Card);
            assert_ne!(
                gate(&[Capability::ShellExecution], None),
                gate(&[Capability::ShellExecution], Some(Availability::NoDevice)),
            );
        }

        /// What the call site asks before spending a WinRT call, read off the
        /// same list the gate reads.
        #[test]
        fn only_the_heavy_two_make_it_worth_asking_windows() {
            assert!(wants_a_person(&[Capability::ShellExecution]));
            assert!(wants_a_person(&[Capability::FileWrite]));
            assert!(!wants_a_person(&[Capability::WindowControl]));
            assert!(!wants_a_person(&[Capability::FileRead]));
            assert!(!wants_a_person(&[]));
        }

        /// Whatever the gate would do, the call site must have asked Windows.
        ///
        /// The two are separate functions because one of them decides whether
        /// to spend a call, and this is what stops them drifting: a capability
        /// promoted to heavy without `wants_a_person` agreeing would be an
        /// action that silently never reaches Hello.
        #[test]
        fn the_two_agree_on_every_capability() {
            for capability in [
                Capability::ClipboardRead,
                Capability::ClipboardWrite,
                Capability::FileRead,
                Capability::FileWrite,
                Capability::ProcessLaunch,
                Capability::InputInjection,
                Capability::Network,
                Capability::Ui,
                Capability::LauncherDismiss,
                Capability::SystemControl,
                Capability::ShellExecution,
                Capability::SelectionRead,
                Capability::WindowControl,
            ] {
                let one = [capability];
                let reaches_hello = gate(&one, Some(Availability::Ready)) == Gate::Hello;

                assert_eq!(
                    reaches_hello,
                    wants_a_person(&one),
                    "{capability:?}: the gate and the call site disagree",
                );
            }
        }

        /// The sentence somebody reads with their finger over the sensor.
        #[test]
        fn the_hello_prompt_names_the_action_the_thing_and_the_harm() {
            let said = hello_message("Run", "deploy.ps1", "runs a command on this machine");

            assert!(said.contains("Run"), "{said}");
            assert!(said.contains("deploy.ps1"), "{said}");
            assert!(said.contains("runs a command on this machine"), "{said}");
            assert!(said.contains("Sill"), "nothing says who is asking: {said}");
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
