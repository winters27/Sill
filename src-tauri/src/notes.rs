/*!
Notes, as a prototype behind a switch that is off.

## What this is, and what it deliberately is not

One object kind and one window. There is no folder tree, no tags, no
formatting, no linking and no second pane. `P3-11` asked for a prototype and
the shortest way to turn a prototype into an application nobody finished is to
start adding the parts a note-taking application has.

What it is instead: somewhere to put a paragraph you would otherwise lose,
findable by typing `note` and whatever you remember of it. Everything else is
somebody else's item, and until somebody has actually lived with this one
there is no honest way to know which parts are wanted.

## Why it is off

Rule 23 asks what a feature costs while nothing is happening, and the honest
answer for an unfinished one is that it should cost nothing at all. With
[`crate::preferences::General::notes`] off, [`matched`] answers `None` on a
boolean and the file is never opened, never read and never held: a keystroke
pays one `bool` for a feature that is not switched on. With it on, the file is
read once and kept, and only when the first word of a query is one of the four
words below.

## Where the text lives, and what happens to it

`notes.json`, in Sill's own data folder, through [`crate::json_store`] like
every other store: a staged write so a torn one cannot lose the file, a byte
order mark skipped so opening it in Notepad and saving does not empty it, and a
schema version so a file from a newer build is kept rather than reinterpreted.
The folder is already the one `leavings.rs` names, so a new file in it changes
nothing about uninstalling.

**One damaged note costs one note.** `json_store::load_list` reads the file
entry by entry and keeps every entry it can, which matters more here than
anywhere else it is used: a snippet or a quicklink can be typed again, and
somebody's own paragraph cannot. Every field also carries a default, so a note
written by a build that spells something differently still reads; the only way
to lose one is for its entry not to be an object at all.

**A file that cannot be read at all is said out loud.** Every other store is
content to keep the file aside and log it, because starting from defaults looks
like starting from defaults. A notes window that draws nothing looks exactly
like a notes window nobody has written in yet, so the one case where somebody
must be told is this one: [`Notes::all`] reports it on the status surface,
naming the file that was kept, rather than letting a person conclude their
writing was never there.

## What ticks

Nothing. There is no timer, no watcher and no thread here. The file is read
when a query asks for a note and written when one is edited, and between those
two moments this module is a `Mutex` holding a `Vec` that may well be `None`.
*/

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::json_store;

/// One note.
///
/// The text is the whole of it. A title is not stored because a stored title
/// and a written first line are two answers to "what is this note called", and
/// the one that goes stale is whichever the editor forgets to update. See
/// [`Self::title`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Note {
    /// Stable for the life of the note. What a row carries and what the window
    /// is opened on, so editing the first line does not make it a new note.
    pub id: String,
    /// What somebody wrote, exactly as they wrote it.
    pub text: String,
    /// Unix seconds.
    pub created: i64,
    /// Unix seconds, which is what the list is ordered by.
    pub updated: i64,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            id: String::new(),
            text: String::new(),
            created: 0,
            updated: 0,
        }
    }
}

/// How much of a note a row can carry.
const GLANCE: usize = 80;

impl Note {
    /**
    What to call it, which is the first line somebody wrote.

    Derived rather than stored, so there is nothing to keep in step. A note
    with nothing in it yet still needs a name on a row, and "Empty note" is a
    truer one than a blank.
    */
    pub fn title(&self) -> String {
        let first = self
            .text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or_default();

        if first.is_empty() {
            return "Empty note".to_string();
        }

        shortened(first)
    }

    /// The rest of it, as one line under the title.
    ///
    /// Skips the line the title came from, so a row does not say the same
    /// thing twice, and flattens what is left so a note full of blank lines
    /// still says something about itself.
    pub fn glance(&self) -> String {
        let mut lines = self.text.lines().map(str::trim).filter(|l| !l.is_empty());
        lines.next();

        shortened(&lines.collect::<Vec<_>>().join(" "))
    }

    /// Whether the words somebody typed are anywhere in this note.
    ///
    /// The whole text rather than the first line, because the reason to look
    /// for a note is usually something in the middle of it.
    fn holds(&self, wanted: &str) -> bool {
        wanted.is_empty() || self.text.to_lowercase().contains(wanted)
    }
}

/// One line, cut to something a row can hold.
fn shortened(text: &str) -> String {
    let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");

    if flattened.chars().count() > GLANCE {
        format!(
            "{}\u{2026}",
            flattened.chars().take(GLANCE).collect::<String>()
        )
    } else {
        flattened
    }
}

/**
The words that ask for a note, and nothing else does.

Exact, and the first word only, which is the gate `media` and `terminals`
already use. A note whose text contains "meeting" must not be found by typing
"meeting", because then every search anybody types is also a search of their
private writing, and a launcher that puts a paragraph from a diary underneath
an application is a launcher nobody opens in front of other people.
*/
const ASKING: &[&str] = &["note", "notes", "scratch", "scratchpad"];

/// What was asked for, if a note was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// The notes that match, newest first.
    pub found: Vec<Note>,
}

/**
Whether this query is asking for notes, and which ones.

`None` costs a `bool` and, when notes are on, one `split_whitespace` and up to
four string comparisons. The reading closure is handed in rather than taken so
that a query which is not asking never opens the file, which is the same shape
`media::matched` and `terminals::matched` use and the same reason.
*/
pub fn matched(query: &str, enabled: bool, read: impl FnOnce() -> Vec<Note>) -> Option<Asked> {
    if !enabled {
        return None;
    }

    let mut words = query.split_whitespace();
    let first = words.next()?.to_lowercase();

    if !ASKING.contains(&first.as_str()) {
        return None;
    }

    let wanted = words.collect::<Vec<_>>().join(" ").to_lowercase();

    let mut found: Vec<Note> = read()
        .into_iter()
        .filter(|note| note.holds(&wanted))
        .collect();

    found.sort_by_key(|note| std::cmp::Reverse(note.updated));

    Some(Asked { found })
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("notes.json")
}

/// How the file is kept. See `json_store` for what each part buys.
///
/// Readable, because the whole file is somebody's own writing and the one
/// thing worth being sure of is that they can open it and get their words back
/// without Sill's help.
const SCHEMA: json_store::Schema = json_store::Schema {
    version: 1,
    shape: json_store::Shape::Around,
    layout: json_store::Layout::Readable,
    unreadable: json_store::Unreadable::KeepAside,
    what: "notes",
};

/// Where a failure to read the file is filed.
const UNREADABLE: &str = "notes:unreadable";

/**
The notes, once anybody has asked for them.

A managed service rather than a `static`, which is rule 2, and lazy, which is
rule 23: a machine where notes are switched off never constructs the `Vec`, and
one where they are on constructs it the first time a query begins with the
word.
*/
#[derive(Default)]
pub struct Notes {
    held: Mutex<Vec<Note>>,
    /// Whether what is on disk has been read.
    ///
    /// The same guard `ai::chat::Chat` carries, for the same reason. Saving
    /// writes what is in memory over the file, and before a load there is
    /// nothing in memory: one save that got in first would replace every note
    /// with an empty list, and nothing about that failure looks like a failure.
    read_the_file: AtomicBool,
}

impl Notes {
    /// Everything, newest first, reading the file the first time it is asked.
    pub fn all(&self, app: &AppHandle) -> Vec<Note> {
        self.load(app);

        let mut all = self
            .held
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone();
        all.sort_by_key(|note| std::cmp::Reverse(note.updated));
        all
    }

    /// One, by the id a row carries.
    pub fn one(&self, app: &AppHandle, id: &str) -> Option<Note> {
        self.load(app);

        self.held
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .iter()
            .find(|note| note.id == id)
            .cloned()
    }

    /**
    Writes a note, making it if the id names none.

    One entry point for both, because "save what is in the window" is one act
    whether or not the note existed a moment ago, and a separate `create` would
    be a second place that has to remember to stamp the times.
    */
    pub fn write(&self, app: &AppHandle, id: &str, text: &str, now: i64) -> Result<Note, String> {
        self.load(app);

        let note = {
            let mut held = self.held.lock().unwrap_or_else(|held| held.into_inner());

            match held.iter_mut().find(|note| note.id == id) {
                Some(found) => {
                    found.text = text.to_string();
                    found.updated = now;
                    found.clone()
                }
                None => {
                    let made = Note {
                        id: fresh_id(&held, now),
                        text: text.to_string(),
                        created: now,
                        updated: now,
                    };
                    held.push(made.clone());
                    made
                }
            }
        };

        self.save(app)?;
        Ok(note)
    }

    /// Removes one, and says whether there was one to remove.
    pub fn forget(&self, app: &AppHandle, id: &str) -> Result<bool, String> {
        self.load(app);

        let removed = {
            let mut held = self.held.lock().unwrap_or_else(|held| held.into_inner());
            let was = held.len();
            held.retain(|note| note.id != id);
            held.len() != was
        };

        if removed {
            self.save(app)?;
        }

        Ok(removed)
    }

    /// Reads the file, once.
    fn load(&self, app: &AppHandle) {
        if self.read_the_file.swap(true, Ordering::SeqCst) {
            return;
        }

        let file = path(&data_dir(app));
        let broken = file.with_extension("json.broken");
        let was_broken = broken.exists();

        *self.held.lock().unwrap_or_else(|held| held.into_inner()) = read(&file);

        /*
         * The one store that says so on screen rather than only in the log.
         *
         * A settings file that could not be read shows its defaults, and
         * defaults look like defaults. An empty notes window looks exactly
         * like a notes window nobody has written in, so the failure is
         * indistinguishable from the ordinary first run unless something says
         * otherwise. The file itself is still on disk under another name,
         * which is the whole point of keeping it aside, and this is what
         * tells somebody to go and look at it.
         */
        if !was_broken && broken.exists() {
            crate::status::report(
                app,
                UNREADABLE,
                format!(
                    "Sill could not read your notes, so it has kept the file as {} \
                     and started an empty one. Nothing was overwritten.",
                    broken.display()
                ),
                None,
            );

            return;
        }

        // A clean read withdraws whatever the last one said. A report that
        // outlives the fault is a surface that stops meaning anything, and
        // this one is reported on a load that happens once a session, so
        // without this it would still be on screen a week later.
        crate::status::resolved(app, UNREADABLE);
    }

    /// Writes the whole file.
    fn save(&self, app: &AppHandle) -> Result<(), String> {
        // Never before a load. See `read_the_file`.
        debug_assert!(self.read_the_file.load(Ordering::SeqCst));

        let all = self
            .held
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clone();

        json_store::save_atomic(&path(&data_dir(app)), &all, &SCHEMA)
            .map_err(|why| format!("could not save that note: {why}"))
    }
}

/**
Reads the file, keeping every note it can.

A free function rather than a line inside [`Notes::load`], because the reading
is the part worth testing and `Notes::load` needs an `AppHandle` that only a
running Tauri application can make.

**The extraction moves the untested part rather than removing it**, which is a
trap this project has paid for: a test that calls `json_store::load_list`
itself proves `json_store` works and says nothing about whether this store uses
it, exactly as three tests of clipboard pruning said nothing about whether
anything called the prune. So `verify:source` refuses a whole-document `load`
in this file at all, which is the wiring the tests below cannot hold.
*/
fn read(file: &Path) -> Vec<Note> {
    json_store::load_list(file, &SCHEMA)
}

pub fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// An id nothing else in the file already answers to.
///
/// The clock plus a counter rather than a random value, because two notes made
/// in the same second are the only collision there is and a suffix settles it
/// without pulling in a generator.
fn fresh_id(existing: &[Note], now: i64) -> String {
    let mut at = 0u32;

    loop {
        let id = if at == 0 {
            format!("note-{now}")
        } else {
            format!("note-{now}-{at}")
        };

        if !existing.iter().any(|note| note.id == id) {
            return id;
        }

        at += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: &str, text: &str, updated: i64) -> Note {
        Note {
            id: id.to_string(),
            text: text.to_string(),
            created: updated,
            updated,
        }
    }

    fn three() -> Vec<Note> {
        vec![
            note("a", "Milk and bread\nfrom the corner shop", 100),
            note("b", "Deploy notes\nrun the migration first", 300),
            note("c", "", 200),
        ]
    }

    /// Off means the file is never opened, which is the whole of what this
    /// feature costs somebody who has not switched it on.
    #[test]
    fn a_switch_that_is_off_reads_nothing() {
        let mut read = false;

        let answered = matched("note", false, || {
            read = true;
            three()
        });

        assert_eq!(answered, None);
        assert!(!read, "the notes file was opened for somebody who has none");
    }

    /// A query that is not asking for a note reads nothing either, even with
    /// the switch on.
    ///
    /// `note taking app` is deliberately not on this list. Its first word is
    /// the word, so it does open the file, and that is the cost of a gate on
    /// one word rather than on a whole phrase. It is the right side to be
    /// wrong on: the other one is a launcher that searches somebody's writing
    /// on every keystroke.
    #[test]
    fn an_ordinary_search_never_opens_the_file() {
        let mut readings = 0;

        for query in ["notepad", "chrome", "", "n", "my notes", "1 + 1"] {
            let _ = matched(query, true, || {
                readings += 1;
                three()
            });
        }

        assert_eq!(
            readings, 0,
            "a query that is not asking for notes read the file"
        );
    }

    /// Only the first word, and only these four.
    #[test]
    fn the_word_is_the_gate() {
        for word in ["note", "notes", "scratch", "scratchpad", "NOTES", "Note"] {
            assert!(
                matched(word, true, three).is_some(),
                "{word} does not ask for notes"
            );
        }

        // "my notes" has the word second, which is somebody searching for
        // something else that happens to contain it.
        assert_eq!(matched("my notes", true, three), None);
        assert_eq!(matched("noted", true, three), None);
    }

    /// The words after it narrow, and they look at the whole note.
    #[test]
    fn what_follows_the_word_looks_inside_the_note() {
        let asked = matched("note migration", true, three).expect("asked for notes");

        assert_eq!(asked.found.len(), 1);
        assert_eq!(asked.found[0].id, "b");
    }

    /// Nothing matching is still a question that was asked, so the window can
    /// offer a new one rather than showing an ordinary empty search.
    #[test]
    fn asking_for_something_that_is_not_there_is_still_asking() {
        let asked = matched("note nothing like this", true, three).expect("asked");
        assert!(asked.found.is_empty());
    }

    /// Newest first, because a scratchpad is read from the end.
    #[test]
    fn the_one_touched_last_is_offered_first() {
        let asked = matched("note", true, three).expect("asked");

        assert_eq!(
            asked
                .found
                .iter()
                .map(|n| n.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "c", "a"],
        );
    }

    /// A title is the first line and nothing is stored to disagree with it.
    #[test]
    fn a_note_is_called_whatever_its_first_line_says() {
        assert_eq!(note("a", "Milk\nand bread", 0).title(), "Milk");
        assert_eq!(note("a", "\n\n  Milk  \nbread", 0).title(), "Milk");
        assert_eq!(note("a", "", 0).title(), "Empty note");
        assert_eq!(note("a", "   \n  ", 0).title(), "Empty note");
    }

    /// The row says something under the title, and never the title again.
    #[test]
    fn the_subtitle_is_the_rest_of_it() {
        assert_eq!(
            note("a", "Milk\nand bread\nand jam", 0).glance(),
            "and bread and jam"
        );
        assert_eq!(note("a", "Milk", 0).glance(), "");
    }

    /// A row is a row, so a note nobody meant to write a wall of text into
    /// still fits on one.
    #[test]
    fn a_very_long_line_is_cut_rather_than_drawn_whole() {
        let long = "x".repeat(400);
        let title = note("a", &long, 0).title();

        assert!(title.chars().count() <= GLANCE + 1, "{}", title.len());
        assert!(title.ends_with('\u{2026}'));
    }

    /// Two notes made in the same second are two notes.
    #[test]
    fn an_id_is_never_reused() {
        let mut all = Vec::new();

        for _ in 0..3 {
            let id = fresh_id(&all, 1000);
            assert!(!all.iter().any(|n: &Note| n.id == id), "{id} was reused");
            all.push(note(&id, "", 1000));
        }

        assert_eq!(
            all.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
            ["note-1000", "note-1000-1", "note-1000-2"],
        );
    }

    /// One entry that cannot be read costs that entry and nothing else.
    ///
    /// The reason this store is a list read through `load_list`. A snippet or
    /// a quicklink can be typed again; a paragraph somebody wrote cannot, so
    /// the file losing every note over one of them is the failure worth
    /// spending a test on.
    #[test]
    fn one_damaged_note_does_not_cost_the_others() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = path(dir.path());

        std::fs::write(
            &file,
            r#"{"version":1,"items":[
                {"id":"a","text":"kept","created":1,"updated":1},
                "this is not a note at all",
                {"id":"c","text":"also kept","created":2,"updated":2}
            ]}"#,
        )
        .expect("writes");

        let all = read(&file);

        assert_eq!(all.len(), 2, "one bad entry took the others with it");
        assert_eq!(all[0].text, "kept");
        assert_eq!(all[1].text, "also kept");
    }

    /// A note missing a field this build expects is still that person's words.
    #[test]
    fn a_note_written_by_another_build_still_reads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = path(dir.path());

        std::fs::write(
            &file,
            r#"{"version":1,"items":[{"text":"only the words"}]}"#,
        )
        .expect("writes");

        let all = read(&file);

        assert_eq!(all.len(), 1, "a missing field lost somebody's writing");
        assert_eq!(all[0].text, "only the words");
    }

    /// A file that cannot be read at all is kept, never written over.
    #[test]
    fn an_unreadable_file_is_kept_beside_itself() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = path(dir.path());

        std::fs::write(&file, "{ this is not json").expect("writes");

        let all = read(&file);

        assert!(all.is_empty());
        assert_eq!(
            std::fs::read_to_string(file.with_extension("json.broken")).expect("kept aside"),
            "{ this is not json",
            "somebody's notes were not kept"
        );
    }
}
