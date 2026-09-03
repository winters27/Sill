//! One place a failure nobody chose can be read.
//!
//! Sill does a great deal on the user's behalf that has no result to look at.
//! A tray icon is created, a startup entry is written, a copied image is
//! stored beside its entry, a granted permission is written down, a
//! conversation is saved. When one of those does not work there is nothing to
//! notice: the toggle still says on, the list still draws, and the only record
//! is a line in a log nobody has open. `HotkeyConflicts` already solved this
//! shape for one case, a key another application owns; this is the same idea
//! for everything else that fails quietly.
//!
//! ## Why it is a set and not a stream
//!
//! Each trouble has a stable id and the newest wins, so a failure that repeats
//! is one entry rather than a hundred. That matters most for the clipboard: an
//! image blob that cannot be written will fail again on the next copy, and for
//! as long as the disk is full. Something a person cannot act on twice must
//! not be able to say so twice.
//!
//! It is also why nothing here raises a toast. A trouble is a state the user
//! can go and read, not an interruption, and it stops being reported the
//! moment the thing works.
//!
//! ## What belongs here
//!
//! Only a failure that leaves the application quietly not doing what its own
//! interface says it is doing. A refusal the user made, an error a command
//! already returned to the window that asked for it, and anything already
//! visible where it happened, all stay out. The value of this surface is that
//! it is empty almost always.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Something Sill tried to do for the user and could not.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Trouble {
    /// Stable across repeats of the same failure, so it replaces rather than
    /// accumulates, and so the thing that starts working can withdraw it.
    pub id: String,
    /// One sentence, in the words somebody who did not write Sill would use.
    pub message: String,
    /// The settings section holding the control this is about, when there is
    /// one, so the surface showing it can offer to go there.
    pub section: Option<String>,
}

/// How many can be held at once.
///
/// A bound rather than a cap that matters in practice: there are nine places
/// that report, and reaching this would mean nearly all of them are failing at
/// once. It exists because the settings window can report too, and nothing
/// that grows from a message should grow without end.
const MOST: usize = 32;

/// Everything currently not working, for whoever is in a position to show it.
///
/// A managed service rather than a `static`, which is what rule 2 refuses, and
/// the same shape as `HotkeyConflicts` next door.
#[derive(Default)]
pub struct Status {
    troubles: Mutex<BTreeMap<String, Trouble>>,
}

impl Status {
    /// Records one, and says whether that changed anything.
    ///
    /// The answer is what stops an event being emitted per failed clipboard
    /// image. The identical trouble arriving again is not news.
    pub fn note(&self, id: &str, message: String, section: Option<&str>) -> bool {
        let trouble = Trouble {
            id: id.to_string(),
            message,
            section: section.map(str::to_string),
        };

        // Poisoning is recovered from rather than swallowed. A surface whose
        // whole job is to report failure must not be the thing that goes
        // quiet, and the map is only ever left mid-insert.
        let mut held = self.troubles.lock().unwrap_or_else(|e| e.into_inner());

        if held.get(id) == Some(&trouble) {
            return false;
        }

        if held.len() >= MOST && !held.contains_key(id) {
            return false;
        }

        held.insert(id.to_string(), trouble);
        true
    }

    /// Withdraws one, and says whether there was anything to withdraw.
    ///
    /// Called when the thing works, because a trouble that outlives its cause
    /// is worse than none: it teaches the reader that the surface is noise.
    pub fn clear(&self, id: &str) -> bool {
        self.troubles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
            .is_some()
    }

    /// Withdraws everything whose id begins with the given text.
    ///
    /// For the group the settings window owns: it re-reads all of them at
    /// once, so whatever it found last time is stale before the first answer
    /// arrives, and clearing them one by one would need the window to know
    /// which ones it reported.
    pub fn clear_group(&self, prefix: &str) -> bool {
        let mut held = self.troubles.lock().unwrap_or_else(|e| e.into_inner());
        let before = held.len();
        held.retain(|id, _| !id.starts_with(prefix));
        held.len() != before
    }

    /// Everything not working, in a stable order.
    pub fn all(&self) -> Vec<Trouble> {
        self.troubles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }
}

/// The prefix every "a window could not read this" report shares.
const UNREADABLE: &str = "unreadable:";

/// Which window is reporting.
///
/// Scoped per window rather than one flat group, because each of them clears
/// what it last failed to read before asking again, and a single group would
/// mean whichever window opened last wiped the others. Opening settings to
/// read a trouble would have been the act that erased it.
///
/// An enum rather than a string the page chooses, so the ids stay a known set
/// and a renderer cannot invent them.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Surface {
    Launcher,
    Settings,
    Ask,
    Capture,
}

impl Surface {
    fn prefix(self) -> String {
        let name = match self {
            Surface::Launcher => "launcher",
            Surface::Settings => "settings",
            Surface::Ask => "ask",
            Surface::Capture => "capture",
        };

        format!("{UNREADABLE}{name}:")
    }
}

/// Records that a window could not read something it needs from Rust.
///
/// Its own function rather than a `report` with a built-up id, so that every
/// `report` call in the tree passes a plain name and `verify-source` can check
/// each one against the `resolved` that withdraws it. A window's whole group
/// is withdrawn together, by that window, when it is about to ask again.
pub fn unreadable(app: &AppHandle, surface: Surface, what: &str, reason: &str, section: &str) {
    report(
        app,
        &format!("{}{what}", surface.prefix()),
        format!(
            "Sill could not read {what}, so what is shown for it is empty rather than \
             right: {reason}"
        ),
        // Empty when the failure belongs to no particular panel, which is
        // most of them: the settings window's own reads are about the window
        // the reader is already in.
        (!section.is_empty()).then_some(section),
    );
}

/// Forgets what one window last failed to read.
pub fn readable_again(app: &AppHandle, surface: Surface) {
    resolved_group(app, &surface.prefix());
}

/// Records a failure, logs it, and tells anything showing them.
///
/// The logging is not separate from the reporting on purpose. Every site this
/// replaces already had a `say!` and the log is still the only place with a
/// timestamp and an ordering, which is what a bug report needs; the surface is
/// what the user needs. Doing both from one call is what stops the two from
/// drifting apart the way they did for the summon key.
pub fn report(app: &AppHandle, id: &str, message: impl Into<String>, section: Option<&str>) {
    let message = message.into();
    crate::say!("{message}");

    // Absent in the tests that drive a command without a full application, and
    // before `setup` manages it. Logging has already happened either way.
    let Some(status) = app.try_state::<Status>() else {
        return;
    };

    if status.note(id, message, section) {
        announce(app, &status.all());
    }
}

/// Says a thing that was failing is working again.
pub fn resolved(app: &AppHandle, id: &str) {
    let Some(status) = app.try_state::<Status>() else {
        return;
    };

    if status.clear(id) {
        announce(app, &status.all());
    }
}

/// Puts whatever is currently wrong back on the surfaces that show it.
///
/// For the tray, which is built with its own tooltip and so forgets anything
/// reported before it existed. Creating it is one of the things that can fail
/// here, and turning it back on afterwards must not be the act that hides the
/// rest.
pub fn refresh(app: &AppHandle) {
    let Some(status) = app.try_state::<Status>() else {
        return;
    };

    announce(app, &status.all());
}

/// Withdraws a whole group, for the window that re-reads all of it at once.
pub fn resolved_group(app: &AppHandle, prefix: &str) {
    let Some(status) = app.try_state::<Status>() else {
        return;
    };

    if status.clear_group(prefix) {
        announce(app, &status.all());
    }
}

/// Puts the current state where somebody will meet it.
///
/// Two places, because they answer different questions. The settings window
/// gets an event and draws the sentences, which is where somebody who is
/// already looking for the problem will be. The tray tooltip gets one line,
/// which is for the person who is not looking: the tray is the only sign Sill
/// is running at all, so it is where a launcher that seems fine but is not
/// gets to say so without interrupting anybody.
fn announce(app: &AppHandle, troubles: &[Trouble]) {
    let _ = app.emit("sill://status-changed", troubles);

    let Some(tray) = app.tray_by_id(crate::TRAY_ID) else {
        // No tray, which is itself one of the things that can go wrong here.
        // The settings surface is the one that still works then.
        return;
    };

    let _ = tray.set_tooltip(Some(&tooltip(troubles)));
}

/// The one line the tray gets.
///
/// A tooltip is a single short string, so several troubles become a count and
/// somewhere to look rather than a list that would be cut off mid-sentence.
fn tooltip(troubles: &[Trouble]) -> String {
    match troubles {
        [] => "Sill".to_string(),
        [one] => format!("Sill: {}", one.message),
        many => format!(
            "Sill: {} things are not working. Open settings to see them.",
            many.len()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{Status, Trouble};

    /// A failure that repeats is one entry, not a hundred.
    ///
    /// The clipboard is why. A blob that cannot be written will fail on the
    /// next copy too, and on every copy after that until the disk has room. A
    /// surface that grew a row per attempt would be unreadable within a minute
    /// of the first one, which is the house rule against toast spam arriving
    /// from the other direction.
    #[test]
    fn the_same_failure_reported_twice_is_one_trouble() {
        let status = Status::default();

        assert!(status.note("clipboard-image", "no room".to_string(), None));
        assert!(
            !status.note("clipboard-image", "no room".to_string(), None),
            "an identical repeat was treated as news, so it would emit and redraw"
        );

        assert_eq!(status.all().len(), 1);
    }

    /// The newest wording of a failure replaces the last one.
    ///
    /// The same id with a different reason is the same broken thing, described
    /// better. Two rows for it would read as two problems.
    #[test]
    fn a_changed_reason_replaces_rather_than_adds() {
        let status = Status::default();

        status.note("grants", "no room".to_string(), None);
        assert!(status.note("grants", "access denied".to_string(), None));

        assert_eq!(
            status.all(),
            vec![Trouble {
                id: "grants".to_string(),
                message: "access denied".to_string(),
                section: None,
            }]
        );
    }

    /// A thing that starts working stops being reported.
    ///
    /// The stale-entry bug `HotkeyConflicts` had to grow a test for, and for
    /// the same reason: a surface that keeps saying a fixed thing is broken is
    /// one people learn to ignore, which costs more than never having built
    /// it.
    #[test]
    fn a_trouble_that_is_fixed_stops_being_reported() {
        let status = Status::default();

        status.note("autostart", "could not write it".to_string(), None);
        assert!(status.clear("autostart"));
        assert!(status.all().is_empty(), "a fixed trouble is still reported");

        assert!(
            !status.clear("autostart"),
            "clearing nothing claimed to have changed something, so it would emit"
        );
    }

    /// One broken thing does not hide another.
    #[test]
    fn every_trouble_is_reported_not_just_the_last() {
        let status = Status::default();

        status.note("tray", "no icon".to_string(), None);
        status.note("autostart", "denied".to_string(), None);

        let ids: Vec<String> = status.all().into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["autostart".to_string(), "tray".to_string()]);
    }

    /// The settings window's own reports are withdrawn as a group.
    ///
    /// It re-reads every one of them each time it opens, so what it found last
    /// time is stale before the first answer arrives. Clearing them
    /// individually would mean the window had to remember which ones it had
    /// reported, which is the duplicate state rule 5 refuses.
    #[test]
    fn a_group_is_withdrawn_together_and_leaves_the_rest() {
        let status = Status::default();

        status.note("unreadable:the search engines", "denied".to_string(), None);
        status.note("unreadable:the browsers", "denied".to_string(), None);
        status.note("tray", "no icon".to_string(), None);

        assert!(status.clear_group("unreadable:"));

        let ids: Vec<String> = status.all().into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["tray".to_string()]);

        assert!(
            !status.clear_group("unreadable:"),
            "clearing an empty group claimed to have changed something"
        );
    }

    /// Nothing that reports can make this grow without end.
    ///
    /// The settings window supplies ids from its own list of things to read,
    /// which is fixed, but a bound belongs on anything a message can add to.
    /// An existing trouble is still updated once it is full, because refusing
    /// that would freeze the surface on stale wording.
    #[test]
    fn the_set_is_bounded_and_still_updates_what_it_holds() {
        let status = Status::default();

        for at in 0..super::MOST + 8 {
            status.note(&format!("trouble-{at}"), "broken".to_string(), None);
        }

        assert_eq!(status.all().len(), super::MOST);

        assert!(
            status.note("trouble-0", "broken differently".to_string(), None),
            "a full set stopped updating the troubles it already holds"
        );
    }

    /// The tray gets one line whatever is wrong.
    ///
    /// A tooltip is a single short string. Several troubles have to become a
    /// count and somewhere to look, because a list would be cut off in the
    /// middle of a sentence and read as a rendering fault.
    #[test]
    fn the_tray_line_stays_one_line() {
        assert_eq!(super::tooltip(&[]), "Sill");

        let one = Trouble {
            id: "tray".to_string(),
            message: "the startup entry could not be written".to_string(),
            section: None,
        };
        assert_eq!(
            super::tooltip(std::slice::from_ref(&one)),
            "Sill: the startup entry could not be written"
        );

        let two = vec![one.clone(), one];
        assert_eq!(
            super::tooltip(&two),
            "Sill: 2 things are not working. Open settings to see them."
        );
    }
}
