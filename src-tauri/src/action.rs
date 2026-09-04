//! What can be done to a thing, and what doing it cost.
//!
//! One registry stands behind every way an action gets invoked: pressing
//! Enter on a result, picking from the action panel, and later a workflow
//! step, an automation trigger, or a tool an AI is allowed to call. That is
//! the whole point of putting it here rather than writing each of those
//! separately. An action implemented once is available to all of them, and a
//! permission checked once covers all of them too.
//!
//! Kept deliberately small. There is no payload type, no chaining and no
//! scheduling, because nothing needs them yet and the shape they should take
//! is not knowable from here. What is here is the part every later feature
//! agrees on: a thing, something you can do to it, what it may touch, and how
//! to take it back.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::object::{Object, ObjectKind};

/// What an action is allowed to touch.
///
/// Declared rather than inferred, so the question "what can this reach?" has
/// an answer that can be read off a list instead of by tracing code. Nothing
/// enforces these yet; they are here now because retrofitting a permission
/// model onto actions that never declared anything means auditing every one
/// of them at the point it is most expensive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    ClipboardRead,
    ClipboardWrite,
    FileRead,
    FileWrite,
    ProcessLaunch,
    /// Synthesises keyboard input into whatever is in front.
    InputInjection,
    Network,
    /// Draws inside the surface Sill already put on screen.
    ///
    /// A toast, a HUD, a dialog, the text in the search bar of the command the
    /// person is looking at. All of it is paint on a window that is open
    /// because somebody opened it, which is why this is the one capability an
    /// extension holds without being asked.
    ///
    /// **Making the window go away is not drawing**, and it used to live here.
    /// See [`Self::LauncherDismiss`].
    Ui,
    /// Takes Sill's window off the screen while somebody is using it.
    ///
    /// Split out of `Ui`, which is free, because the two are not the same
    /// favour. Drawing a toast asks nothing of the person: the window they
    /// opened is still the window in front of them. Dismissing it ends what
    /// they were doing, hands the keyboard back to whatever was behind, and is
    /// the second half of the copy-then-hide-then-paste chord that types into
    /// a document Sill was never shown. It is also the whole of a denial of
    /// service: a command that calls it on every render is a launcher that
    /// cannot be kept open.
    ///
    /// Named for what somebody is agreeing to rather than for the window
    /// system, because the sentence on the card is "close Sill's window while
    /// you are using it" and nobody consents to a subsystem.
    LauncherDismiss,
    /// Changes the machine: the volume, the theme, the lock screen.
    ///
    /// Its own thing rather than folded into `Ui`, which is Sill's own
    /// surface. Somebody granting a launcher permission to draw its own window
    /// has not thereby granted it permission to mute their speakers.
    SystemControl,
    /// Runs an arbitrary command line.
    ///
    /// Not a stronger `ProcessLaunch`, which opens a named thing the way
    /// double-clicking it would. This hands over a shell, and a shell is every
    /// other capability on this list at once: it can read files, write them,
    /// reach the network and start anything. It is separate so that nothing
    /// grants it by accident while meaning "open a program".
    ShellExecution,
    /// Reads whatever is selected in whichever program is in front.
    ///
    /// Its own thing rather than a kind of `ClipboardRead`: somebody has to
    /// copy before a clipboard read sees anything, and that act is the consent.
    /// A selection is read without anybody doing anything at all, so it can
    /// take text out of a document nobody chose to share.
    SelectionRead,
    /// Moves, resizes, focuses or closes somebody else's window.
    ///
    /// Separate from `Ui`, which is Sill's own surface. Reaching into another
    /// application's windows is a different thing to ask for and the two must
    /// not be grantable together by accident.
    WindowControl,
}

/// How to take an action back.
///
/// Data rather than a closure, and the difference is load-bearing. A closure
/// captures whatever it needs and keeps it alive, which is how an undo stack
/// quietly becomes a place where deleted files live. A descriptor is small,
/// inspectable, and can be shown in a history someone actually reads.
///
/// Deliberately short. Most actions return no undo at all, and that is the
/// honest answer: launching an application cannot be taken back, and neither
/// can pasting into somebody else's window. Offering an undo that silently
/// does nothing is worse than offering none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Undo {
    /// Put back what was on the clipboard before.
    RestoreClipboard { text: String },
    /// Remove a file this action made, and nothing else.
    ///
    /// Only ever a file that did not exist a moment ago: compressing writes a
    /// new archive and touches nothing else, so deleting it puts things back
    /// exactly. Never used to undo anything that removed or replaced a file,
    /// because that is not something a descriptor can reverse.
    DeleteFile { path: String, name: String },
    /// Put something back in the folder it came out of.
    ///
    /// Two paths and a name, which is all a move is. Safe to keep because it
    /// describes the change rather than holding what was changed: undoing a
    /// move of a ten gigabyte folder costs the same as undoing a move of a
    /// text file.
    MovePath {
        /// Where it is now.
        path: String,
        /// The folder to put it back in.
        back_to: String,
        name: String,
    },
    /// Put a window back where it was, and back how it was.
    ///
    /// The state matters as much as the rectangle: a window that was maximized
    /// and got snapped to a half has to be maximized again, not merely
    /// restored to the size it would have had. Restoring only the rectangle
    /// leaves it looking almost right, which is worse than leaving it alone.
    RestoreWindow {
        id: isize,
        rect: crate::windowing::Rect,
        maximized: bool,
        title: String,
    },
}

/// What happened, and whether it can be reversed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// One line, shown to the user. Past tense, because it already happened.
    pub message: String,
    /// Absent for the great majority of actions. See [`Undo`].
    ///
    /// **Never sent to the window.** It is the recipe for reversing something,
    /// and the window's job is to ask for that by name rather than to hold it:
    /// a descriptor sent out and handed back could be replayed as often as
    /// somebody pressed the key, which is what `undone` is for.
    #[serde(skip)]
    pub undo: Option<Undo>,
    /// Which entry in the activity log this became, when it can be taken back.
    ///
    /// The window keeps this and passes it to `undo_activity`, so the log
    /// spends the undo and the same action cannot be reversed twice. Absent
    /// when there is nothing to reverse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undone_by: Option<u64>,
    /// An extension command that is now running and needs rendering.
    ///
    /// The one piece of an action's result that outlives the action. It is
    /// named for exactly what it is rather than hidden in a general-purpose
    /// bag, because a general-purpose bag is how a type stops meaning
    /// anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The text this produced, when it produced any.
    ///
    /// What lets a shortcut put the result back where the text came from, and
    /// the seam a workflow will eventually chain through: an action that
    /// returns text can feed one that takes it. Absent for everything that
    /// opens, launches or navigates, which is most of them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Outcome {
    pub fn done(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            undo: None,
            undone_by: None,
            session: None,
            text: None,
        }
    }

    pub fn undoable(message: impl Into<String>, undo: Undo) -> Self {
        Self {
            undo: Some(undo),
            ..Self::done(message)
        }
    }

    pub fn running(message: impl Into<String>, session: impl Into<String>) -> Self {
        Self {
            session: Some(session.into()),
            ..Self::done(message)
        }
    }

    /// Records what this produced, so a caller can put it somewhere.
    pub fn producing(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// Everything an action gets besides the thing it is acting on.
///
/// It exists as a type rather than a bare `AppHandle` because the next things
/// to go in are known: the foreground application, the current selection, what
/// is on the clipboard. Adding a field to this is a change nobody else has to
/// notice; adding a parameter to every action is not. The second field is the
/// first time that bargain has been called in.
pub struct ActionCtx {
    pub app: AppHandle,
    /**
    The one answer somebody had to be asked for before this could run.

    Renaming needs a new name and moving needs a folder, and for as long as
    there was nowhere in an action to put either, both lived in the window:
    the page took over the search field, collected the answer, and called a
    Tauri command that did the work itself. That made them **the two actions
    only the page could reach**. A key could not be bound to them, the model
    could not run them, and neither appeared in the activity log, which is the
    exact arrangement the registry exists to end.

    Deliberately one string rather than a map. Both questions have one answer,
    and a bag of named parameters would be a shape invented for a second case
    that does not exist yet. When one arrives, this becomes an enum and the
    two call sites change.

    Private, so it cannot be set to something empty by accident: the
    constructors drop whitespace, and an action asking for it either gets
    something or knows to say what it needed.
    */
    argument: Option<String>,
}

impl ActionCtx {
    /// For the great majority of actions, which take nothing but the object.
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            argument: None,
        }
    }

    /// For an action that had to ask something first.
    ///
    /// Blank is the same as absent. A rename whose field was cleared is not a
    /// rename to the empty string, it is a rename with no answer yet, and the
    /// action says so rather than handing `""` to the filesystem.
    pub fn answering(app: AppHandle, argument: Option<String>) -> Self {
        Self {
            app,
            argument: meant(argument),
        }
    }

    /// What was answered, when anything was.
    pub fn argument(&self) -> Option<&str> {
        self.argument.as_deref()
    }
}

/// What an answer amounts to, once whitespace is taken off it.
///
/// A free function rather than a line inside the constructor, because it is
/// the only part of the context a test can reach: an [`ActionCtx`] holds a
/// concrete `AppHandle`, and nothing but a running Tauri app can make one.
///
/// The rule is that blank is absent. A rename whose field was cleared is not a
/// rename to the empty string, and the difference is a file called `""` that
/// no shell can address versus a sentence saying what was needed. `""` also
/// reaches here from the model, whose tool arguments have no absent, only
/// empty.
fn meant(argument: Option<String>) -> Option<String> {
    argument
        .map(|given| given.trim().to_string())
        .filter(|given| !given.is_empty())
}

/// Something that can be done to an [`Object`].
///
/// Async, and boxed by `async_trait` so the registry can hold them as trait
/// objects. Two of the actions that exist today already await, and nearly
/// everything queued behind this one (a file operation, a network call, an AI
/// request) awaits by nature. A synchronous trait would have meant one
/// permanent special case now and an awkward migration later.
#[async_trait]
pub trait Action: Send + Sync {
    /// Stable across releases: it is what a shortcut, a workflow step or a
    /// stored preference refers to. Renaming one breaks those; changing the
    /// title does not.
    ///
    /// Borrowed from the action rather than `&'static str`, which is what this
    /// was. Every action Sill ships names itself with a literal and still
    /// does. What `'static` also said, without meaning to, is that **no action
    /// can be learned about while Sill is running**, and that is the whole of
    /// why an extension could not contribute one: its id is read out of a
    /// manifest at install time and is a `String` the action owns.
    fn id(&self) -> &str;

    /// What the action panel shows. Imperative, because it is a thing you are
    /// about to do rather than a description of what it does.
    fn title(&self) -> &str;

    fn accepts(&self, kind: ObjectKind) -> bool;

    fn capabilities(&self) -> &'static [Capability];

    /// Whether this is what Enter does for this kind.
    ///
    /// Exactly one action per kind should answer yes. The registry checks it.
    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    /// The chord this action ships with, before anybody changes it.
    ///
    /// Declared here rather than by whichever surface happens to draw the
    /// action, which is what it was: the launcher wrote `Ctrl+C` beside the
    /// clipboard's Copy by hand, so an action that arrived through a different
    /// list arrived with no chord at all and nothing said so. An action knows
    /// what it is for; the window does not.
    ///
    /// `None` for most of them, and deliberately. A key that runs something
    /// destructive without being asked for is worse than no key: nothing here
    /// deletes, quits or uninstalls on a chord it was given rather than one
    /// somebody chose. The person can still set one in Settings.
    fn shortcut(&self) -> Option<crate::action_keys::Shortcut> {
        None
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String>;
}

/// An action as the window needs to draw it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionInfo {
    pub id: String,
    pub title: String,
    pub primary: bool,
    /// The chord that runs it, after whatever the person has set.
    ///
    /// Resolved here rather than in the window, so the panel that draws it and
    /// the matcher that reads it are looking at one answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<crate::action_keys::Shortcut>,
}

/// Every action Sill knows.
///
/// A plain list rather than a map, because the lookups are "which of these
/// accept this kind" and "which is primary", and both are linear over a set
/// small enough that anything cleverer would be slower to read and no faster
/// to run.
///
/// ## Two lists, one vocabulary
///
/// [`Self::shipped`] is the vocabulary compiled into this build. It cannot
/// change while Sill runs, because nothing can add a Rust type at run time.
///
/// [`Self::contributed`] is what installed extensions declare, which changes
/// the moment somebody installs, updates or removes one. It is **the same
/// trait, reached through the same `for_kind`, `get`, `primary` and
/// `perform`**: the split is about when the list is known, not about what is
/// in it, and every question anybody asks the registry sees both. A second
/// lookup path for extension actions is the exact shape (two lists that must
/// agree, with nothing making them agree) that has cost this project a search
/// bug, a launch bug and an icon table.
///
/// `ArcSwap` for the same reason the index uses one: reads happen on every
/// selection change and must not take a lock, and writes happen when an
/// extension is installed, which is rare and human-paced.
pub struct ActionRegistry {
    /// The actions this build was compiled with.
    shipped: Vec<std::sync::Arc<dyn Action>>,
    /// The actions installed extensions declare, replaced whole on a rescan.
    contributed: arc_swap::ArcSwap<Vec<std::sync::Arc<dyn Action>>>,
}

impl ActionRegistry {
    /// Runs an action and records that it happened.
    ///
    /// **Everything that runs an action goes through here**, and the reason is
    /// that the first version of the activity log did not have this: it
    /// recorded inside the Tauri command the window calls, under a comment
    /// claiming that was the one place every action passed through. It was
    /// not. A key bound to an action reaches the registry directly and never
    /// touches that command, so the hotkey path recorded nothing, which is the
    /// one path where undo matters most because the launcher has already
    /// closed.
    ///
    /// Same shape as the search that chained four record lists while launch
    /// looked in one: two routes to the same thing with nothing making them
    /// agree.
    pub async fn perform(
        &self,
        ctx: &ActionCtx,
        action: &dyn Action,
        object: &Object,
    ) -> Result<Outcome, String> {
        let mut outcome = action.run(ctx, object).await?;

        /*
         * Recorded here, and the entry's id goes back with the result.
         *
         * The window used to be handed the undo descriptor itself and to hand
         * it back on Ctrl+Z, which never touched the log. So the entry stayed
         * undoable and "Undo Last Action", or the Activity panel, would
         * cheerfully do the same thing again: move a file back to a folder it
         * was already in, or restore a clipboard over the one just restored.
         * Naming the entry instead means the log decides, once.
         */
        let id = crate::activity::record(ctx, action.title(), &object.title, &outcome);
        outcome.undone_by = outcome.undo.as_ref().and(id).filter(|id| *id != 0);

        Ok(outcome)
    }

    pub fn new(actions: Vec<Box<dyn Action>>) -> Self {
        Self {
            shipped: actions.into_iter().map(std::sync::Arc::from).collect(),
            contributed: arc_swap::ArcSwap::from_pointee(Vec::new()),
        }
    }

    /// Replaces what installed extensions contribute.
    ///
    /// Whole, never merged. An extension being removed is a shorter list, and
    /// a merge could only ever add: an action whose extension was uninstalled
    /// would keep its row in the panel and, pressed, would start a bundle that
    /// is not on disk any more.
    ///
    /// Called from one place, [`crate::adopt_commands`], which is also the one
    /// place the index's commands are replaced. Two rescans of the same thing
    /// with nothing making them agree is how the panel ends up offering an
    /// action for an extension the index has already forgotten.
    pub fn contribute(&self, actions: Vec<std::sync::Arc<dyn Action>>) {
        self.contributed.store(std::sync::Arc::new(actions));
    }

    /// Everything there is, shipped first.
    ///
    /// Shipped first is a design decision rather than bookkeeping: it is what
    /// puts an extension's contributed action **below** every action Sill
    /// itself offers on a file, so installing something cannot quietly take
    /// the top of somebody's panel.
    fn everything(&self) -> Vec<std::sync::Arc<dyn Action>> {
        let mut out = self.shipped.clone();
        out.extend(self.contributed.load().iter().cloned());
        out
    }

    /// What can be done to this kind, primary first.
    pub fn for_kind(&self, kind: ObjectKind) -> Vec<std::sync::Arc<dyn Action>> {
        let mut found: Vec<std::sync::Arc<dyn Action>> = self
            .everything()
            .into_iter()
            .filter(|a| a.accepts(kind))
            .collect();

        // Primary first, then registration order. Sorting by title instead
        // would reshuffle the panel whenever an action is renamed, and the
        // muscle memory is in the position rather than the name.
        found.sort_by_key(|a| !a.is_primary(kind));
        found
    }

    /// What Enter does for this kind.
    ///
    /// Only ever something Sill ships, and that is two guards rather than one.
    /// `contributed` is not searched at all here, and even if it were,
    /// [`Self::everything`] puts what Sill ships first so `find` reaches it
    /// first. Sabotaging either alone leaves the other holding, which is why
    /// `tests/actions.rs::nothing_contributed_can_take_enter_even_if_it_claims_it`
    /// only fails when both are broken together.
    ///
    /// Worth two, because Enter is the key somebody presses without looking
    /// and this is the one thing installing a stranger's extension must never
    /// be able to change.
    pub fn primary(&self, kind: ObjectKind) -> Option<std::sync::Arc<dyn Action>> {
        self.shipped
            .iter()
            .find(|a| a.accepts(kind) && a.is_primary(kind))
            .cloned()
    }

    pub fn get(&self, id: &str) -> Option<std::sync::Arc<dyn Action>> {
        self.everything().into_iter().find(|a| a.id() == id)
    }

    /// What the window draws for this kind, with the chords it runs on.
    ///
    /// Takes the settings rather than reading them, because the registry is
    /// built once at startup and the shortcuts change while it is alive.
    pub fn describe(
        &self,
        kind: ObjectKind,
        keys: &crate::action_keys::Settings,
    ) -> Vec<ActionInfo> {
        self.for_kind(kind)
            .into_iter()
            .map(|a| ActionInfo {
                id: a.id().to_string(),
                title: a.title().to_string(),
                primary: a.is_primary(kind),
                shortcut: crate::action_keys::effective(keys, a.id(), a.shortcut()),
            })
            .collect()
    }

    /// Every action, whatever kind it acts on, with the chord it ships with.
    ///
    /// For the settings screen, which lists all of them: a key that runs an
    /// action is a fact about the action rather than about the kind it was
    /// found under, and the same action reached from a file and from a folder
    /// is one row.
    pub fn all(&self) -> Vec<(String, String, Option<crate::action_keys::Shortcut>)> {
        self.everything()
            .into_iter()
            .map(|a| (a.id().to_string(), a.title().to_string(), a.shortcut()))
            .collect()
    }

    /// Every registered id, for the tests that check they are unique.
    pub fn ids(&self) -> Vec<String> {
        self.everything()
            .into_iter()
            .map(|a| a.id().to_string())
            .collect()
    }
}

/// Reverses what an [`Outcome`] said could be reversed.
pub fn undo(ctx: &ActionCtx, undo: &Undo) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    match undo {
        Undo::RestoreClipboard { text } => {
            ctx.app
                .clipboard()
                .write_text(text.clone())
                .map_err(|err| format!("could not restore the clipboard: {err}"))?;
            Ok("Clipboard restored".to_string())
        }

        Undo::DeleteFile { path, name } => {
            let path = std::path::Path::new(path);

            // Already gone is not a failure. Somebody may have deleted it
            // themselves, and the end state is the one that was asked for.
            if !path.exists() {
                return Ok(format!("{name} was already gone"));
            }

            std::fs::remove_file(path).map_err(|err| format!("could not remove {name}: {err}"))?;

            Ok(format!("{name} removed"))
        }

        Undo::MovePath {
            path,
            back_to,
            name,
        } => {
            let landed = crate::files_ops::move_to(
                std::path::Path::new(path),
                std::path::Path::new(back_to),
            )?;

            Ok(format!(
                "{name} put back in {}",
                crate::files_ops::name_of(landed.parent().unwrap_or(&landed))
            ))
        }

        Undo::RestoreWindow {
            id,
            rect,
            maximized,
            title,
        } => {
            // The window may have closed since. That is not a failure worth
            // an error dialog, but it is worth saying, because otherwise undo
            // silently appears to have worked.
            if crate::windowing::find(*id).is_none() {
                return Err(format!("{title} has closed"));
            }

            if *maximized {
                crate::windowing::maximize(*id)?;
            } else {
                crate::windowing::place(*id, *rect)?;
            }

            Ok(format!("{title} put back"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blank answer is no answer.
    ///
    /// The one rule in the context a test can reach, and the one that costs
    /// something when it is wrong: renaming to `""` asks the filesystem for a
    /// file with no name, and moving to `""` asks it to move something into
    /// whatever directory this process happens to be sitting in.
    ///
    /// `""` is not hypothetical. A model's tool arguments have no absent, only
    /// empty, so every action a model runs arrives here with a blank answer
    /// unless it gave one.
    #[test]
    fn an_answer_that_is_only_whitespace_is_no_answer() {
        assert_eq!(meant(None), None);
        assert_eq!(meant(Some(String::new())), None);
        assert_eq!(meant(Some("   ".to_string())), None);
        assert_eq!(meant(Some("\t\r\n".to_string())), None);
    }

    /// A real one survives, with the whitespace around it taken off.
    ///
    /// Trailing space matters more than it looks: Windows silently drops one
    /// from a file name, so `notes.md ` and `notes.md` are the same file under
    /// two names and one of them is a name nothing can open afterwards.
    #[test]
    fn an_answer_arrives_with_the_whitespace_taken_off_it() {
        assert_eq!(
            meant(Some("notes.md".to_string())).as_deref(),
            Some("notes.md")
        );
        assert_eq!(
            meant(Some("  notes.md \n".to_string())).as_deref(),
            Some("notes.md")
        );
        assert_eq!(
            meant(Some(r"  C:\Users\me\Archive  ".to_string())).as_deref(),
            Some(r"C:\Users\me\Archive")
        );
    }

    /// The space inside a name is not whitespace to be tidied away.
    #[test]
    fn only_the_edges_are_trimmed() {
        assert_eq!(
            meant(Some(" my notes.md ".to_string())).as_deref(),
            Some("my notes.md")
        );
    }
}
