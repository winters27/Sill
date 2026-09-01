//! What Sill has done, and what of it can be taken back.
//!
//! P2.1. Undo existed before this and lived in the window: an action handed
//! back a descriptor, the page held it, and Ctrl+Z sent it home again. That
//! works for exactly as long as the launcher is open, which is seconds. Close
//! it and the last thing you did stops being undoable, and the one moment
//! somebody wants undo is the moment after they have looked away.
//!
//! So the record lives in Rust and outlives the window. **Nothing here holds
//! what was changed**, only descriptors: two paths for a move, a rectangle for
//! a window, the previous text for a clipboard write. Undoing a move of a ten
//! gigabyte folder costs what undoing a move of a text file costs, and an
//! activity log cannot become the place deleted things quietly live.
//!
//! ## Why it is not written to disk
//!
//! An undo token is only good while the world it describes still holds. A
//! window id from last Tuesday names a window that has gone; a clipboard
//! restore from before a reboot puts back something nobody remembers copying.
//! Offering those after a restart would be offering to do something
//! unpredictable, so the log is per-run and says so.

use std::sync::Mutex;

use serde::Serialize;

use crate::action::{ActionCtx, Outcome, Undo};

/// Remembers one finished action.
///
/// Takes the whole outcome rather than its pieces, so a field added to an
/// outcome cannot be silently left out of the record.
pub fn record(ctx: &ActionCtx, action: &str, target: &str, outcome: &Outcome) {
    use tauri::Manager;

    let Some(log) = ctx.app.try_state::<Activity>() else {
        return;
    };

    log.record(
        action,
        target,
        &outcome.message,
        outcome.undo.clone(),
        crate::state::now_seconds(),
    );
}

/// How many actions are remembered.
///
/// Enough to find the one you meant after a few more have happened, and not a
/// scrollback. Anything older than this is not something anybody is still
/// trying to reverse.
const KEEP: usize = 50;

/// One thing that happened.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    /// Rises forever within a run, so the window can name one entry without
    /// depending on its position in a list that keeps growing at the front.
    pub id: u64,
    /// The action's title, as the action panel spells it.
    pub action: String,
    /// What it was done to.
    pub target: String,
    /// What the action said afterwards, which is the past-tense line the
    /// launcher already showed once.
    pub message: String,
    /// Unix seconds.
    pub at: i64,
    /// Whether this one can still be taken back.
    ///
    /// The descriptor itself never crosses to the window. It is an instruction
    /// to change the machine, and the window's copy of one is a thing that can
    /// be replayed, edited or held after the log has moved on.
    pub undoable: bool,
}

/// One entry, with the half the window never sees.
#[derive(Debug, Clone)]
struct Entry {
    done: Done,
    undo: Option<Undo>,
}

/// Everything this run has done.
#[derive(Default)]
pub struct Activity {
    inner: Mutex<Log>,
}

#[derive(Default)]
struct Log {
    entries: Vec<Entry>,
    next_id: u64,
}

impl Activity {
    /// Remembers something that happened.
    pub fn record(&self, action: &str, target: &str, message: &str, undo: Option<Undo>, at: i64) {
        let Ok(mut log) = self.inner.lock() else {
            // A poisoned log is not worth failing an action that already
            // succeeded. The thing was done; only the record of it is lost.
            return;
        };

        log.next_id += 1;
        let id = log.next_id;

        log.entries.push(Entry {
            done: Done {
                id,
                action: action.to_string(),
                target: target.to_string(),
                message: message.to_string(),
                at,
                undoable: undo.is_some(),
            },
            undo,
        });

        // Oldest first in the vector, so trimming takes from the front.
        let over = log.entries.len().saturating_sub(KEEP);
        log.entries.drain(..over);
    }

    /// What happened, newest first.
    pub fn recent(&self) -> Vec<Done> {
        let Ok(log) = self.inner.lock() else {
            return Vec::new();
        };

        log.entries.iter().rev().map(|e| e.done.clone()).collect()
    }

    /// Takes one entry's undo descriptor, if it has one.
    ///
    /// **Taken rather than read.** An undo is spent by being used: leaving the
    /// descriptor in place would offer to move a file back to a folder it is
    /// already in, or to restore a clipboard twice, and the second press of a
    /// button that worked once should not quietly do something else.
    pub fn take(&self, id: u64) -> Option<Undo> {
        let mut log = self.inner.lock().ok()?;
        let entry = log.entries.iter_mut().find(|e| e.done.id == id)?;

        entry.done.undoable = false;
        entry.undo.take()
    }

    /// Takes the most recent undo there is, for "undo the last thing".
    pub fn take_last(&self) -> Option<(u64, Undo)> {
        let mut log = self.inner.lock().ok()?;
        let entry = log.entries.iter_mut().rev().find(|e| e.undo.is_some())?;

        entry.done.undoable = false;
        let id = entry.done.id;
        entry.undo.take().map(|undo| (id, undo))
    }

    /// Forgets everything.
    pub fn clear(&self) {
        if let Ok(mut log) = self.inner.lock() {
            log.entries.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn clipboard_undo(text: &str) -> Undo {
        Undo::RestoreClipboard { text: text.to_string() }
    }

    fn log() -> Activity {
        Activity::default()
    }

    #[test]
    fn the_newest_thing_is_first() {
        let log = log();
        log.record("Copy Path", "a.txt", "Copied", None, NOW);
        log.record("Move To", "b.txt", "Moved", None, NOW + 1);

        let recent = log.recent();
        assert_eq!(recent[0].target, "b.txt");
        assert_eq!(recent[1].target, "a.txt");
    }

    /// An undo is spent by being used.
    #[test]
    fn taking_an_undo_leaves_nothing_to_take_twice() {
        let log = log();
        log.record("Copy", "x", "Copied", Some(clipboard_undo("before")), NOW);

        let id = log.recent()[0].id;

        assert!(log.take(id).is_some(), "the first take should find it");
        assert!(
            log.take(id).is_none(),
            "a second take would undo something already undone"
        );
        assert!(
            !log.recent()[0].undoable,
            "and the row must stop offering it"
        );
    }

    /// "Undo the last thing" means the last *undoable* thing, not the last
    /// thing: launching an application is not undoable and must not shadow the
    /// file move before it.
    #[test]
    fn the_last_undo_skips_everything_that_cannot_be_undone() {
        let log = log();
        log.record("Move To", "report.pdf", "Moved", Some(clipboard_undo("x")), NOW);
        log.record("Open", "Firefox", "Opened", None, NOW + 1);
        log.record("Reveal", "notes.md", "Revealed", None, NOW + 2);

        let (id, _) = log.take_last().expect("the move is still undoable");
        assert_eq!(
            log.recent().iter().find(|d| d.id == id).unwrap().target,
            "report.pdf"
        );
    }

    #[test]
    fn nothing_undoable_is_nothing_to_take() {
        let log = log();
        log.record("Open", "Firefox", "Opened", None, NOW);

        assert!(log.take_last().is_none());
    }

    /// The log is bounded, and it is the oldest that goes.
    #[test]
    fn only_the_last_fifty_are_kept() {
        let log = log();
        for i in 0..KEEP + 20 {
            log.record("Copy", &format!("{i}"), "Copied", None, NOW + i as i64);
        }

        let recent = log.recent();
        assert_eq!(recent.len(), KEEP);
        assert_eq!(recent[0].target, format!("{}", KEEP + 19), "newest kept");
        assert_eq!(recent[KEEP - 1].target, "20", "oldest twenty dropped");
    }

    /// Ids name an entry for the window, so they must not be reused as the
    /// log rolls: a position would move under the row it names.
    #[test]
    fn an_id_is_never_handed_out_twice() {
        let log = log();
        for i in 0..KEEP + 5 {
            log.record("Copy", &format!("{i}"), "Copied", None, NOW);
        }

        let mut ids: Vec<u64> = log.recent().iter().map(|d| d.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), before, "an id was handed out twice");
    }
}
