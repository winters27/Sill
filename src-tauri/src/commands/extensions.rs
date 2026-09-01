//! Driving a loaded extension command.

use serde_json::Value;
use tauri::State;

use crate::exthost::LoadOptions;
use crate::host::{host_of, running_host};
use crate::state::HostState;

#[tauri::command]
pub(crate) async fn load_extension(
    app: tauri::AppHandle,
    state: State<'_, HostState>,
    entrypoint: String,
    extension: String,
    command: String,
) -> Result<String, String> {
    let host = host_of(&state).await?;
    let mut opts = LoadOptions::view(entrypoint, &extension, &command);
    opts.capabilities = crate::exthost::grants::for_extension(&app, &extension);
    host.load(&opts).await.map_err(|e| e.to_string())
}

/// Fires a callback in a running command.
///
/// Deliberately does not start the host: a handler belongs to a session, and
/// with no host there is no session for it to belong to.
#[tauri::command]
pub(crate) async fn activate_handler(
    state: State<'_, HostState>,
    session: String,
    handler: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let host = running_host(&state)
        .await
        .ok_or_else(|| format!("no such session: {session}"))?;

    host.activate_handler(&session, &handler, args.unwrap_or(Value::Array(vec![])))
        .await
        .map_err(|e| e.to_string())
}

/// Tears down a running command.
///
/// Also does not start the host. The window unloads on its way back to the
/// root list, and after an idle shutdown that would otherwise respawn Node
/// purely to be told the session it is closing no longer exists.
#[tauri::command]
pub(crate) async fn unload_extension(
    state: State<'_, HostState>,
    session: String,
) -> Result<bool, String> {
    let Some(host) = running_host(&state).await else {
        return Ok(false);
    };

    host.unload(&session).await.map_err(|e| e.to_string())
}

/// Installs an extension from a folder on this machine.
///
/// Everything about how is in [`crate::extension_install`]; this is the
/// transport and nothing else. It is `async` because bundling is a subprocess
/// per command and a large extension is long enough to be worth not holding
/// the window still for.
#[cfg(windows)]
#[tauri::command]
pub(crate) async fn install_extension(
    app: tauri::AppHandle,
    folder: String,
) -> Result<crate::extension_install::Installed, String> {
    let source = std::path::PathBuf::from(folder);

    // Off the UI thread: esbuild is a subprocess and the index is a file.
    tauri::async_runtime::spawn_blocking(move || {
        let origin = crate::store::Origin::folder(&source, crate::state::now_seconds());
        crate::extension_install::install(&app, &source, &origin)
    })
    .await
    .map_err(|err| format!("the install did not finish: {err}"))?
}

/// One permission, and how it reads to somebody deciding about it.
///
/// The words come from `permission::plainly` rather than from a table in the
/// settings window, so the screen that lists a permission and the card that
/// asked for it cannot describe the same thing differently.
#[derive(serde::Serialize)]
pub(crate) struct Permission {
    capability: crate::action::Capability,
    plainly: &'static str,
}

/// What one extension has been allowed to reach.
#[derive(serde::Serialize)]
pub(crate) struct GrantedTo {
    extension: String,
    permissions: Vec<Permission>,
}

/// What every extension has been allowed to reach.
///
/// The screen this feeds is the one the audit said nobody in this category
/// has: not a list of what is installed, but of what each one can touch.
#[tauri::command]
pub(crate) fn extension_grants(
    grants: State<'_, std::sync::Arc<crate::exthost::grants::Granted>>,
) -> Vec<GrantedTo> {
    grants
        .everything()
        .into_iter()
        .map(|(extension, held)| GrantedTo {
            extension,
            permissions: held
                .into_iter()
                .map(|capability| Permission {
                    capability,
                    plainly: crate::exthost::permission::plainly(&capability),
                })
                .collect(),
        })
        .collect()
}

/// Takes one permission back.
///
/// The extension is asked again the next time it tries, rather than being
/// refused from then on: revoking is "ask me about this again", not "never".
#[tauri::command]
pub(crate) fn revoke_extension_grant(
    grants: State<'_, std::sync::Arc<crate::exthost::grants::Granted>>,
    extension: String,
    capability: crate::action::Capability,
) {
    grants.revoke(&extension, &capability);
}
