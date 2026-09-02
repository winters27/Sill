//! What the About and Advanced panels report.

use std::collections::BTreeMap;

use tauri::{AppHandle, Manager, State};

use crate::everything_ipc;
use crate::registry::CommandRecord;
use crate::state::{data_dir, RegistryState};

/// What the About and Advanced panels report.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Diagnostics {
    version: String,
    data_dir: String,
    /// Everything responds to an IPC query, so file search works.
    everything_running: bool,
    indexed_commands: usize,
    /// Distinct entries that have ever been launched.
    launched_entries: usize,
    /// One per installed extension, with how many commands it contributes.
    extensions: Vec<ExtensionInfo>,
    /// Whether the machine has the interpreter extensions run in.
    ///
    /// Reported rather than discovered on first use. Extensions are Node
    /// programs, which is a requirement nothing in the application had ever
    /// mentioned: the first sign of it was a spawn failing with "the system
    /// cannot find the file specified", naming a file the person reading it
    /// had never heard of.
    node_installed: bool,
    /// How many entries each source contributed, for the Sources panel.
    by_source: Vec<SourceCount>,
    /// Whether Sill believes the keyboard hook is installed.
    ///
    /// Believing is the operative word, and why the count beside it exists.
    keyboard_hook_installed: bool,
    /// Keystrokes that hook has actually been called for.
    ///
    /// **Installed with this stuck at zero is the signature of a hook Windows
    /// removed**, which it does silently to any low-level hook whose callback
    /// runs long, and which leaves snippet expansion, the hyper key and
    /// double-tap all dead at once with nothing to look at. The dictation hook
    /// reports the same pair for the same reason.
    keyboard_keys_seen: u64,
}

#[derive(serde::Serialize)]
pub(crate) struct ExtensionInfo {
    id: String,
    title: String,
    commands: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct SourceCount {
    mode: String,
    count: usize,
}

#[tauri::command]
pub(crate) async fn diagnostics(
    app: AppHandle,
    registry: State<'_, RegistryState>,
) -> Result<Diagnostics, String> {
    let guard = registry.inner.lock().await;

    /*
     * Asked before the record is built, and asked of the expander rather than
     * of the preferences: whether the hook is installed is a fact about the
     * machine, and the whole point of reporting it is that it can disagree
     * with what the settings say.
     */
    let hook = app
        .try_state::<crate::snippets::expander::Expander>()
        .map(|expander| crate::snippets::expander::facts(&expander))
        .unwrap_or((false, 0));

    Ok(Diagnostics {
        version: app.package_info().version.to_string(),
        data_dir: data_dir(&app).to_string_lossy().into_owned(),
        // Asked live rather than cached: Everything is a separate program the
        // user can quit at any moment, so a remembered answer goes stale.
        everything_running: everything_ipc::available(),
        indexed_commands: guard.commands.len(),
        launched_entries: guard.frecency.len(),
        // Asked live for the same reason Everything is: somebody can install
        // Node while Sill is open, and the panel showing this is exactly where
        // they would look afterwards.
        node_installed: crate::host::node_exe().is_some(),
        extensions: extension_summary(&guard.commands),
        by_source: source_summary(&guard.commands),
        keyboard_hook_installed: hook.0,
        keyboard_keys_seen: hook.1,
    })
}

/// Installed extensions, in display order, with their command counts.
pub(crate) fn extension_summary(commands: &[CommandRecord]) -> Vec<ExtensionInfo> {
    let mut seen: BTreeMap<&str, (&str, usize)> = BTreeMap::new();

    for command in commands {
        // Only extension commands have a mode a host would run; everything
        // else is a shortcut, an executable or a settings page.
        if command.mode != "view" && command.mode != "no-view" {
            continue;
        }
        let entry = seen
            .entry(&command.extension)
            .or_insert((&command.extension_title, 0));
        entry.1 += 1;
    }

    seen.into_iter()
        .map(|(id, (title, commands))| ExtensionInfo {
            id: id.to_string(),
            title: title.to_string(),
            commands,
        })
        .collect()
}

/// How many entries each source contributed.
pub(crate) fn source_summary(commands: &[CommandRecord]) -> Vec<SourceCount> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for command in commands {
        *counts.entry(command.mode.as_str()).or_default() += 1;
    }

    let mut out: Vec<SourceCount> = counts
        .into_iter()
        .map(|(mode, count)| SourceCount {
            mode: mode.to_string(),
            count,
        })
        .collect();

    // Largest first: the useful question is which source dominates the index.
    out.sort_by(|a, b| b.count.cmp(&a.count));
    out
}
