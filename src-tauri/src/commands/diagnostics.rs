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
    pub(crate) version: String,
    pub(crate) data_dir: String,
    /// Everything responds to an IPC query, so file search works.
    pub(crate) everything_running: bool,
    pub(crate) indexed_commands: usize,
    /// Distinct entries that have ever been launched.
    pub(crate) launched_entries: usize,
    /// One per installed extension, with how many commands it contributes.
    pub(crate) extensions: Vec<ExtensionInfo>,
    /// Whether the machine has the interpreter extensions run in.
    ///
    /// Reported rather than discovered on first use. Extensions are Node
    /// programs, which is a requirement nothing in the application had ever
    /// mentioned: the first sign of it was a spawn failing with "the system
    /// cannot find the file specified", naming a file the person reading it
    /// had never heard of.
    pub(crate) node_installed: bool,
    /// How many entries each source contributed, for the Sources panel.
    pub(crate) by_source: Vec<SourceCount>,
    /// Whether Sill believes the keyboard hook is installed.
    ///
    /// Believing is the operative word, and why the count beside it exists.
    pub(crate) keyboard_hook_installed: bool,
    /// Keystrokes that hook has actually been called for.
    ///
    /// **Installed with this stuck at zero is the signature of a hook Windows
    /// removed**, which it does silently to any low-level hook whose callback
    /// runs long, and which leaves snippet expansion, the hyper key and
    /// double-tap all dead at once with nothing to look at. The dictation hook
    /// reports the same pair for the same reason.
    pub(crate) keyboard_keys_seen: u64,
}

#[derive(serde::Serialize)]
pub(crate) struct ExtensionInfo {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) commands: usize,
}

#[derive(serde::Serialize)]
pub(crate) struct SourceCount {
    pub(crate) mode: String,
    pub(crate) count: usize,
}

#[tauri::command]
pub(crate) async fn diagnostics(
    app: AppHandle,
    registry: State<'_, RegistryState>,
) -> Result<Diagnostics, String> {
    Ok(gather(&app, &registry))
}

/// The same reading, for whoever wants it without being a command.
///
/// Split out so the export bundle and the settings panel report one set of
/// facts. Two gatherers would drift, and a bundle that disagreed with the
/// screen the person is looking at is worse than no bundle.
fn gather(app: &AppHandle, registry: &RegistryState) -> Diagnostics {
    let index = registry.index();
    let ranking = registry.ranking();

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

    Diagnostics {
        version: app.package_info().version.to_string(),
        data_dir: data_dir(app).to_string_lossy().into_owned(),
        // Asked live rather than cached: Everything is a separate program the
        // user can quit at any moment, so a remembered answer goes stale.
        everything_running: everything_ipc::available(),
        indexed_commands: index.commands.len(),
        launched_entries: ranking.frecency.len(),
        // Asked live for the same reason Everything is: somebody can install
        // Node while Sill is open, and the panel showing this is exactly where
        // they would look afterwards.
        node_installed: crate::host::node_exe(
            &app.state::<crate::state::HostState>().node,
            crate::host::bundled_node(app),
        )
        .is_some(),
        extensions: extension_summary(&index.commands),
        by_source: source_summary(&index.commands),
        keyboard_hook_installed: hook.0,
        keyboard_keys_seen: hook.1,
    }
}

/**
Writes everything Sill knows about itself into one file that can be sent.

The transport half of [`crate::bundle`], which owns the shape and, more to the
point, owns what is left out. Everything here is gathering: what the settings
panels already show, what `timing` measured, what the status surface is
reporting, the log, and the crash file if there is one. The command hands those
to `assemble` and writes the answer.

Written into the data folder rather than offered as a save dialog, because the
folder already has an "Open folder" button beside it and because a person who
has just been asked for diagnostics should be able to read the file before
deciding to send it. Opening it is the last thing this does, for the same
reason.
*/
#[tauri::command]
pub(crate) async fn export_diagnostics(
    app: AppHandle,
    registry: State<'_, RegistryState>,
    timings: State<'_, crate::timing::Timings>,
) -> Result<String, String> {
    let facts = gather(&app, &registry);
    let report = timings.report();
    let dir = data_dir(&app);

    let troubles = app
        .try_state::<crate::status::Status>()
        .map(|status| status.all())
        .unwrap_or_default();

    // Read rather than remembered. The log is the file this is about and it is
    // still being appended to; anything cached would be the wrong end of it.
    let log = crate::log::path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();

    let crash = crate::log::crash_path().and_then(|path| std::fs::read_to_string(path).ok());

    let scrub = crate::bundle::Scrub::new(crate::reach::home().as_deref());
    let budgets = crate::bundle::budgets(crate::bundle::private_bytes(), &report);

    let by_source: Vec<(String, usize)> = facts
        .by_source
        .iter()
        .map(|source| (source.mode.clone(), source.count))
        .collect();

    let extensions: Vec<(String, String, usize)> = facts
        .extensions
        .iter()
        .map(|one| (one.id.clone(), one.title.clone(), one.commands))
        .collect();

    let text = crate::bundle::assemble(&crate::bundle::Parts {
        version: &facts.version,
        when: &crate::bundle::when(),
        level: crate::log::level(),
        facts: &readings(&facts),
        budgets: &budgets,
        by_source: &by_source,
        extensions: &extensions,
        timings: &report,
        troubles: &troubles,
        crash: crash.as_deref(),
        log: &log,
        scrub: &scrub,
    });

    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = dir.join(format!("sill-diagnostics-{}.txt", crate::bundle::filed()));
    std::fs::write(&path, text).map_err(|e| e.to_string())?;

    let shown = path.to_string_lossy().into_owned();
    crate::say!("wrote a diagnostic bundle");

    // Opened so it can be read before it is sent. A failure to open is not a
    // failure to export: the file is written and its path is on screen.
    if let Ok(target) = crate::reach::target(&shown) {
        let _ = tauri_plugin_opener::open_path(target, None::<&str>);
    }

    Ok(shown)
}

/// The plain facts, in the order somebody reads them.
///
/// Named here rather than in `bundle` because these are this application's
/// facts and that module is about the shape and the scrubbing. The data folder
/// is included and is scrubbed on the way out, since the path is the one thing
/// in this list that says whose machine it is.
fn readings(facts: &Diagnostics) -> Vec<(&'static str, String)> {
    vec![
        (
            "Processors",
            std::thread::available_parallelism()
                .map(|count| count.to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
        ),
        ("Data folder", facts.data_dir.clone()),
        (
            "File search (Everything)",
            yes_no(facts.everything_running, "running", "not running"),
        ),
        (
            "Extensions (Node)",
            yes_no(facts.node_installed, "installed", "not installed"),
        ),
        (
            "Keyboard hook",
            format!(
                "{}, {} keys seen",
                yes_no(facts.keyboard_hook_installed, "installed", "not installed"),
                facts.keyboard_keys_seen,
            ),
        ),
        ("Entries indexed", facts.indexed_commands.to_string()),
        ("Entries ever launched", facts.launched_entries.to_string()),
    ]
}

fn yes_no(state: bool, yes: &str, no: &str) -> String {
    if state { yes } else { no }.to_string()
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
