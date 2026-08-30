//! Running whatever the user picked.
//!
//! This was a two-hundred-line chain comparing an index entry's `mode` string
//! against eleven values. It is now a lookup: what kind of thing is this, what
//! does Enter do to that kind, do it. The behaviours themselves moved to
//! `crate::actions` unchanged.
//!
//! The point of the move is not tidiness. It is that pressing Enter, choosing
//! from the action panel, binding a shortcut and (later) a workflow step or a
//! tool an AI may call are now four ways into one implementation rather than
//! four implementations that drift.

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::action::{ActionCtx, ActionRegistry};
use crate::object::Object;
use crate::state::{now_seconds, RegistryState};

/// Runs a command from the root list.
///
/// Frecency is recorded before the action runs rather than after, so a command
/// that fails still counts as chosen. The user picked it; that is the signal
/// being learned, not whether it worked.
#[tauri::command]
pub(crate) async fn launch_command(
    app: AppHandle,
    state: State<'_, RegistryState>,
    id: String,
    // What was in the field when this was chosen, so Sill can learn the
    // user's own shorthand for it. Typing `ggm` and choosing Gmail says
    // something the id alone cannot: not "Gmail is popular" but "`ggm` means
    // Gmail". Optional, because a launch can come from places with no query.
    query: Option<String>,
) -> Result<LaunchedCommand, String> {
    let record = {
        let mut registry = state.inner.lock().await;
        let record = registry
            .commands
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| format!("no such command: {id}"))?;

        let now = now_seconds();
        registry.frecency.record(&id, now);

        // The query as it was typed, not as it was matched. The shorthand is
        // the thing worth learning; the full name teaches nothing.
        if let Some(query) = query.as_deref() {
            registry.frecency.record_query(query, &id, now);
            registry.frecency.remember(query);
        }
        let path = registry.frecency_path.clone();
        if let Err(err) = registry.frecency.save(&path) {
            // Losing ranking history is not worth failing a launch over.
            crate::say!("could not save frecency: {err}");
        }
        record
    };

    let object = Object::from_record(&record)
        .ok_or_else(|| format!("{} is a kind of thing Sill cannot act on", record.title))?;

    let actions = app.state::<ActionRegistry>();
    let action = actions
        .primary(object.kind)
        .ok_or_else(|| format!("nothing is bound to Enter for {}", record.title))?;

    let outcome = action.run(&ActionCtx { app: app.clone() }, &object).await?;

    Ok(LaunchedCommand {
        // Empty rather than absent: the window has always read this as a
        // string and an extension command is the only thing that fills it.
        session: outcome.session.unwrap_or_default(),
        title: record.title,
        extension_title: record.extension_title,
        mode: record.mode,
    })
}

/// What the UI needs to show once a command is running.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchedCommand {
    pub session: String,
    pub title: String,
    pub extension_title: String,
    /// "view" or "no-view"; the UI stays at the root list for no-view.
    pub mode: String,
}

/// Performs an action Raycast implements itself rather than handing to the
/// extension.
///
/// `Action.CopyToClipboard` and friends carry no `onAction`; they declare what
/// they want done through their props and the launcher is expected to do it.
/// Treating them as broken because they have no callback would silently kill
/// the most common action in the whole ecosystem.
#[tauri::command]
pub(crate) async fn perform_builtin(
    app: AppHandle,
    tag: String,
    props: Value,
) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    /// Raycast lets `content` be a string, a number, or a shaped object.
    fn text_of(value: Option<&Value>) -> Option<String> {
        match value? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Object(map) => map
                .get("text")
                .or_else(|| map.get("html"))
                .or_else(|| map.get("file"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            _ => None,
        }
    }

    match tag.as_str() {
        "Action.CopyToClipboard" => {
            let content = text_of(props.get("content"))
                .ok_or_else(|| "that action carried nothing to copy".to_string())?;
            app.clipboard()
                .write_text(content)
                .map_err(|e| e.to_string())?;
            Ok("Copied".to_string())
        }

        "Action.OpenInBrowser" | "Action.Open" => {
            let target = props
                .get("url")
                .or_else(|| props.get("target"))
                .and_then(Value::as_str)
                .ok_or_else(|| "that action carried nothing to open".to_string())?;

            tauri_plugin_opener::open_url(target, None::<&str>).map_err(|e| e.to_string())?;
            Ok("Opened".to_string())
        }

        "Action.Paste" => {
            let content = text_of(props.get("content"))
                .ok_or_else(|| "that action carried nothing to paste".to_string())?;
            app.clipboard()
                .write_text(content)
                .map_err(|e| e.to_string())?;

            // It said "paste injection is not built yet" and only copied,
            // which was honest at the time and is no longer true: the same
            // synthetic input dictation has always used does this.
            crate::dictation::paste::deliver(&app);
            Ok("Pasted".to_string())
        }

        other => Err(format!("{other} is not a built-in Sill can perform")),
    }
}

/// An object the window is pointing at.
///
/// The window echoes back the fields Rust already sent it in a search result
/// rather than inventing any, which is what lets a file work: a file result
/// comes from Everything at query time and was never in the index, so there
/// is nothing to look it up in.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectRef {
    id: String,
    mode: String,
    /// The result's `entrypoint`: a path, a panel, a stored id, a value.
    target: String,
    title: String,
}

impl ObjectRef {
    fn into_object(self) -> Result<Object, String> {
        let kind = crate::object::ObjectKind::from_mode(&self.mode)
            .ok_or_else(|| format!("{} is a kind of thing Sill cannot act on", self.title))?;

        Ok(Object {
            kind,
            id: self.id,
            target: self.target,
            title: self.title,
            mode: self.mode,
        })
    }
}

/// What can be done to the selected result.
///
/// Keyed on the mode rather than looked up by id, because the answer depends
/// only on what kind of thing it is, and because a file result is not in any
/// index to be looked up in.
#[tauri::command]
pub(crate) fn actions_for(
    actions: State<'_, ActionRegistry>,
    mode: String,
) -> Vec<crate::action::ActionInfo> {
    let out = crate::object::ObjectKind::from_mode(&mode)
        .map(|kind| actions.describe(kind))
        .unwrap_or_default();
    out
}

/// Runs one action against one object.
///
/// Frecency is deliberately not recorded here. It learns what you open, and
/// copying a path or opening a containing folder is looking at something
/// rather than reaching for it. Enter still records, through `launch_command`.
#[tauri::command]
pub(crate) async fn run_action(
    app: AppHandle,
    action: String,
    object: ObjectRef,
) -> Result<crate::action::Outcome, String> {
    let object = object.into_object()?;

    let registry = app.state::<ActionRegistry>();
    let chosen = registry
        .get(&action)
        .ok_or_else(|| format!("no such action: {action}"))?;

    if !chosen.accepts(object.kind) {
        // Not an error the user caused, so it names both halves: this arrives
        // when the window's idea of the selection has drifted from Rust's.
        return Err(format!(
            "{} cannot be done to {}",
            chosen.title(),
            object.title
        ));
    }

    let outcome = chosen.run(&ActionCtx { app: app.clone() }, &object).await?;

    // The screen now belongs to something else, so the launcher must not put
    // it back on the way out. Decided from the kind rather than from the
    // action, because it is a fact about what was acted on: every way of
    // reaching a window ends with that window in front.
    if object.kind.hands_over_the_screen() {
        crate::summon::forget_foreground();
    }

    Ok(outcome)
}

/// Reverses an action that said it could be reversed.
#[tauri::command]
pub(crate) async fn undo_action(
    app: AppHandle,
    undo: crate::action::Undo,
) -> Result<String, String> {
    crate::action::undo(&ActionCtx { app: app.clone() }, &undo)
}

/// Counts a use of something the window opened by itself.
///
/// Two results are handled entirely in the window and never reach
/// `launch_command`: the clipboard history, which becomes a view rather than
/// a launch, and a quicklink with a hole in it, which takes over the field
/// instead. **Both were therefore invisible to ranking.** `sill:clipboard`
/// had never been recorded once, however often it was opened, so it could
/// never rise in the root list and nothing typed at it could ever be learned.
///
/// Separate from `launch_command` rather than folded into it because these
/// genuinely are not launches; what they share is only that they count.
#[tauri::command]
pub(crate) async fn record_use(
    state: State<'_, RegistryState>,
    id: String,
    query: Option<String>,
    // Whether the query belongs in the history that Up walks back through.
    //
    // The history is what was typed at the root. A query typed into the emoji
    // picker taught something useful about emoji and nothing about the root
    // list, and offering it back there would recall a search that now finds
    // nothing. Defaults to true, because every other caller is the root.
    history: Option<bool>,
) -> Result<(), String> {
    let mut registry = state.inner.lock().await;

    let now = now_seconds();
    registry.frecency.record(&id, now);
    if let Some(query) = query.as_deref() {
        registry.frecency.record_query(query, &id, now);
        if history.unwrap_or(true) {
            registry.frecency.remember(query);
        }
    }

    let path = registry.frecency_path.clone();
    if let Err(err) = registry.frecency.save(&path) {
        crate::say!("could not save frecency: {err}");
    }

    Ok(())
}

/// What was typed before, most recent first.
///
/// For walking back through past queries in an empty field. Only queries that
/// reached something: a launcher offering back the half-finished strings
/// somebody abandoned would mostly be offering them their mistakes.
#[tauri::command]
pub(crate) async fn query_history(state: State<'_, RegistryState>) -> Result<Vec<String>, String> {
    Ok(state.inner.lock().await.frecency.history().to_vec())
}
