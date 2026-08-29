//! Running whatever the user picked.
//!
//! One entry point over every kind of thing the index holds, which is why the
//! body is a chain of modes: an application, a settings page, a snippet, a
//! quicklink, a calculator answer and an extension command are all launched
//! from the same list and have nothing else in common.

use crate::commands::settings::open_settings;
use crate::{dismiss_main, reload_index};

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::exthost::{self, LoadOptions};
use crate::host::host_of;
use crate::state::{now_seconds, HostState, RegistryState};
use crate::{apps, dictation, quicklinks, settings_catalog, snippets};

/// Runs a command from the root list.
///
/// Frecency is recorded before the load rather than after, so a command that
/// crashes on startup still counts as chosen. The user picked it; that is the
/// signal being learned, not whether it worked.
#[tauri::command]
pub(crate) async fn launch_command(
    app: AppHandle,
    hosts: State<'_, HostState>,
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

    // One of Sill's own settings, which opens settings at its panel. The
    // entrypoint IS the panel, so nothing has to be looked up.
    if record.mode == "sill-setting" {
        open_settings(app.clone(), Some(record.entrypoint.clone())).await?;

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    // A snippet is expanded and pasted where the launcher was, so the
    // launcher gets out of the way first.
    if record.mode == "snippet" {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        let expansion = snippets::commands::expand_snippet(app.clone(), record.entrypoint.clone())?;
        app.clipboard()
            .write_text(expansion.text)
            .map_err(|e| format!("Could not copy the snippet: {e}"))?;

        dictation::paste::deliver(&app);

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    // A quicklink with nothing to ask opens immediately. One that wants a
    // query never reaches here: the frontend keeps it, collects the text and
    // calls `open_quicklink` itself, because the asking is the feature.
    if record.mode == "quicklink" {
        quicklinks::commands::open_quicklink(app.clone(), record.entrypoint.clone(), String::new())?;
        dismiss_main(&app);

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    // The answer's entrypoint is the result itself, so launching it is a
    // copy. Nothing is spawned and nothing is indexed.
    if record.mode == "answer" {
        use tauri_plugin_clipboard_manager::ClipboardExt;

        app.clipboard()
            .write_text(record.entrypoint.clone())
            .map_err(|e| format!("Could not copy the answer: {e}"))?;
        dismiss_main(&app);

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    if record.mode == "builtin" {
        match record.entrypoint.as_str() {
            "settings" => open_settings(app.clone(), None).await?,
            "reload" => reload_index(&app),
            // Dismissed first: the launcher is frontmost right now, and a
            // dictation started here has to land in whatever was in front
            // before it, not in Sill.
            "dictate" => {
                dismiss_main(&app);
                let service = app.state::<dictation::service::DictationService>();
                service.start(&app).map_err(String::from)?;
            }
            "snippets" => open_settings(app.clone(), Some("snippets".into())).await?,
            "quicklinks" => open_settings(app.clone(), Some("quicklinks".into())).await?,
            "dictation-history" => open_settings(app.clone(), Some("history".into())).await?,
            "vocabulary" => open_settings(app.clone(), Some("dictation".into())).await?,
            "last-transcription" => {
                use tauri_plugin_clipboard_manager::ClipboardExt;

                let Some(entry) = dictation::history::last(&app) else {
                    return Err("Nothing has been dictated yet".to_string());
                };
                app.clipboard()
                    .write_text(entry.text)
                    .map_err(|e| format!("Could not copy the transcript: {e}"))?;
                dismiss_main(&app);
            }
            other => return Err(format!("unknown Sill command: {other}")),
        }

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    if record.mode == "setting" {
        settings_catalog::launch(&record.entrypoint)?;

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    // Applications and bare executables are launched by the shell, not by the
    // extension host.
    if record.mode == "app" || record.mode == "exe" {
        if let Some(app_id) = record.entrypoint.strip_prefix(apps::APPS_FOLDER) {
            // Packaged apps have no path to open. Explorer resolves an
            // AppUserModelID through the Apps folder, which is how the Start
            // Menu launches them too.
            std::process::Command::new("explorer.exe")
                .arg(format!("{}{}", apps::APPS_FOLDER, app_id))
                .spawn()
                .map_err(|e| format!("could not launch {}: {e}", record.title))?;
        } else {
            tauri_plugin_opener::open_path(&record.entrypoint, None::<&str>)
                .map_err(|e| format!("could not launch {}: {e}", record.title))?;
        }

        return Ok(LaunchedCommand {
            session: String::new(),
            title: record.title,
            extension_title: record.extension_title,
            mode: record.mode,
        });
    }

    // The manifest decides. A no-view command runs and exits without ever
    // rendering, so loading it as a view would leave the UI waiting forever.
    let mode = if record.mode == "no-view" {
        exthost::CommandMode::NoView
    } else {
        exthost::CommandMode::View
    };

    let host = host_of(&hosts).await?;
    let opts = LoadOptions::with_preferences(
        record.entrypoint.clone(),
        &record.extension,
        &record.command,
        mode,
        record.preferences.clone(),
    );
    let session = host.load(&opts).await.map_err(|e| e.to_string())?;

    Ok(LaunchedCommand {
        session,
        title: record.title,
        extension_title: record.extension_title,
        mode: record.mode,
    })
}

/// What the UI needs to show once a command is running.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchedCommand {
    session: String,
    title: String,
    extension_title: String,
    /// "view" or "no-view"; the UI stays at the root list for no-view.
    mode: String,
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
            dictation::paste::deliver(&app);
            Ok("Pasted".to_string())
        }

        other => Err(format!("{other} is not a built-in Sill can perform")),
    }
}
