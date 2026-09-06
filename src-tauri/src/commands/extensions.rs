//! Driving a loaded extension command.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::host::running_host;
use crate::state::HostState;

/*
 * `load_extension` used to live here and has been removed.
 *
 * Nothing called it. The window opens an extension by running the command's
 * action, which is `actions::RunExtensionCommand`, and that is the path with
 * the manifest, the preferences, the required-preference check, the assets
 * folder, the support folder and the declared arguments on it. This one had a
 * bare entrypoint and a capability list, so anything wired to it would have
 * started every extension as though the user had cleared every setting.
 *
 * It also held the only measurement of what an extension costs to open, on a
 * path nobody took, which is why the answer to "which extension is slow" was
 * a shrug with a `Timings` field behind it. The measurement is on the real
 * path now.
 */

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

/// A picture out of a running extension's own `assets`, as a data URI.
///
/// A view writes `icon: "files.png"` and means the file beside its code. The
/// window cannot open that file and does not know where the extension lives,
/// so it names the session and the picture, and this finds the extension
/// behind the session, refuses a name that climbs out of `assets`, and reads
/// the picture the way command icons are read at install. `None` for a name
/// that is not a picture Sill can read, which the window letters as before.
#[tauri::command]
pub(crate) async fn extension_asset(
    app: AppHandle,
    state: State<'_, HostState>,
    session: String,
    name: String,
) -> Result<Option<String>, String> {
    let host = running_host(&state)
        .await
        .ok_or_else(|| format!("no such session: {session}"))?;
    let extension = host
        .extension_of(&session)
        .ok_or_else(|| format!("no such session: {session}"))?;

    let home = crate::store::extensions_home(&crate::state::data_dir(&app));
    let Some(path) = crate::extension_install::asset_path(&home, &extension, &name) else {
        return Ok(None);
    };

    // A file read, off the async runtime's threads.
    let path = path.to_string_lossy().replace('\\', "/");
    tokio::task::spawn_blocking(move || crate::icons::image_file(&path))
        .await
        .map_err(|err| err.to_string())
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

/// What one extension has cost, and what it is costing.
///
/// Openings are per extension because that is what somebody installs and
/// removes. What is running is per command, because an extension is not one
/// program: "this one is using 47 MB" is half an answer when only one of its
/// four commands is the reason.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionCost {
    pub extension: String,
    /// Milliseconds to first screen, averaged, when Sill had to start Node.
    pub cold_ms: Option<u64>,
    pub cold_opens: u64,
    /// The same, when it did not.
    pub warm_ms: Option<u64>,
    pub warm_opens: u64,
    /// The most one of its commands was holding when it was closed, in bytes.
    ///
    /// The figure that makes this a comparison. A launcher has one command
    /// loaded at a time, so what is running is one number, and somebody
    /// hunting for the expensive extension has closed the other three by the
    /// time they come to look.
    pub held_bytes: Option<u64>,
    /// Its commands that are loaded right now, with what each holds.
    pub running: Vec<crate::exthost::Running>,
}

/// What each extension has cost to open, and what its commands hold now.
///
/// **Nothing is started to answer this.** The openings are numbers Sill
/// already wrote down as they happened, and the live half is asked only of a
/// host that is already up, so opening this panel on a machine that has not
/// run an extension today costs a lock and an empty list rather than a Node
/// process.
///
/// Openings are measured for as long as Sill is running and are not written to
/// disk. Persisting them would mean a write per launch to answer a question
/// nobody asks between runs: somebody working out which of their extensions is
/// the slow one opens them and looks, and the comparison they want is of this
/// machine as it is now rather than of an average taken over a month.
#[tauri::command]
pub(crate) async fn extension_resources(
    state: State<'_, HostState>,
    timings: State<'_, crate::timing::Timings>,
) -> Result<Vec<ExtensionCost>, String> {
    let openings = timings.report().extensions;

    let running = match running_host(&state).await {
        Some(host) => host.worker_readings().await,
        None => Vec::new(),
    };

    Ok(gather_costs(openings, running))
}

/// Puts the two halves together, in the order the question is asked in.
///
/// A free function so the joining can be tested without a Node process or a
/// window. What it has to get right is the union: an extension that is running
/// but has no opening recorded still gets a row, because a command that
/// crashed before it drew anything never completed an opening and is exactly
/// the one somebody is looking for.
fn gather_costs(
    openings: Vec<crate::timing::Opening>,
    running: Vec<crate::exthost::Running>,
) -> Vec<ExtensionCost> {
    let mut costs: Vec<ExtensionCost> = openings
        .into_iter()
        .map(|opening| ExtensionCost {
            cold_ms: opening.cold.as_ref().map(|it| it.average_us() / 1_000),
            cold_opens: opening.cold.as_ref().map(|it| it.count).unwrap_or(0),
            warm_ms: opening.warm.as_ref().map(|it| it.average_us() / 1_000),
            warm_opens: opening.warm.as_ref().map(|it| it.count).unwrap_or(0),
            held_bytes: opening.held_bytes,
            running: Vec::new(),
            extension: opening.name,
        })
        .collect();

    for one in running {
        match costs.iter_mut().find(|it| it.extension == one.extension) {
            Some(cost) => cost.running.push(one),
            None => costs.push(ExtensionCost {
                extension: one.extension.clone(),
                cold_ms: None,
                cold_opens: 0,
                warm_ms: None,
                warm_opens: 0,
                held_bytes: None,
                running: vec![one],
            }),
        }
    }

    costs
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

/**
The file picker behind `Form.FilePicker`, which is Windows' own.

## Why this is in Rust and not three lines of `@tauri-apps/plugin-dialog`

The launcher window is not on `capabilities/file-picker.json`. Two windows are,
`ask` and `settings`, and the main one is deliberately not: a window that can
open a dialog is a window that can be talked into opening one, and the main
window is the one an extension draws into. So the frontend cannot call the
plugin here even if somebody wanted it to, and this is the door instead.

## What it hands back, and what it does not

**Paths, and only the paths the dialog returned.** Nothing here reads a file,
lists a folder or asks for so much as a size, and `verify:source` refuses this
function if it grows a filesystem call. That is the whole of the answer to
"could an extension use a picker to look inside a folder nobody granted it":
picking a folder yields the folder's own path, its contents are not looked at,
and reading anything at that path afterwards is `fs` inside the worker, which
`patch-require.ts` refuses without `fileRead` exactly as it did before.

## Why no permission is charged for opening it

Every other door into somebody's disk is charged for because the extension
chooses what it touches. This one it does not: Windows draws the dialog, the
person reads a real file name in it and presses Open, and what comes back is
that one answer. Charging `fileRead` to draw the field would also be a promise
Sill does not keep, because the grant would let the extension read the whole
disk while the field only ever produces what was chosen.

The session is checked, though, and looked up rather than believed. An id that
does not belong to a running command opens nothing, so this is not a way for
anything else in the window to make dialogs appear.

## Why the launcher stops dismissing while it is up

The main window closes when it loses focus, and a dialog takes focus. Without
this the picker would appear, the launcher would vanish behind it, and pressing
Open would answer a form that had already been closed. The setting is put back
whatever happens, including when the dialog is dismissed with nothing chosen.
*/
#[tauri::command]
pub(crate) async fn pick_files(
    app: tauri::AppHandle,
    state: State<'_, HostState>,
    session: String,
    directories: bool,
    multiple: bool,
) -> Result<Vec<String>, String> {
    use tauri::Manager;
    use tauri_plugin_dialog::DialogExt;

    if crate::host::extension_of(&state, &session).await.is_none() {
        return Err("that picker does not belong to a running extension".to_string());
    }

    // Absent in a test harness, and its absence is not a reason to refuse: the
    // window that would have been dismissed is not there either.
    let blur = app.try_state::<crate::DismissOnBlur>();
    if let Some(blur) = &blur {
        blur.set(false);
    }

    let dialog = app.dialog().file().set_title(if directories {
        "Choose a folder"
    } else {
        "Choose a file"
    });

    let chosen = match (directories, multiple) {
        (true, true) => dialog.blocking_pick_folders(),
        (true, false) => dialog.blocking_pick_folder().map(|one| vec![one]),
        (false, true) => dialog.blocking_pick_files(),
        (false, false) => dialog.blocking_pick_file().map(|one| vec![one]),
    };

    if let Some(blur) = &blur {
        blur.set(true);
    }

    // Nothing chosen is an empty list rather than an error. Somebody opened the
    // dialog and changed their mind, which is an ordinary thing to do, and a
    // form field that reported it as a failure would be a form that says
    // something went wrong every time somebody looks and decides not to.
    Ok(chosen
        .unwrap_or_default()
        .into_iter()
        .filter_map(|one| one.into_path().ok())
        .map(|one| one.to_string_lossy().into_owned())
        .collect())
}

#[cfg(test)]
mod what_they_cost {
    use super::gather_costs;
    use crate::exthost::Running;
    use crate::timing::{Cost, Opening};

    fn opened(name: &str, warm_us: u64) -> Opening {
        Opening {
            name: name.to_string(),
            cold: None,
            warm: Some(Cost {
                name: name.to_string(),
                count: 1,
                total_us: warm_us,
                slowest_us: warm_us,
            }),
            held_bytes: None,
        }
    }

    fn running(extension: &str, command: &str) -> Running {
        Running {
            session: format!("{extension}/{command}"),
            extension: extension.to_string(),
            command: command.to_string(),
            heap_bytes: Some(63 * 1024 * 1024),
            heap_limit_bytes: 512 * 1024 * 1024,
            core_percent: 10.0,
            answering: true,
        }
    }

    /// A reading lands on the extension it belongs to.
    ///
    /// The whole panel is one comparison, so an extension wearing another's
    /// memory reading would be worse than no reading at all: it names the
    /// wrong culprit with a number beside it.
    #[test]
    fn a_reading_goes_to_the_extension_it_came_from() {
        let costs = gather_costs(
            vec![opened("emoji", 78_000), opened("uuid-generator", 48_000)],
            vec![running("emoji", "Search Emoji")],
        );

        let emoji = costs
            .iter()
            .find(|it| it.extension == "emoji")
            .expect("emoji");
        let uuid = costs
            .iter()
            .find(|it| it.extension == "uuid-generator")
            .expect("uuid-generator");

        assert_eq!(emoji.running.len(), 1);
        assert_eq!(emoji.running[0].command, "Search Emoji");
        assert!(
            uuid.running.is_empty(),
            "a reading was hung on an extension it did not come from"
        );
    }

    /// An extension running with no opening recorded still gets a row.
    ///
    /// This is not a corner case, it is the interesting one. An opening is
    /// completed by the extension drawing, so a command that is stuck before
    /// its first render has no opening at all, and dropping it would hide the
    /// exact thing somebody opened this panel to find.
    #[test]
    fn a_command_that_never_drew_is_still_reported() {
        let mut stuck = running("stuck", "Never Draws");
        stuck.answering = false;
        stuck.heap_bytes = None;
        stuck.core_percent = 99.4;

        let costs = gather_costs(Vec::new(), vec![stuck]);

        assert_eq!(costs.len(), 1, "a running command with no opening vanished");
        assert_eq!(costs[0].extension, "stuck");
        assert!(costs[0].warm_ms.is_none(), "an opening was invented for it");
        assert!(!costs[0].running[0].answering);
    }

    /// Two commands of one extension are two lines under one name.
    #[test]
    fn every_running_command_of_one_extension_is_kept() {
        let costs = gather_costs(
            vec![opened("emoji", 78_000)],
            vec![running("emoji", "Search Emoji"), running("emoji", "Recent")],
        );

        assert_eq!(costs.len(), 1);
        assert_eq!(costs[0].running.len(), 2);
    }

    /// Microseconds become milliseconds once, here, rather than in the window.
    #[test]
    fn the_window_is_handed_milliseconds() {
        let costs = gather_costs(vec![opened("emoji", 78_400)], Vec::new());

        assert_eq!(costs[0].warm_ms, Some(78));
        assert_eq!(costs[0].warm_opens, 1);
        assert_eq!(costs[0].cold_ms, None);
    }
}

/// What a command's fields were left set to last time.
///
/// Raycast's `storeValue`: a dropdown or a form field marked with it opens on
/// what somebody last chose rather than on what its author defaulted to. The
/// window asks for these when a command starts and applies them itself, which
/// is where they have to be applied: it is what holds a dropdown's selection
/// and a form's values, and an extension is never told anything but the answer.
///
/// Empty for a command nobody has used, which is the same shape as one whose
/// fields are all fresh, and the window treats both the same way.
#[tauri::command]
pub(crate) fn extension_stored_fields(
    state: State<'_, HostState>,
    extension: String,
    command: String,
) -> serde_json::Map<String, Value> {
    state.api.storage().fields_for(&extension, &command)
}

/// Remembers what a field was set to, for the next time the command runs.
///
/// Failures are reported and not returned. The caller is somebody choosing a
/// feed from a dropdown, the choice has already been made and acted on, and
/// there is nothing they could do with "the launcher could not write that
/// down" except see an error about something that worked.
#[tauri::command]
pub(crate) fn remember_extension_field(
    state: State<'_, HostState>,
    extension: String,
    command: String,
    id: String,
    value: Value,
) {
    let key = crate::exthost::storage::field_key(&extension, &command, &id);

    if let Err(err) = state
        .api
        .storage()
        .set(crate::exthost::storage::FIELDS, &key, &value)
    {
        crate::say!("could not remember {key}: {err}");
    }
}
