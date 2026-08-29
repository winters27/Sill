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
) -> Result<LaunchedCommand, String> {
    let record = {
        let mut registry = state.inner.lock().await;
        let record = registry
            .commands
            .iter()
            .find(|c| c.id == id)
            .cloned()
            .ok_or_else(|| format!("no such command: {id}"))?;

        registry.frecency.record(&id, now_seconds());
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
