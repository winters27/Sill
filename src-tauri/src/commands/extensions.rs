//! Driving a loaded extension command.

use serde_json::Value;
use tauri::State;

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
