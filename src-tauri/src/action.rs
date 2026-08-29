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
use serde::Serialize;
use tauri::AppHandle;

use crate::object::{Object, ObjectKind};

/// What an action is allowed to touch.
///
/// Declared rather than inferred, so the question "what can this reach?" has
/// an answer that can be read off a list instead of by tracing code. Nothing
/// enforces these yet; they are here now because retrofitting a permission
/// model onto actions that never declared anything means auditing every one
/// of them at the point it is most expensive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    /// Opens or changes one of Sill's own windows.
    Ui,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Undo {
    /// Put back what was on the clipboard before.
    RestoreClipboard { text: String },
}

/// What happened, and whether it can be reversed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// One line, shown to the user. Past tense, because it already happened.
    pub message: String,
    /// Absent for the great majority of actions. See [`Undo`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo: Option<Undo>,
    /// An extension command that is now running and needs rendering.
    ///
    /// The one piece of an action's result that outlives the action. It is
    /// named for exactly what it is rather than hidden in a general-purpose
    /// bag, because a general-purpose bag is how a type stops meaning
    /// anything.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Outcome {
    pub fn done(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            undo: None,
            session: None,
        }
    }

    pub fn undoable(message: impl Into<String>, undo: Undo) -> Self {
        Self {
            message: message.into(),
            undo: Some(undo),
            session: None,
        }
    }

    pub fn running(message: impl Into<String>, session: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            undo: None,
            session: Some(session.into()),
        }
    }
}

/// Everything an action gets besides the thing it is acting on.
///
/// One field today. It exists as a type rather than a bare `AppHandle` because
/// the next things to go in are known: the foreground application, the current
/// selection, what is on the clipboard. Adding a field to this is a change
/// nobody else has to notice; adding a parameter to every action is not.
pub struct ActionCtx {
    pub app: AppHandle,
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
    fn id(&self) -> &'static str;

    /// What the action panel shows. Imperative, because it is a thing you are
    /// about to do rather than a description of what it does.
    fn title(&self) -> &'static str;

    fn accepts(&self, kind: ObjectKind) -> bool;

    fn capabilities(&self) -> &'static [Capability];

    /// Whether this is what Enter does for this kind.
    ///
    /// Exactly one action per kind should answer yes. The registry checks it.
    fn is_primary(&self, _kind: ObjectKind) -> bool {
        false
    }

    async fn run(&self, ctx: &ActionCtx, object: &Object) -> Result<Outcome, String>;
}

/// An action as the window needs to draw it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub primary: bool,
}

/// Every action Sill knows.
///
/// A plain list rather than a map, because the lookups are "which of these
/// accept this kind" and "which is primary", and both are linear over a set
/// small enough that anything cleverer would be slower to read and no faster
/// to run.
pub struct ActionRegistry {
    actions: Vec<Box<dyn Action>>,
}

impl ActionRegistry {
    pub fn new(actions: Vec<Box<dyn Action>>) -> Self {
        Self { actions }
    }

    /// What can be done to this kind, primary first.
    pub fn for_kind(&self, kind: ObjectKind) -> Vec<&dyn Action> {
        let mut found: Vec<&dyn Action> = self
            .actions
            .iter()
            .map(|a| a.as_ref())
            .filter(|a| a.accepts(kind))
            .collect();

        // Primary first, then registration order. Sorting by title instead
        // would reshuffle the panel whenever an action is renamed, and the
        // muscle memory is in the position rather than the name.
        found.sort_by_key(|a| !a.is_primary(kind));
        found
    }

    /// What Enter does for this kind.
    pub fn primary(&self, kind: ObjectKind) -> Option<&dyn Action> {
        self.actions
            .iter()
            .map(|a| a.as_ref())
            .find(|a| a.accepts(kind) && a.is_primary(kind))
    }

    pub fn get(&self, id: &str) -> Option<&dyn Action> {
        self.actions.iter().map(|a| a.as_ref()).find(|a| a.id() == id)
    }

    /// What the window draws for this kind.
    pub fn describe(&self, kind: ObjectKind) -> Vec<ActionInfo> {
        self.for_kind(kind)
            .into_iter()
            .map(|a| ActionInfo {
                id: a.id(),
                title: a.title(),
                primary: a.is_primary(kind),
            })
            .collect()
    }

    /// Every registered id, for the tests that check they are unique.
    pub fn ids(&self) -> Vec<&'static str> {
        self.actions.iter().map(|a| a.id()).collect()
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
    }
}
