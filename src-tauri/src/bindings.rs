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
    /**
    Whatever is selected right now, whether that is files or text.

    The one source that does not say in advance what kind of thing it will
    produce, and the reason it exists: "act on what I am looking at" is one
    thought, and having to know beforehand whether you are looking at a
    paragraph or at three files makes it two keys instead of one.

    Explorer is asked first, through [`crate::explorer`], because it can be
    asked without touching anything: it already knows what is highlighted and
    says so. Only when that comes back with nothing does this fall through to
    [`Source::Selection`], which presses Ctrl+C. So the expensive, intrusive
    half of reading a selection is skipped entirely whenever the cheap half
    answered, and the clipboard is left alone with it. See
    [`Source::touches_clipboard`].
    */
    CurrentSelection,
    /**
    The folder open in the Explorer window nearest the front.

    The source `P8-07` needs, and the one that could not be expressed by any
    of the others. [`Source::CurrentSelection`] asks Explorer what is
    highlighted **in the window that has the keyboard**, which is right for
    "act on what I am looking at" and useless here: a dialog jump is pressed
    inside a Save dialog, so Explorer is behind it by definition and has no
    selection worth reading. What is wanted is the folder that window is
    showing, whether or not anything in it is highlighted.

    Not restricted to jumping. It is "the folder I have open", so a key can
    equally open a terminal there, copy its path, or compress it, and each of
    those is an action that already exists.
    */
    ExplorerFolder,
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

    **The universal source is the one that cannot answer this from its own
    name**, which is why the question takes an argument. Files highlighted in
    Explorer are read out of Explorer and never go near the clipboard; the
    same key pressed in a document has to press Ctrl+C. Asking Explorer
    happens first, so by the time this is asked the answer is known, and a
    key pressed over three selected files pauses nothing.
    */
    fn touches_clipboard(&self, files_are_selected: bool) -> bool {
        match self {
            Source::Selection | Source::Clipboard | Source::ClipboardImage => true,
            Source::CurrentSelection => !files_are_selected,
            Source::ExplorerFolder | Source::ForegroundWindow | Source::Command { .. } => false,
        }
    }

    /// Whether this source asks Explorer what is **highlighted** before
    /// anything else.
    ///
    /// Only the universal one. Reading Explorer costs a few cross-process COM
    /// calls, which is nothing next to pressing Ctrl+C in somebody's document
    /// but is not nothing on a key that was never going to act on a file.
    ///
    /// [`Source::ExplorerFolder`] asks Explorer something too and still
    /// answers no, which is not an oversight. This question exists to order
    /// one thing against the clipboard: what is highlighted has to be known
    /// before the clipboard is taken, because knowing it is what makes taking
    /// it unnecessary. A folder is not a selection, has nothing to do with the
    /// clipboard, and is read where every other source is read.
    fn reads_explorer(&self) -> bool {
        matches!(self, Source::CurrentSelection)
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

/// "Do not run anything, show me what could be run."
///
/// The second sentinel, and the one that makes `P8-01` a feature rather than a
/// list of new keys. A binding names one action, which is right for "upper-case
/// this" and wrong for "I have three files selected and I will decide when I
/// see the list". This puts the launcher's own action panel on whatever the
/// source resolved to and stops there.
///
/// **It is not an action and must not become one.** Registering it would put
/// "Show Actions" in every action panel in the launcher, including the one it
/// opens. It is also not a hole in the rule that `ActionRegistry::perform` is
/// the only way an action runs: nothing is performed here. Whatever is chosen
/// from the panel goes through `run_action` and so through `perform`, exactly
/// as it does when the panel was opened with Ctrl+K on a search result.
pub const PANEL: &str = "sill.actions";

/// What a binding's action id actually names.
///
/// Two of the three are ids no action has, so reading them as action ids finds
/// nothing and the key reports that it is bound to something impossible. Pure
/// and tested, because the failure is silent: a sentinel that stopped being
/// recognised would leave a working key answering "cannot be done to File".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wanted<'a> {
    /// Show the panel instead of running anything. See [`PANEL`].
    Panel,
    /// Whatever Enter does for the kind that was resolved. See [`PRIMARY`].
    Primary,
    /// One action, by its stable id.
    Named(&'a str),
}

pub fn wanted(action: &str) -> Wanted<'_> {
    match action {
        PANEL => Wanted::Panel,
        PRIMARY => Wanted::Primary,
        other => Wanted::Named(other),
    }
}

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

    /*
     * Explorer is asked first, and without the clipboard.
     *
     * The order is the whole reason this is here rather than inside `resolve`.
     * Taking the clipboard suspends history, and a key pressed over three
     * highlighted files has no business doing that: those files are read out
     * of Explorer and nothing is copied. So what is highlighted has to be
     * known before the clipboard question is asked, and it is.
     *
     * Blocking, on a pool thread: the read crosses into another process and
     * gives up on its own after `explorer::PATIENCE`.
     */
    let files: Vec<Object> = if binding.source.reads_explorer() {
        tokio::task::spawn_blocking(|| crate::explorer::objects_from(&crate::explorer::selection()))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Taken before anything else runs, and given back at the end whatever
    // happens in between. One owner for the whole operation: the action itself
    // writes its result to the clipboard, so anything reading "the previous
    // contents" later reads Sill's own output and restores that instead.
    //
    // Only for the sources that have anything to do with it. See
    // `Source::touches_clipboard`.
    let held = binding
        .source
        .touches_clipboard(!files.is_empty())
        .then(|| Held::take(app));

    let (objects, origin) = match resolve(app, binding, held.as_ref(), files).await {
        Ok(resolved) => resolved,
        Err(reason) => {
            crate::say!("{}: {reason}", binding.accelerator);
            if let Some(held) = held {
                held.give_back();
            }
            return;
        }
    };

    /*
     * "Show me what could be done" runs nothing and stops here.
     *
     * Before the registry lookup, because `PANEL` is deliberately not an
     * action id: looking it up would find nothing and the key would report
     * itself as bound to something impossible. The launcher goes up with the
     * selection on it, and whatever is chosen there runs through
     * `ActionRegistry::perform` like every other panel entry.
     */
    if matches!(wanted(&binding.action), Wanted::Panel) {
        crate::summon::show_actions(app, &objects);

        // Nothing was pasted and nothing was meant to be kept: reading a text
        // selection left Sill's own copy on the clipboard, and the person's
        // own contents go back.
        if let Some(held) = held {
            held.give_back();
        }
        return;
    }

    // One object for every source but the universal one, and however many are
    // highlighted for that. Each goes through `perform` separately, because an
    // action acts on a thing: "recycle these three" is three recycles, three
    // entries in the activity log and three undos, which is what somebody who
    // then presses Ctrl+Z expects to find.
    let mut produced: Option<String> = None;

    for object in &objects {
        // Resolved now rather than when the key was recorded: `PRIMARY` means
        // "open this", and which action opens a thing is a fact about the thing.
        let action = match wanted(&binding.action) {
            Wanted::Primary => registry.primary(object.kind),
            Wanted::Named(id) => registry.get(id),
            // Handled above, before anything was looked up.
            Wanted::Panel => None,
        };

        let Some(action) = action.filter(|action| action.accepts(object.kind)) else {
            crate::say!(
                "{} is bound to {}, which cannot be done to {:?}",
                binding.accelerator,
                binding.action,
                object.kind
            );
            continue;
        };

        match registry
            .perform(
                // The answer a key was recorded with, when it was recorded with
                // one. A key bound to "move this to the archive folder" is the
                // whole reason `Binding` carries it.
                &ActionCtx::answering(app.clone(), binding.argument.clone()),
                action,
                object,
            )
            .await
        {
            Ok(outcome) => produced = outcome.text,
            // One failure does not abandon the rest of a selection. A file
            // that has been deleted since it was highlighted must not stop the
            // other two being acted on.
            Err(reason) => crate::say!("{} failed: {reason}", binding.accelerator),
        }
    }

    if may_paste_back(binding, origin) {
        if let Some(text) = produced {
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

/**
What the binding acts on, and where the text came from if it is text.

A list rather than one thing, because one source can resolve to several: three
files highlighted in Explorer are three objects and the key means all of them.
Every other source produces exactly one, and a text selection always produces
exactly one, which is what keeps the paste-back below unambiguous.

`files` is what Explorer already said was highlighted, read by `fire` before
the clipboard was considered. Empty means either that this source does not ask
Explorer or that Explorer had nothing to say, and both fall through to reading
text.
*/
async fn resolve(
    app: &AppHandle,
    binding: &Binding,
    held: Option<&Held>,
    files: Vec<Object>,
) -> Result<(Vec<Object>, Option<Origin>), String> {
    match &binding.source {
        // Files, when Explorer had some. No origin: a file is not text and
        // there is no selection behind it to paste anything into.
        Source::CurrentSelection if !files.is_empty() => Ok((files, None)),

        // Everything else the universal source can find is text, read the one
        // way there is to read it.
        Source::Selection | Source::Clipboard | Source::CurrentSelection => {
            // Unreachable rather than defensive: `touches_clipboard` says
            // these do, so `fire` took it. A test holds the two together
            // so this can never become a silent refusal.
            let held = held.ok_or_else(|| {
                "the clipboard was not taken for a source that reads it".to_string()
            })?;

            // The universal source has already tried the cheap half and found
            // nothing, so what is left of it is exactly `Source::Selection`.
            let reads_a_selection =
                matches!(binding.source, Source::Selection | Source::CurrentSelection);

            let captured = if reads_a_selection {
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
                vec![Object {
                    kind: ObjectKind::Text,
                    id: "selection".to_string(),
                    title: preview(&captured.text),
                    target: captured.text,
                    mode: "text".to_string(),
                }],
                Some(origin),
            ))
        }

        // No origin: there was no selection behind a screenshot to put
        // anything back into.
        Source::ClipboardImage => last_image(app).map(|object| (vec![object], None)),

        /*
         * The folder Explorer has open behind whatever is in front.
         *
         * Read here rather than in `fire` beside the selection, because
         * nothing about the ordering depends on it: this source never touches
         * the clipboard, so there is no question of taking it first. Blocking,
         * on a runtime worker held open by `block_in_place`, and bounded by
         * `explorer::PATIENCE` the same way the selection read is.
         */
        Source::ExplorerFolder => {
            let path = tokio::task::block_in_place(crate::explorer::folder_in_front)
                .ok_or_else(|| "no Explorer window has a folder open".to_string())?;

            Ok((
                vec![Object {
                    kind: ObjectKind::Folder,
                    // The same id a folder found by search has, so the
                    // activity log and the ranker see one identity for one
                    // folder however it was reached.
                    id: format!("file:{path}"),
                    title: crate::files_ops::name_of(std::path::Path::new(&path)),
                    target: path,
                    mode: "folder".to_string(),
                }],
                None,
            ))
        }

        // No origin either. Moving a window produces no text, and there is
        // nothing to paste back into.
        Source::ForegroundWindow => crate::windowing::foreground()
            .map(|window| (vec![Object::from_window(&window)], None))
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
                .map(|object| (vec![object], None))
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
        let reads = [
            Source::Selection,
            Source::Clipboard,
            Source::ClipboardImage,
            // With nothing highlighted in Explorer, the universal source is
            // `Source::Selection`, and `resolve` refuses it without the
            // clipboard.
            Source::CurrentSelection,
        ];

        for source in reads {
            assert!(
                source.touches_clipboard(false),
                "{source:?} is resolved from the clipboard and would arrive without it"
            );
        }
    }

    /// Files highlighted in Explorer are read without pausing the history.
    ///
    /// The rule `P3-05` established, arriving at the one source that can go
    /// either way. Taking the clipboard suspends the watcher for the length of
    /// the operation, and a key pressed over three selected files never reads
    /// or writes it: Explorer was asked and Explorer answered. Suspending it
    /// anyway would drop whatever was copied in that moment, with nothing
    /// connecting the missing entry to the key.
    #[test]
    fn a_file_selection_leaves_the_clipboard_alone() {
        assert!(!Source::CurrentSelection.touches_clipboard(true));

        // And the other three do not change their answer because Explorer
        // happens to have something highlighted behind them.
        for source in [Source::Selection, Source::Clipboard, Source::ClipboardImage] {
            assert!(source.touches_clipboard(true), "{source:?}");
        }
    }

    /// Only the universal source pays for asking Explorer.
    #[test]
    fn nothing_but_the_universal_source_asks_explorer() {
        assert!(Source::CurrentSelection.reads_explorer());

        for source in [
            Source::Selection,
            Source::Clipboard,
            Source::ClipboardImage,
            Source::ForegroundWindow,
            // Asks Explorer for a folder rather than for a selection, which
            // is not what this question is about. See `reads_explorer`.
            Source::ExplorerFolder,
            Source::Command {
                id: "app:code".into(),
            },
        ] {
            assert!(!source.reads_explorer(), "{source:?}");
        }
    }

    /// The folder source survives the preferences file.
    ///
    /// A variant that serialises to a name the reader does not know is a
    /// shortcut that quietly disappears from somebody's settings on the next
    /// start, which is how a launcher loses a key nobody touched.
    #[test]
    fn the_folder_source_reads_back_as_itself() {
        let written = serde_json::to_string(&Source::ExplorerFolder).expect("serialisable");
        assert_eq!(written, r#"{"from":"explorerFolder"}"#);

        let read: Source = serde_json::from_str(&written).expect("readable");
        assert_eq!(read, Source::ExplorerFolder);
    }

    /// Jumping to a folder must not pause clipboard history.
    ///
    /// The rule `P8-01` established, arriving at the source `P8-07` added.
    /// Explorer is asked where it is and answers; nothing is copied, nothing
    /// is pasted, and suspending the watcher for it would drop whatever the
    /// person copied in that moment with nothing connecting the two.
    #[test]
    fn a_folder_jump_leaves_the_clipboard_alone() {
        assert!(!Source::ExplorerFolder.touches_clipboard(false));
        assert!(!Source::ExplorerFolder.touches_clipboard(true));
    }

    /// The two ids that are not actions are read as what they are.
    ///
    /// Silent when it breaks, which is why it is a test. A sentinel that
    /// stopped being recognised would be looked up in the registry, found
    /// missing, and reported as a key bound to something that cannot be done
    /// to a file, which points at the action rather than at this.
    #[test]
    fn the_two_ids_that_are_not_actions_are_recognised() {
        assert_eq!(wanted(PANEL), Wanted::Panel);
        assert_eq!(wanted(PRIMARY), Wanted::Primary);
        assert_eq!(wanted("sill.text.upper"), Wanted::Named("sill.text.upper"));
    }

    // The matching check that neither sentinel is a registered action id lives
    // in `tests/actions.rs`, and has to. Calling `actions::builtins()` from the
    // library's own test binary retains the dialog plugin, whose
    // `TaskDialogIndirect` needs a manifest that binary does not get, and the
    // whole run dies with `STATUS_ENTRYPOINT_NOT_FOUND` before a test executes.
    // `suite/mod.rs` documents the rule; this is what it looks like when the
    // rule is broken by one line in a test.

    /// The universal source survives the preferences file.
    ///
    /// A variant that serialises to a name the reader does not know is a
    /// shortcut that quietly disappears from somebody's settings on the next
    /// start.
    #[test]
    fn the_universal_source_reads_back_as_itself() {
        let written = serde_json::to_string(&Source::CurrentSelection).expect("serialisable");
        assert_eq!(written, r#"{"from":"currentSelection"}"#);

        let read: Source = serde_json::from_str(&written).expect("readable");
        assert_eq!(read, Source::CurrentSelection);
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
        assert!(!Source::ForegroundWindow.touches_clipboard(false));
        assert!(!Source::Command {
            id: "app:chrome".into()
        }
        .touches_clipboard(false));
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
