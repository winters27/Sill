//! What the About and Advanced panels report.

use std::collections::BTreeMap;

use tauri::{AppHandle, State};

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
    /// How many entries each source contributed, for the Sources panel.
    by_source: Vec<SourceCount>,
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

    Ok(Diagnostics {
        version: app.package_info().version.to_string(),
        data_dir: data_dir(&app).to_string_lossy().into_owned(),
        // Asked live rather than cached: Everything is a separate program the
        // user can quit at any moment, so a remembered answer goes stale.
        everything_running: everything_ipc::available(),
        indexed_commands: guard.commands.len(),
        launched_entries: guard.frecency.len(),
        extensions: extension_summary(&guard.commands),
        by_source: source_summary(&guard.commands),
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
