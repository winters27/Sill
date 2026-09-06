//! What Sill says the first time it runs on a machine.
//!
//! ## Why the sentences are built here
//!
//! A welcome is the one screen where being wrong costs the most, because it is
//! read before the reader knows enough to doubt it. The first thing it says is
//! which key opens Sill, and **on the machine this was written on that key has
//! been refused at every start for weeks**: something else owns `Alt+Space`,
//! Windows says so once in a log nobody opens, and the launcher goes on
//! believing the settings file. A page that reads the setting and prints
//! "press Alt+Space" is a page whose very first sentence is false.
//!
//! So nothing here reads the configured key on its own. It is given the key
//! **and what registration answered**, which `HotkeyConflicts` already
//! records, and the three cases are three different sentences. That is the
//! same rule `keysheet` next door follows, for the same reason: a written list
//! is wrong the first time the thing it describes changes, and the person
//! reading it has no way to tell.
//!
//! ## Why it is a pure function
//!
//! Every decision here is about words, and words are the part worth pinning
//! down with a test. `greeting` takes facts and returns prose, so "a refused
//! key is never something the welcome tells you to press" is an assertion
//! rather than a hope. The window draws what comes back and decides nothing.

use serde::Serialize;

/// A block of prose: the line somebody reads, and the sentence under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Said {
    pub headline: String,
    pub body: String,
}

/// What choosing a row on the welcome does.
///
/// Every one of these is something the launcher can already do. There is no
/// second action framework here and nothing this can run that is not reachable
/// another way, which is rule 15: a welcome is a shortcut to existing
/// behaviour, not a place where new behaviour hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Does {
    /// Open Settings on the row that sets the key that opens Sill.
    ChooseKey,
    /// Open Settings on the folders file search covers.
    ChooseFolders,
    /// Start the whole-drive indexer that is already on this machine.
    StartEverything,
    /// Show every key Sill answers to.
    ShowKeys,
    /// Put the welcome away and search.
    Finish,
}

/// One row of the welcome: what it offers, and what pressing Enter does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// Stable, so the row keyed by it is the row that changes when the welcome
    /// is asked again after somebody has fixed something.
    pub id: &'static str,
    pub title: String,
    pub subtitle: String,
    pub does: Does,
}

/// The whole welcome, as the window draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    /// How Sill is opened, said as it actually is rather than as configured.
    pub opening: Said,
    /// Whether the key was refused, so the page can mark the line.
    ///
    /// Carried separately from the prose because colour is the window's
    /// business and the sentence is this module's.
    pub summon_taken: bool,
    /// What the icon in the notification area is.
    pub tray: Said,
    /// Things worth doing now, in the order they matter.
    pub steps: Vec<Step>,
}

/// What the machine says about itself, which is all this decides from.
#[derive(Debug, Clone)]
pub struct Facts<'a> {
    /// The key the settings file names.
    pub summon: &'a str,
    /// Whether registering that key was refused.
    ///
    /// The whole reason this struct exists. Everything else here could be read
    /// from preferences; this one cannot, because preferences hold what was
    /// asked for and this is what was answered.
    pub summon_taken: bool,
    /// Whether the icon in the notification area is on.
    pub tray: bool,
    /// The folders Sill indexes itself, as they would be shown.
    pub roots: Vec<String>,
    /// Whether a whole-drive indexer is running and answering.
    pub everything_running: bool,
    /// Whether one is on this machine at all.
    pub everything_installed: bool,
}

/// The words of the welcome, for one particular machine on one particular day.
pub fn greeting(facts: &Facts<'_>) -> Welcome {
    let chord = facts.summon.trim();
    let taken = facts.summon_taken && !chord.is_empty();

    Welcome {
        opening: opening(chord, taken),
        summon_taken: taken,
        tray: tray(facts.tray),
        steps: steps(facts, chord, taken),
    }
}

/// How Sill is opened, in whichever of the three states that is in.
///
/// The middle case is the point of the whole module. It names the key, because
/// somebody has to know which one to go and change, and it never asks anybody
/// to press it.
fn opening(chord: &str, taken: bool) -> Said {
    if chord.is_empty() {
        return Said {
            headline: "No key opens Sill yet".to_string(),
            body: "Choose one below and Sill is one press away from wherever you are.".to_string(),
        };
    }

    // A key Windows would not register is still the key: the keyboard hook
    // takes it ahead of whatever else wanted it. Said, because the hook cannot
    // prove it is alive and a person who presses the key and gets nothing
    // should know where to look.
    if taken {
        return Said {
            headline: format!("Press {chord} to open Sill"),
            body: format!(
                "Another program had already asked Windows for {chord}, so Sill takes it \
                 through its own keyboard hook instead, ahead of that program. If the key \
                 does nothing, choose a different one below."
            ),
        };
    }

    Said {
        headline: format!("Press {chord} to open Sill"),
        body: "Press it again to put Sill away. There is no taskbar button, so Sill stays out \
               of the way until you ask for it."
            .to_string(),
    }
}

/// What the icon in the notification area is for.
///
/// It matters most in exactly the case that made this item worth doing: with
/// the key taken, the icon is the only way in, and somebody who does not know
/// what it is has no way into the application at all.
fn tray(shown: bool) -> Said {
    if shown {
        return Said {
            headline: "The icon in the notification area is Sill".to_string(),
            body: "Click it to open the launcher and right click it for a menu. Its label is \
                   also where Sill says so when something it tried to do did not work."
                .to_string(),
        };
    }

    Said {
        headline: "Sill has no icon anywhere".to_string(),
        body: "The notification area icon is turned off, so a key is the only way in and \
               nothing on screen says Sill is running."
            .to_string(),
    }
}

/// The rows, in the order somebody needs them.
///
/// A way in comes first when there is not one, because nothing below it can be
/// reached until that is fixed.
fn steps(facts: &Facts<'_>, chord: &str, taken: bool) -> Vec<Step> {
    let mut steps = Vec::new();

    if taken || chord.is_empty() {
        steps.push(Step {
            id: "key",
            title: if taken {
                "Choose a key that is free".to_string()
            } else {
                "Choose the key that opens Sill".to_string()
            },
            subtitle: "Settings opens on the row that sets it, and the new key takes effect \
                       straight away."
                .to_string(),
            does: Does::ChooseKey,
        });
    }

    steps.push(folders(&facts.roots));
    steps.push(whole_drives(
        facts.everything_running,
        facts.everything_installed,
    ));

    steps.push(Step {
        id: "keys",
        title: "See every key Sill answers to".to_string(),
        // Rather than printing them here. A list on this page would be a list
        // to keep up to date, and the reference is built from the keys that
        // actually run.
        subtitle: "Built from the keys that really are bound, so nothing on it is a key that \
                   does nothing."
            .to_string(),
        does: Does::ShowKeys,
    });

    steps.push(Step {
        id: "start",
        title: "Start searching".to_string(),
        subtitle: "Escape does the same.".to_string(),
        does: Does::Finish,
    });

    steps
}

/// What Sill indexes of its own, and where to change it.
fn folders(roots: &[String]) -> Step {
    if roots.is_empty() {
        return Step {
            id: "folders",
            title: "Choose what Sill searches".to_string(),
            subtitle: "Sill is not indexing any folders of its own yet, so typing a file name \
                       finds nothing."
                .to_string(),
            does: Does::ChooseFolders,
        };
    }

    Step {
        id: "folders",
        title: "Choose which folders Sill searches".to_string(),
        subtitle: format!(
            "{} is indexed to start with. Add more, or point Sill somewhere else, in Settings.",
            listed(roots)
        ),
        does: Does::ChooseFolders,
    }
}

/// One, two or many folders, written the way a sentence would write them.
///
/// A raw debug list of paths in the middle of a sentence reads as a fault
/// rather than as an answer, and a home folder is one path on nearly every
/// machine, so the common case is the short one.
fn listed(roots: &[String]) -> String {
    match roots {
        [] => String::new(),
        [one] => one.clone(),
        [one, two] => format!("{one} and {two}"),
        [one, rest @ ..] => format!("{one} and {} more", rest.len()),
    }
}

/// Whether the rest of the machine is searched, and what to do about it.
///
/// Three states and three sentences. Naming the program is a fact about how
/// file search works here rather than a recommendation, and there is no row
/// that installs anything: `P1-15` found this exact row running
/// `winget install` while its own subtitle talked about Sill's index, and a
/// welcome is the last place that should be able to happen.
fn whole_drives(running: bool, installed: bool) -> Step {
    if running {
        return Step {
            id: "everything",
            title: "Whole drive search is already on".to_string(),
            subtitle: "Everything is running, so Sill asks it as well and sees the rest of the \
                       machine."
                .to_string(),
            does: Does::ChooseFolders,
        };
    }

    if installed {
        return Step {
            id: "everything",
            title: "Start Everything".to_string(),
            subtitle: "It is on this machine and not running. While it runs, Sill searches \
                       whole drives through it."
                .to_string(),
            does: Does::StartEverything,
        };
    }

    Step {
        id: "everything",
        title: "Search whole drives too".to_string(),
        subtitle: "Sill searches the folders it indexes. It also asks Everything, a separate \
                   file indexer, whenever that is running, and it is not on this machine."
            .to_string(),
        does: Does::ChooseFolders,
    }
}

/// Whether a refused summon key should also open the settings window.
///
/// It should, normally: with the key taken there is no launcher to read a
/// message in, so `P1-11` opens the window that holds the row instead.
///
/// It should **not** on a first run, because the welcome is about to appear
/// saying the same thing with more room and the fix on it. Two windows opening
/// at once on somebody's first contact with the application, one of them a
/// settings pane they have never seen, is worse than either on its own.
pub fn also_open_settings(taken: bool, first_run: bool) -> bool {
    taken && !first_run
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts<'static> {
        Facts {
            summon: "Alt+Space",
            summon_taken: false,
            tray: true,
            roots: vec![r"C:\Users\someone".to_string()],
            everything_running: false,
            everything_installed: false,
        }
    }

    /// Every sentence the welcome puts on screen, as one piece of text.
    ///
    /// The assertions below are about what a reader is told, and a reader does
    /// not know which field a sentence came out of.
    fn everything_said(welcome: &Welcome) -> String {
        let mut text = format!(
            "{} {} {} {}",
            welcome.opening.headline,
            welcome.opening.body,
            welcome.tray.headline,
            welcome.tray.body
        );

        for step in &welcome.steps {
            text.push(' ');
            text.push_str(&step.title);
            text.push(' ');
            text.push_str(&step.subtitle);
        }

        text
    }

    /// **The one that matters.**
    ///
    /// `Alt+Space` was refused on this machine at every start for weeks, and
    /// the welcome used to say so and send the reader to choose another key.
    /// The keyboard hook now takes a refused key ahead of whoever held it, so
    /// the key is still the key; what the welcome owes the reader is where to
    /// look if it does nothing, because a hook cannot prove it is alive.
    #[test]
    fn a_refused_key_is_still_the_key_and_the_welcome_says_where_to_look() {
        let refused = greeting(&Facts {
            summon_taken: true,
            ..facts()
        });
        let said = everything_said(&refused);

        assert!(said.contains("Press Alt+Space"), "the key still opens Sill: {:?}", refused.opening);
        assert!(said.contains("keyboard hook"), "and the reader is told how");
        assert!(said.contains("does nothing"), "and where to look if it does not");

        let works = greeting(&facts());
        assert!(
            !everything_said(&works).contains("keyboard hook"),
            "a key Windows registered is not explained away"
        );
    }

    /// The key is still named, because somebody has to know which one to change.
    #[test]
    fn a_refused_key_is_named_rather_than_hidden() {
        let refused = greeting(&Facts {
            summon_taken: true,
            ..facts()
        });

        assert!(refused.opening.headline.contains("Alt+Space"));
        assert!(refused.summon_taken);
    }

    /// And fixing it leads the list, because nothing below it can be reached.
    #[test]
    fn a_refused_key_puts_choosing_another_one_first() {
        let refused = greeting(&Facts {
            summon_taken: true,
            ..facts()
        });

        assert_eq!(refused.steps[0].does, Does::ChooseKey);
    }

    /// A key that works needs no row about keys at all.
    #[test]
    fn a_key_that_works_is_not_a_thing_to_go_and_fix() {
        let works = greeting(&facts());
        assert!(!works.steps.iter().any(|step| step.does == Does::ChooseKey));
    }

    /// An empty key is not a refused one, and the two do not read the same.
    ///
    /// "Alt+Space belongs to another application" with no key set would name a
    /// combination nobody chose, and `HotkeyConflicts` cannot hold an empty
    /// accelerator because registering one is never attempted.
    #[test]
    fn no_key_at_all_reads_differently_from_a_refused_one() {
        let none = greeting(&Facts {
            summon: "   ",
            summon_taken: true,
            ..facts()
        });

        assert_eq!(none.opening.headline, "No key opens Sill yet");
        assert!(!none.summon_taken, "an empty key cannot have been taken");
        assert_eq!(none.steps[0].does, Does::ChooseKey);
    }

    /// The tray is explained, and what it says depends on whether it is there.
    #[test]
    fn the_tray_is_described_as_it_actually_is() {
        let shown = greeting(&facts());
        assert!(shown.tray.headline.contains("notification area"));

        let hidden = greeting(&Facts {
            tray: false,
            ..facts()
        });
        assert_ne!(hidden.tray, shown.tray);
        assert!(hidden.tray.headline.contains("no icon"));
    }

    /// Nothing on the welcome installs anything.
    ///
    /// The row this replaces used to run `winget install voidtools.Everything`
    /// while its subtitle talked about Sill's own index, so choosing it opened
    /// a console installing software the row never mentioned. A first run is
    /// the last screen that should be able to do that.
    #[test]
    fn no_row_offers_to_install_anything() {
        for (running, installed) in [(true, true), (false, true), (false, false)] {
            let welcome = greeting(&Facts {
                everything_running: running,
                everything_installed: installed,
                ..facts()
            });

            let said = everything_said(&welcome).to_lowercase();
            assert!(!said.contains("install"), "{said}");
        }
    }

    /// Starting it is only offered when there is something to start.
    #[test]
    fn starting_the_indexer_is_offered_only_when_it_is_there_and_asleep() {
        let asleep = greeting(&Facts {
            everything_installed: true,
            ..facts()
        });
        assert!(
            asleep
                .steps
                .iter()
                .any(|step| step.does == Does::StartEverything),
            "an installed indexer sitting closed was not offered a start"
        );

        for (running, installed) in [(true, true), (false, false)] {
            let welcome = greeting(&Facts {
                everything_running: running,
                everything_installed: installed,
                ..facts()
            });

            assert!(
                !welcome
                    .steps
                    .iter()
                    .any(|step| step.does == Does::StartEverything),
                "offered to start an indexer that is running: {running}, installed: {installed}"
            );
        }
    }

    /// The folders row names what is indexed rather than claiming in general.
    #[test]
    fn the_folders_row_names_what_is_actually_indexed() {
        let one = greeting(&facts());
        let row = one
            .steps
            .iter()
            .find(|step| step.id == "folders")
            .expect("a row about folders");

        assert!(row.subtitle.contains(r"C:\Users\someone"), "{row:?}");

        let none = greeting(&Facts {
            roots: Vec::new(),
            ..facts()
        });
        let row = none
            .steps
            .iter()
            .find(|step| step.id == "folders")
            .expect("a row about folders");

        assert!(row.subtitle.contains("not indexing any folders"), "{row:?}");
    }

    #[test]
    fn several_folders_are_a_sentence_rather_than_a_list() {
        assert_eq!(listed(&[]), "");
        assert_eq!(listed(&["a".to_string()]), "a");
        assert_eq!(listed(&["a".to_string(), "b".to_string()]), "a and b");
        assert_eq!(
            listed(&["a".to_string(), "b".to_string(), "c".to_string()]),
            "a and 2 more"
        );
    }

    /// Every row does something, and leaving is one of them.
    ///
    /// A read-only line in a list somebody is arrowing through is a row that
    /// swallows Enter, which reads as the launcher having stopped responding.
    #[test]
    fn every_row_does_something_and_the_last_one_leaves() {
        let welcome = greeting(&facts());
        assert!(!welcome.steps.is_empty());
        assert_eq!(
            welcome.steps.last().map(|step| step.does),
            Some(Does::Finish)
        );
    }

    /// Two rows with one id would blank the whole list.
    ///
    /// The window keys the rows by id, and a duplicate key in a keyed `{#each}`
    /// throws rather than drawing twice, so the welcome would render as
    /// nothing at all.
    #[test]
    fn no_two_rows_share_an_id() {
        for taken in [true, false] {
            for roots in [Vec::new(), vec!["one".to_string()]] {
                for (running, installed) in [(true, true), (false, true), (false, false)] {
                    let welcome = greeting(&Facts {
                        summon_taken: taken,
                        roots: roots.clone(),
                        everything_running: running,
                        everything_installed: installed,
                        ..facts()
                    });

                    let mut ids: Vec<&str> = welcome.steps.iter().map(|step| step.id).collect();
                    let count = ids.len();
                    ids.sort_unstable();
                    ids.dedup();
                    assert_eq!(ids.len(), count, "{ids:?}");
                }
            }
        }
    }

    /// The settings window is not opened over the top of the welcome.
    #[test]
    fn a_first_run_lets_the_welcome_say_it_rather_than_opening_settings() {
        assert!(also_open_settings(true, false));
        assert!(!also_open_settings(true, true));

        // And a key that registered opens nothing either way.
        assert!(!also_open_settings(false, false));
        assert!(!also_open_settings(false, true));
    }
}
