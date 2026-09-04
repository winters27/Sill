//! Global shortcuts that run an action without the launcher appearing.
//!
//! This is the point of having built an action registry. A shortcut does not
//! get its own implementation of anything: it names an action and says what to
//! run it against, and the same code that runs behind Enter and behind the
//! action panel runs here too. Adding a transform makes it bindable for free,
//! and fixing one fixes it everywhere.
//!
//! The launcher is never shown. Highlight some text, press the key, and the
//! text changes where it sits. Summoning a window to do that would defeat the
//! point, and it would also make the selection unreadable: reading a selection
//! means pressing Ctrl+C in the foreground application, and Sill must not be
//! the foreground application when that happens.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::action::{ActionCtx, ActionRegistry};
use crate::object::{Object, ObjectKind};
use crate::selection::{Held, Origin};

/// Where a bound action gets the thing it acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "camelCase")]
pub enum Source {
    /// Whatever is highlighted, falling back to the clipboard.
    ///
    /// The fallback is what makes a bound transform usable rather than fussy.
    /// Pressing the key with nothing highlighted should do the obvious thing
    /// rather than nothing, and the last thing copied is the obvious thing.
    Selection,
    /// What is on the clipboard, whatever is highlighted.
    Clipboard,
    /// The newest picture in the clipboard history.
    ///
    /// Its own source because a picture is not text and the two above both
    /// resolve to text. Screenshot something and press the key: there is
    /// nothing to select and nothing to highlight, and the last picture copied
    /// is the only thing it could sensibly mean.
    ClipboardImage,
    /// The window in front, whatever it is.
    ///
    /// The source that makes every window action bindable. Until this existed
    /// a key could not snap, maximise or move a window at all: the fifteen
    /// slots, the display move and the state actions all take a window, and
    /// the only way to name one was to summon the launcher, type its title and
    /// pick it out of a list. Which is not what anybody wants from "put this
    /// on the left"; by the time you have done that, dragging it was quicker.
    ///
    /// Sill's own window is deliberately not a candidate, which
    /// `windowing::foreground` already handles. A key pressed while the
    /// launcher is open means the thing behind it, never the launcher.
    ForegroundWindow,
    /// One particular thing from the index, named once when the binding is
    /// made. This is how a key opens a specific application.
    Command { id: String },
}

impl Source {
    /**
    Whether running this binding has anything to do with the clipboard.

    Taking the clipboard is not free and it is not invisible: it **pauses
    clipboard history** for the length of the operation and reads the current
    contents so they can be put back. That is exactly right for a transform,
    which presses Ctrl+C to read a selection and Ctrl+V to write the result.

    It is wrong for the other two. A key that snaps the window in front, or
    one that opens an application, never reads or writes the clipboard, and
    suspending the watcher for it means something copied in that moment is
    not recorded. Nobody would connect a missing history entry to having
    pressed the key that moved a window.
    */
    fn touches_clipboard(&self) -> bool {
        match self {
            Source::Selection | Source::Clipboard | Source::ClipboardImage => true,
            Source::ForegroundWindow | Source::Command { .. } => false,
        }
    }
}

/// One key, one action, one thing to run it against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    /// An accelerator like `Ctrl+Alt+U`.
    pub accelerator: String,
    /// The action's stable id, which is why action ids are stable.
    pub action: String,
    pub source: Source,
    /// Put the result back where the text came from.
    ///
    /// Only meaningful for a selection: replacing means pressing Ctrl+V, and
    /// there is nothing to paste over when the source was the clipboard.
    #[serde(default = "yes")]
    pub replace: bool,
    /**
    The answer to whatever the action would otherwise stop and ask for.

    Absent for every action that asks nothing, which is nearly all of them.
    It is here so that the two that do ask can be bound at all: "move this
    file to the archive folder" is a perfectly good key, and until the answer
    could travel with the binding it was not a key anything could be bound to.
    */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
}

fn yes() -> bool {
    true
}

/// "Whatever this thing's primary action is."
///
/// A key bound to an application means "open it", and the action that opens
/// one is not the action that opens a settings page or runs an extension
/// command. Storing the concrete action id would mean the settings list
/// guessing the kind at the moment the key is recorded, and getting it wrong
/// silently the moment the index changes underneath. Resolved at fire time
/// from the object instead, which is the only place the kind is a fact.
pub const PRIMARY: &str = "sill.primary";

/// Registers every binding, releasing whatever was registered before.
///
/// Called on startup and whenever the bindings change. Releasing first matters:
/// leaving a stale accelerator registered means the old key keeps working,
/// which looks exactly like the setting having been ignored.
pub fn apply(app: &AppHandle, previous: &[Binding], current: &[Binding]) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    for binding in previous {
        // Only the ones that are actually going away. Unregistering and
        // immediately re-registering an unchanged accelerator leaves a window
        // in which the key does nothing.
        if !current.iter().any(|b| b.accelerator == binding.accelerator) {
            let _ = app
                .global_shortcut()
                .unregister(binding.accelerator.as_str());
        }
    }

    for binding in current {
        if previous.iter().any(|b| b == binding) {
            continue;
        }

        // Changed rather than new: release the old meaning before binding it.
        if previous
            .iter()
            .any(|b| b.accelerator == binding.accelerator)
        {
            let _ = app
                .global_shortcut()
                .unregister(binding.accelerator.as_str());
        }

        let handle = app.clone();
        let bound = binding.clone();

        let result =
            app.global_shortcut()
                .on_shortcut(binding.accelerator.as_str(), move |_, _, event| {
                    // Fires on press and release; acting on both runs everything
                    // twice, which for a transform means transforming the result.
                    if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        return;
                    }

                    let handle = handle.clone();
                    let bound = bound.clone();
                    tauri::async_runtime::spawn(async move { fire(&handle, &bound).await });
                });

        /*
         * Recorded where settings can show it.
         *
         * The summon, switcher and capture keys have always been noted; a
         * per-command key was not, so a shortcut Windows refused looked
         * exactly like one that worked. The row showed the key, the key did
         * nothing, and the only sign was a line in a log nobody has open.
         *
         * Before the match below, which consumes the result.
         */
        if let Some(conflicts) = app.try_state::<crate::HotkeyConflicts>() {
            conflicts.note(&binding.accelerator, result.is_ok());
        }

        match result {
            Ok(()) => println!(
                "[sill] {} runs {} on {:?}",
                binding.accelerator, binding.action, binding.source
            ),
            // Another application already holds the combination. Reported
            // rather than fatal: one unusable key is not a reason to refuse
            // to start.
            Err(err) => crate::say!("could not bind {}: {err}", binding.accelerator),
        }
    }
}

/// Runs one binding.
async fn fire(app: &AppHandle, binding: &Binding) {
    let registry = app.state::<ActionRegistry>();

    // Taken before anything else runs, and given back at the end whatever
    // happens in between. One owner for the whole operation: the action itself
    // writes its result to the clipboard, so anything reading "the previous
    // contents" later reads Sill's own output and restores that instead.
    //
    // Only for the sources that have anything to do with it. See
    // `Source::touches_clipboard`.
    let held = binding.source.touches_clipboard().then(|| Held::take(app));

    let (object, origin) = match resolve(app, binding, held.as_ref()).await {
        Ok(resolved) => resolved,
        Err(reason) => {
            crate::say!("{}: {reason}", binding.accelerator);
            if let Some(held) = held {
                held.give_back();
            }
            return;
        }
    };

    // Resolved now rather than when the key was recorded: `PRIMARY` means
    // "open this", and which action opens a thing is a fact about the thing.
    let action = if binding.action == PRIMARY {
        registry.primary(object.kind)
    } else {
        registry.get(&binding.action)
    };

    let Some(action) = action else {
        crate::say!(
            "{} is bound to {}, which cannot be done to {:?}",
            binding.accelerator,
            binding.action,
            object.kind
        );
        if let Some(held) = held {
            held.give_back();
        }
        return;
    };

    if !action.accepts(object.kind) {
        crate::say!(
            "{} is bound to {}, which cannot be done to {:?}",
            binding.accelerator,
            binding.action,
            object.kind
        );
        if let Some(held) = held {
            held.give_back();
        }
        return;
    }

    let outcome = match registry
        .perform(
            // The answer a key was recorded with, when it was recorded with
            // one. A key bound to "move this to the archive folder" is the
            // whole reason `Binding` carries it.
            &ActionCtx::answering(app.clone(), binding.argument.clone()),
            action,
            &object,
        )
        .await
    {
        Ok(outcome) => outcome,
        Err(reason) => {
            crate::say!("{} failed: {reason}", binding.accelerator);
            if let Some(held) = held {
                held.give_back();
            }
            return;
        }
    };

    if may_paste_back(binding, origin) {
        if let Some(text) = outcome.text {
            // Blocking, like the capture: it waits on another application.
            let handle = app.clone();
            let result =
                tokio::task::spawn_blocking(move || crate::selection::replace(&handle, &text))
                    .await;

            match result {
                Ok(Err(reason)) => crate::say!("{}: {reason}", binding.accelerator),
                Err(err) => crate::say!("{}: the paste did not finish: {err}", binding.accelerator),
                Ok(Ok(())) => {}
            }
        }

        // The text changed where it sits, so the clipboard goes back to what
        // the user had. Leaving the result there is a side effect nobody asked
        // for.
        if let Some(held) = held {
            held.give_back();
        }
    } else if let Some(held) = held {
        // Nothing was pasted, so the point of the shortcut was to leave the
        // result on the clipboard, and it is already there.
        held.keep_result();
    }
}

/// Whether a binding's result may be pasted back over what it came from.
///
/// Pure and tested on its own, because the wrong answer here writes text into
/// somebody's document that they never chose.
///
/// The question is not what the binding asked for, it is what the capture
/// actually read. A selection binding falls back to the clipboard when nothing
/// is highlighted, which is a good fallback for showing a result and a
/// destructive one for pasting: the highlighted text would be replaced by
/// whatever happened to be on the clipboard. **This is not hypothetical.** The
/// first time this code ran against a real editor the capture failed, the
/// fallback fired, and the editor's contents were replaced with an unrelated
/// document.
fn may_paste_back(binding: &Binding, captured: Option<Origin>) -> bool {
    binding.replace && captured == Some(Origin::Selection)
}

/// What the binding acts on, and where the text came from if it is text.
async fn resolve(
    app: &AppHandle,
    binding: &Binding,
    held: Option<&Held>,
) -> Result<(Object, Option<Origin>), String> {
    match &binding.source {
        Source::Selection | Source::Clipboard => {
            // Unreachable rather than defensive: `touches_clipboard` says
            // these two do, so `fire` took it. A test holds the two together
            // so this can never become a silent refusal.
            let held = held.ok_or_else(|| {
                "the clipboard was not taken for a source that reads it".to_string()
            })?;

            let captured = if matches!(binding.source, Source::Selection) {
                // Blocking: reading a selection presses Ctrl+C and waits for
                // the other application to answer, which is not something to
                // do on a runtime worker. `block_in_place` rather than
                // `spawn_blocking` because the borrow cannot be moved to
                // another thread and must not be cloned: there is one
                // clipboard and one owner of it.
                tokio::task::block_in_place(|| {
                    crate::selection::Captured::selection_or_clipboard(app, held)
                })
            } else {
                crate::selection::Captured::clipboard(held)
            };

            let captured =
                captured.ok_or_else(|| "nothing selected and nothing copied".to_string())?;
            let origin = captured.from;

            Ok((
                Object {
                    kind: ObjectKind::Text,
                    id: "selection".to_string(),
                    title: preview(&captured.text),
                    target: captured.text,
                    mode: "text".to_string(),
                },
                Some(origin),
            ))
        }

        // No origin: there was no selection behind a screenshot to put
        // anything back into.
        Source::ClipboardImage => last_image(app).map(|object| (object, None)),

        // No origin either. Moving a window produces no text, and there is
        // nothing to paste back into.
        Source::ForegroundWindow => crate::windowing::foreground()
            .map(|window| (Object::from_window(&window), None))
            .ok_or_else(|| "nothing is in front".to_string()),

        Source::Command { id } => {
            let registry = app.state::<crate::state::RegistryState>();
            let record = registry
                .index()
                .commands
                .iter()
                .find(|c| &c.id == id)
                .cloned()
                .ok_or_else(|| format!("nothing in the index is called {id}"))?;

            // No origin. Launching an application produces no text, and there
            // is no selection behind it to put anything back into.
            Object::from_record(&record)
                .map(|object| (object, None))
                .ok_or_else(|| format!("{} is a kind of thing Sill cannot act on", record.title))
        }
    }
}

/// The last picture copied, as something an action can be run against.
///
/// One implementation, because two ways of reaching text recognition should
/// not be able to disagree about which picture they mean: the key bound to it
/// and the row in the list both come through here.
pub(crate) fn last_image(app: &AppHandle) -> Result<Object, String> {
    let clipboard = app
        .try_state::<crate::clipboard::monitor::Clipboard>()
        .ok_or_else(|| "clipboard history is not running".to_string())?;

    let newest = clipboard
        .store()
        .search("", Some(crate::clipboard::kind::Kind::Image), 1)
        .map_err(|err| format!("could not look for a picture: {err}"))?
        .into_iter()
        .next()
        .ok_or_else(|| "nothing has been copied as a picture yet".to_string())?;

    Ok(Object {
        kind: ObjectKind::ClipboardEntry,
        // The row number, which is what reaches the picture. A picture has no
        // text, so the target cannot carry it.
        id: newest.id.to_string(),
        title: "the last picture copied".to_string(),
        target: String::new(),
        mode: "clipboard".to_string(),
    })
}

/// A one-line stand-in for a block of text, for logs and messages.
fn preview(text: &str) -> String {
    let first = text.lines().next().unwrap_or_default().trim();
    let mut out: String = first.chars().take(40).collect();
    if first.chars().count() > 40 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(accelerator: &str, action: &str) -> Binding {
        Binding {
            accelerator: accelerator.into(),
            action: action.into(),
            source: Source::Selection,
            replace: true,
            argument: None,
        }
    }

    #[test]
    fn a_binding_round_trips_through_its_stored_form() {
        for source in [
            Source::Selection,
            Source::Clipboard,
            Source::Command {
                id: "app:code".into(),
            },
        ] {
            let original = Binding {
                source,
                ..binding("Ctrl+Alt+U", "sill.text.upper")
            };

            let text = serde_json::to_string(&original).expect("serialises");
            let back: Binding = serde_json::from_str(&text).expect("parses");
            assert_eq!(back, original);
        }
    }

    /// A key can carry the answer its action would otherwise stop and ask for.
    ///
    /// The point of the whole change. Renaming and moving were commands the
    /// window called, so "move this to the archive folder" was not something a
    /// key could mean: there was nowhere on a binding to put the folder and
    /// nowhere in an action to receive it. Both halves exist now, and this is
    /// the half that survives being written to disk.
    #[test]
    fn a_binding_carries_the_answer_its_action_has_to_be_given() {
        let bound = Binding {
            action: "sill.file.move".into(),
            argument: Some(r"C:\Users\me\Archive".into()),
            source: Source::Command {
                id: "file:notes".into(),
            },
            ..binding("Ctrl+Alt+M", "sill.file.move")
        };

        let text = serde_json::to_string(&bound).expect("serialises");
        let back: Binding = serde_json::from_str(&text).expect("parses");

        assert_eq!(back, bound);
        assert_eq!(back.argument.as_deref(), Some(r"C:\Users\me\Archive"));
    }

    /// And the great majority, which ask nothing, write nothing.
    ///
    /// `skip_serializing_if` rather than a null on every binding anybody has:
    /// preferences are a file people open, and a field that is empty on all
    /// twenty of their keys is twenty lines of noise about a feature two
    /// actions use.
    #[test]
    fn a_binding_with_nothing_to_answer_says_nothing() {
        let text =
            serde_json::to_string(&binding("Ctrl+Alt+U", "sill.text.upper")).expect("serialises");

        assert!(
            !text.contains("argument"),
            "an ordinary binding writes an empty argument: {text}"
        );
    }

    /// A binding written before arguments existed still parses.
    ///
    /// Every binding on every machine is one of these. Making the field
    /// required would refuse the whole preferences file, which is how a
    /// launcher loses somebody's shortcuts on an update.
    #[test]
    fn a_binding_written_before_arguments_existed_still_parses() {
        let older = r#"{"accelerator":"Ctrl+Alt+U","action":"sill.text.upper","source":{"from":"selection"},"replace":true}"#;
        let parsed: Binding = serde_json::from_str(older).expect("parses");

        assert_eq!(parsed.argument, None);
        assert!(parsed.replace);
    }

    #[test]
    fn a_binding_written_before_replace_existed_still_replaces() {
        // The upgrade case. Defaulting this to false would silently turn every
        // existing transform binding into one that copies and does nothing
        // visible, which reads as the shortcut having broken.
        let older = r#"{"accelerator":"Ctrl+Alt+U","action":"sill.text.upper","source":{"from":"selection"}}"#;
        let parsed: Binding = serde_json::from_str(older).expect("parses");
        assert!(parsed.replace);
    }

    #[test]
    fn a_result_goes_back_only_into_a_selection_that_was_actually_read() {
        let bound = binding("Ctrl+Alt+U", "sill.text.upper");
        assert!(bound.replace, "the binding under test asks to replace");

        // The whole point: read a selection, put the result back.
        assert!(may_paste_back(&bound, Some(Origin::Selection)));

        // Nothing was highlighted, so the capture fell back to the clipboard.
        // Pasting now would replace whatever *is* highlighted with unrelated
        // text. Found on a real desktop, where it overwrote a document.
        assert!(!may_paste_back(&bound, Some(Origin::Clipboard)));

        // Launching an application produces nothing to paste anywhere.
        assert!(!may_paste_back(&bound, None));
    }

    #[test]
    fn a_binding_that_only_copies_never_pastes() {
        let copier = Binding {
            replace: false,
            ..binding("Ctrl+Alt+J", "sill.text.upper")
        };

        for captured in [Some(Origin::Selection), Some(Origin::Clipboard), None] {
            assert!(!may_paste_back(&copier, captured), "{captured:?}");
        }
    }

    #[test]
    fn a_preview_is_one_line_and_short() {
        // It goes in a log line and a status message; a paragraph in either is
        // unreadable and a multi-line one breaks the format.
        let long = "first line that runs on and on and on and on and on\nsecond line";
        let shown = preview(long);

        assert!(!shown.contains('\n'));
        assert!(shown.chars().count() <= 41, "{shown:?}");
        assert!(shown.ends_with('…'));

        assert_eq!(preview("  short  "), "short");
        assert_eq!(preview(""), "");
    }

    /// The rule that keeps `touches_clipboard` and `resolve` in step.
    ///
    /// `resolve` refuses a clipboard source that arrives without the
    /// clipboard, and `fire` decides whether to take it from this predicate.
    /// If the two ever disagree, a transform stops working and the reason is
    /// a message about the clipboard not being taken, which reads as a bug in
    /// the clipboard rather than in this pair. Held together here instead.
    #[test]
    fn every_source_that_reads_the_clipboard_is_one_that_takes_it() {
        let reads = [Source::Selection, Source::Clipboard, Source::ClipboardImage];

        for source in reads {
            assert!(
                source.touches_clipboard(),
                "{source:?} is resolved from the clipboard and would arrive without it"
            );
        }
    }

    /// A key that moves a window must not pause clipboard history.
    ///
    /// Taking the clipboard suspends the watcher for the length of the
    /// operation, so anything copied in that moment is not recorded. Nobody
    /// would connect a missing history entry to having pressed the key that
    /// moved a window, which is what makes this worth a test rather than a
    /// comment.
    #[test]
    fn a_window_or_a_launch_leaves_the_clipboard_alone() {
        assert!(!Source::ForegroundWindow.touches_clipboard());
        assert!(!Source::Command {
            id: "app:chrome".into()
        }
        .touches_clipboard());
    }

    /// The source survives a round trip through the preferences file.
    ///
    /// A binding is stored as JSON, and a variant that serialises to a name
    /// the reader does not know is a shortcut that quietly disappears from
    /// somebody's settings on the next start.
    #[test]
    fn the_window_source_reads_back_as_itself() {
        let written = serde_json::to_string(&Source::ForegroundWindow).expect("serialisable");
        assert_eq!(written, r#"{"from":"foregroundWindow"}"#);

        let read: Source = serde_json::from_str(&written).expect("readable");
        assert_eq!(read, Source::ForegroundWindow);
    }
}
