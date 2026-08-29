pub mod apps;
pub mod icons;
pub mod log;
pub mod lnk;
pub mod calculator;
pub mod clipboard;
pub mod dictation;
pub mod exthost;
pub mod everything_ipc;
pub mod files;
pub mod preferences;
pub mod registry;
pub mod settings_catalog;
pub mod settings_index;
pub mod quicklinks;
pub mod synthetic;
pub mod snippets;
pub mod summon;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

use exthost::{ExtHost, LoadOptions, UiEvent};
use registry::{CommandRecord, Frecency};

/// Holds the running extension host.
///
/// Startup is deferred to `setup` so a failure to spawn Node surfaces as a
/// visible error rather than a panic during builder construction.
///
/// The slot is an `Arc` so it can be cloned out of Tauri's state and moved
/// into an async task. Holding a `State<'_, _>` across an await would borrow
/// the app handle for the life of the task.
#[derive(Clone)]
struct HostState(Arc<tokio::sync::Mutex<Option<Arc<ExtHost>>>>);

/// The user's own preferences.
#[derive(Clone)]
struct PrefsState {
    inner: Arc<tokio::sync::Mutex<preferences::Preferences>>,
    path: Arc<PathBuf>,
}

/// The installed command registry and its ranking state.
#[derive(Clone)]
struct RegistryState {
    inner: Arc<tokio::sync::Mutex<Registry>>,
}

struct Registry {
    commands: Vec<CommandRecord>,
    /// Sill's own settings, shaped as commands.
    ///
    /// Built once at startup: the catalogue is a `const` and cannot change
    /// while the app runs, so rebuilding it per keystroke would be pure cost.
    own_settings: Vec<CommandRecord>,
    /// Snippets, shaped as commands.
    ///
    /// Held here rather than read per query: the previous version parsed
    /// `snippets.json` off disk on every keystroke, which is a filesystem
    /// round trip per character typed. Refreshed whenever a snippet changes.
    snippets: Vec<CommandRecord>,
    /// Quicklinks, shaped as commands. Held for the same reason as snippets.
    quicklinks: Vec<CommandRecord>,
    frecency: Frecency,
    frecency_path: PathBuf,
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Where built extensions register themselves during development.
fn dev_index_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .join("extensions")
        .join("build")
        .join("index.json")
}

/// Where the bundled extension host lives during development.
///
/// Release builds will ship it as a resource; resolving that is M2 work, so
/// this deliberately fails loudly rather than guessing.
fn dev_host_js() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .join("host")
        .join("dist")
        .join("host.js")
}

/// How long a command will wait for the host to finish starting.
const HOST_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Resolves the host, waiting briefly if it is still starting.
///
/// Startup moved onto the async runtime, so the window can be up and issuing
/// commands before the child process exists. Failing immediately would turn a
/// normal race into a visible error on every cold start.
async fn host_of(state: &State<'_, HostState>) -> Result<Arc<ExtHost>, String> {
    let deadline = std::time::Instant::now() + HOST_READY_TIMEOUT;

    loop {
        if let Some(host) = state.0.lock().await.clone() {
            return Ok(host);
        }
        if std::time::Instant::now() >= deadline {
            return Err("extension host did not start; check the log for why".to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

/// The root list, or what matches a query.
#[tauri::command]
async fn search_commands(
    state: State<'_, RegistryState>,
    prefs: State<'_, PrefsState>,
    query: String,
) -> Result<Vec<registry::RankedCommand>, String> {
    let excluded = prefs.inner.lock().await.sources.excluded.clone();
    let registry = state.inner.lock().await;

    // Chained, not collected: both sides are borrowed and nothing is copied.
    let mut results = registry::search_excluding(
        registry
            .commands
            .iter()
            .chain(registry.snippets.iter())
            .chain(registry.quicklinks.iter())
            .chain(registry.own_settings.iter()),
        &query,
        &registry.frecency,
        now_seconds(),
        registry::SEARCH_LIMIT,
        &excluded,
    );

    // Above everything, because when a query IS a sum the answer is the only
    // thing wanted. `evaluate` returns nothing for the ninety-nine queries in
    // a hundred that are searches, so this costs those nothing.
    if let Some(answer) = calculator::evaluate(&query) {
        results.insert(0, registry::answer_record(&answer.text, &answer.input));
    }

    Ok(results)
}

#[tauri::command]
async fn get_preferences(
    state: State<'_, PrefsState>,
) -> Result<preferences::Preferences, String> {
    Ok(state.inner.lock().await.clone())
}

/// Saves preferences and applies whatever can change without a restart.
///
/// The hotkey and the backdrop take effect immediately; source and file search
/// changes are read on the next query or scan. Anything needing a restart says
/// so in the UI rather than pretending.
#[tauri::command]
async fn set_preferences(
    app: AppHandle,
    state: State<'_, PrefsState>,
    prefs: preferences::Preferences,
) -> Result<(), String> {
    let previous = {
        let mut current = state.inner.lock().await;
        let previous = current.clone();
        *current = prefs.clone();
        previous
    };

    prefs.save(&state.path).map_err(|e| e.to_string())?;

    if previous.appearance.visible_rows != prefs.appearance.visible_rows
        || previous.appearance.window_width != prefs.appearance.window_width
    {
        apply_window_size(&app, &prefs.appearance);
    }

    if previous.general.open_at_login != prefs.general.open_at_login {
        apply_autostart(&app, prefs.general.open_at_login);
    }

    if previous.general.show_in_tray != prefs.general.show_in_tray {
        apply_tray(&app, prefs.general.show_in_tray);
    }

    if let Some(expander) = app.try_state::<snippets::expander::Expander>() {
        expander.set_enabled(prefs.snippets.expand_keywords);
        // Watching starts on demand and never stops: the hook owns a thread
        // with a message pump, and standing that up and tearing it down as a
        // setting is toggled is far more machinery than declining to match.
        if prefs.snippets.expand_keywords {
            snippets::expander::watch(&app, &expander);
        }
    }

    if let Some(history) = app.try_state::<clipboard::monitor::Clipboard>() {
        history.set_rules(clipboard::monitor::Rules {
            enabled: prefs.clipboard.enabled,
            keep_images: prefs.clipboard.keep_images,
            ignored_apps: prefs.clipboard.ignored_apps.clone(),
        });
        // Watching starts on demand and never stops: the listener owns a
        // thread, and turning the setting off simply stops it recording.
        if prefs.clipboard.enabled {
            clipboard::monitor::watch(&app, &history);
        }
    }

    if !same_dictation(&previous.dictation, &prefs.dictation) {
        apply_dictation(&app, &prefs.dictation);
    }

    if previous.hotkey.summon != prefs.hotkey.summon {
        rebind_summon(&app, &previous.hotkey.summon, &prefs.hotkey.summon);
    }

    if previous.appearance.backdrop != prefs.appearance.backdrop
        || previous.appearance.tint_alpha != prefs.appearance.tint_alpha
    {
        if let Some(window) = app.get_webview_window("main") {
            summon::apply_backdrop(&window, prefs.appearance.backdrop, prefs.appearance.tint_alpha);
        }
    }

    // The launcher window re-reads whatever it renders from.
    let _ = app.emit("sill://preferences-changed", &prefs);
    Ok(())
}

/// Opens the settings window, creating it the first time.
///
/// A separate window rather than a view inside the launcher: settings are read
/// and edited slowly, while the launcher is built to disappear the moment it
/// loses focus.
#[tauri::command]
async fn open_settings(app: AppHandle, section: Option<String>) -> Result<(), String> {
    // A section is carried in the query so a deep link lands where it means
    // to. Without it "About" would open settings at whatever was last shown.
    let route = match section.as_deref() {
        Some(name) if !name.is_empty() => format!("settings?section={name}"),
        _ => "settings".to_string(),
    };

    if let Some(existing) = app.get_webview_window("settings") {
        let _ = existing.show();
        let _ = existing.set_focus();
        if let Some(name) = section {
            let _ = existing.emit("sill://settings-section", name);
        }
        return Ok(());
    }

    let window = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App(route.into()),
    )
    .title("Settings")
    // Room for a 244px sidebar plus a settings pane that does not wrap its own
    // descriptions. Anything narrower and the right pane reads as a column.
    .inner_size(1180.0, 800.0)
    .min_inner_size(940.0, 620.0)
    .resizable(true)
    // Frameless and transparent, so the page draws its own title bar and the
    // same glass the launcher uses. A default title bar next to a glass body
    // looks like two different applications.
    .decorations(false)
    .transparent(true)
    .center()
    .build()
    .map_err(|e| e.to_string())?;

    let appearance = {
        let prefs = app.state::<PrefsState>();
        let guard = prefs.inner.lock().await;
        (guard.appearance.backdrop, guard.appearance.tint_alpha)
    };

    summon::apply_backdrop(&window, appearance.0, appearance.1);

    Ok(())
}

/// What the About and Advanced panels report.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
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
struct ExtensionInfo {
    id: String,
    title: String,
    commands: usize,
}

#[derive(serde::Serialize)]
struct SourceCount {
    mode: String,
    count: usize,
}

#[tauri::command]
async fn diagnostics(
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
fn extension_summary(commands: &[CommandRecord]) -> Vec<ExtensionInfo> {
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
fn source_summary(commands: &[CommandRecord]) -> Vec<SourceCount> {
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

/// Sill's own settings, for the settings window's filter box.
///
/// Read from the same catalogue the launcher searches, so the two can never
/// disagree about what exists or which panel it is in.
#[tauri::command]
fn list_own_settings() -> Vec<settings_index::Setting> {
    settings_index::SETTINGS.to_vec()
}

/// Rescans every enabled source.
///
/// Returns as soon as the scan is queued rather than waiting for it: the
/// launcher keeps answering from the old index and re-queries when
/// `sill://registry-updated` lands.
#[tauri::command]
fn rebuild_index(app: AppHandle) {
    reload_index(&app);
}

/// Opens the log in whatever reads a text file.
#[tauri::command]
fn open_log() -> Result<(), String> {
    let path = log::path().ok_or_else(|| "The log has not been opened".to_string())?;
    tauri_plugin_opener::open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Reveals the folder holding preferences, the index cache and the log.
#[tauri::command]
fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let dir = data_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    tauri_plugin_opener::open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Forgets which entries have been launched, so ranking starts over.
#[tauri::command]
async fn clear_usage_history(registry: State<'_, RegistryState>) -> Result<(), String> {
    let mut guard = registry.inner.lock().await;
    guard.frecency = Frecency::default();
    let path = guard.frecency_path.clone();
    guard.frecency.save(&path).map_err(|e| e.to_string())
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Files matching a query, from Everything.
///
/// Separate from `search_commands` rather than merged into it: this spawns a
/// process, so the UI debounces it and lets command results appear first.
#[tauri::command]
async fn search_files(
    state: State<'_, PrefsState>,
    query: String,
) -> Result<Vec<files::FileHit>, String> {
    let settings = state.inner.lock().await.files.clone();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    let query = files::scope(&query, &settings.only_in);

    let hits = tokio::task::spawn_blocking(move || {
        files::search_with(
            &query,
            settings.max_results as usize,
            settings.match_path,
            settings.match_case,
            settings.regex,
        )
    })
    .await
    .unwrap_or_default();

    Ok(hits)
}

/// Opens a file or folder in its default application.
#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

/// Runs a command from the root list.
///
/// Frecency is recorded before the load rather than after, so a command that
/// crashes on startup still counts as chosen. The user picked it; that is the
/// signal being learned, not whether it worked.
#[tauri::command]
async fn launch_command(
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
        dismiss_main(&app);

        // The same settle every paste in Sill needs: writing and immediately
        // pasting races the target application's read of the clipboard.
        std::thread::sleep(std::time::Duration::from_millis(60));
        dictation::paste::chord();

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
    let opts = LoadOptions::for_command(
        record.entrypoint.clone(),
        &record.extension,
        &record.command,
        mode,
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
struct LaunchedCommand {
    session: String,
    title: String,
    extension_title: String,
    /// "view" or "no-view"; the UI stays at the root list for no-view.
    mode: String,
}

#[tauri::command]
async fn load_extension(
    state: State<'_, HostState>,
    entrypoint: String,
    extension: String,
    command: String,
) -> Result<String, String> {
    let host = host_of(&state).await?;
    let opts = LoadOptions::view(entrypoint, &extension, &command);
    host.load(&opts).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn activate_handler(
    state: State<'_, HostState>,
    session: String,
    handler: String,
    args: Option<Value>,
) -> Result<Value, String> {
    let host = host_of(&state).await?;
    host.activate_handler(&session, &handler, args.unwrap_or(Value::Array(vec![])))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn unload_extension(state: State<'_, HostState>, session: String) -> Result<bool, String> {
    let host = host_of(&state).await?;
    host.unload(&session).await.map_err(|e| e.to_string())
}

/// Forwards everything the extension asks for to the window.
///
/// One channel to one event name keeps ordering intact: a toast raised during
/// a render must not overtake the render that caused it.
fn forward_events(app: AppHandle, mut events: mpsc::UnboundedReceiver<UiEvent>) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let Err(err) = app.emit("sill://ui", &event) {
                crate::say!("could not forward a UI event: {err}");
            }
        }
    });
}

/// Reads the installed command index and the saved ranking history.
///
/// A missing index is normal on a fresh checkout: nothing has been built yet,
/// so the root list is simply empty rather than an error.
fn load_registry(app: &tauri::App, handle: &AppHandle) {
    let handle = handle.clone();

    let data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let frecency_path = registry::frecency_path(&data_dir);
    let frecency = Frecency::load(&frecency_path);
    let cache_path = registry::cache_path(&data_dir);
    let cached = registry::load_cache(&cache_path);

    let state = handle.state::<RegistryState>().inner().clone();
    let sources = handle.state::<PrefsState>().inner.blocking_lock().sources.clone();

    tauri::async_runtime::spawn(async move {
        // Last run's index, shown immediately. Discovery costs a PowerShell
        // round trip and thousands of filesystem calls, and this is what keeps
        // a cold start from spending a second half-populated.
        if !cached.is_empty() {
            let mut registry = state.inner.lock().await;
            println!("[sill] {} entries from cache", cached.len());
            registry.commands = cached;
            registry.frecency = frecency;
            registry.frecency_path = frecency_path.clone();
            drop(registry);
            let _ = handle.emit("sill://registry-updated", 0);
        }

        // The scan then rebuilds the index from scratch and replaces it
        // wholesale. Merging into the cache instead would mean an uninstalled
        // application never disappeared.
        let fresh = tokio::task::spawn_blocking(move || scan_everything(&sources))
            .await
            .unwrap_or_default();

        if fresh.is_empty() {
            return;
        }

        let mut registry = state.inner.lock().await;
        registry.commands = fresh;

        // Set even when the cache was empty, which the block above skipped.
        registry.frecency_path = frecency_path;

        if let Err(err) = registry::save_cache(&cache_path, &registry.commands) {
            // A missing cache only costs a slower next start.
            crate::say!("could not write the index cache: {err}");
        }

        let total = registry.commands.len();
        drop(registry);

        println!("[sill] indexed {total} entries");

        /*
         * The window has already drawn its list by now.
         *
         * Discovery finishes a second or so after the UI first asked for
         * results and nothing re-asks on its own, so without this the user
         * keeps looking at the older list until they happen to type.
         */
        if let Err(err) = handle.emit("sill://registry-updated", total) {
            crate::say!("could not announce the updated registry: {err}");
        }
    });
}

/// Builds the whole index: extensions, settings, applications, executables.
///
/// Blocking on purpose. It is a PowerShell round trip plus a few thousand
/// filesystem calls, so it runs on a blocking task rather than holding an
/// async worker.
fn scan_everything(sources: &preferences::Sources) -> Vec<registry::CommandRecord> {
    // Sill's own commands are never optional; they are how the launcher is
    // configured and repaired.
    let mut out = registry::builtins();
    out.extend(registry::load_index(&dev_index_path()));

    // Settings pages are not files, so no scan finds them.
    if sources.windows_settings {
        out.extend(settings_catalog::load());
    }

    let shortcuts = if sources.shortcuts {
        apps::scan_shortcuts()
    } else {
        Vec::new()
    };

    // One PowerShell round trip covers three registry sources, so it is only
    // skipped when the user has turned all of them off.
    let registry_sources =
        sources.packaged_apps || sources.app_paths || sources.installed_programs;
    let packaged = if registry_sources {
        apps::scan_apps_folder()
    } else {
        Vec::new()
    };

    let on_path = if sources.path_executables {
        apps::scan_path_executables()
    } else {
        Vec::new()
    };

    let mut names: std::collections::HashSet<String> =
        out.iter().map(|c| c.title.to_lowercase()).collect();
    let mut targets: std::collections::HashSet<String> = std::collections::HashSet::new();

    let keep = |record: &apps::AppRecord,
                    names: &mut std::collections::HashSet<String>,
                    targets: &mut std::collections::HashSet<String>| {
        if !names.insert(record.name.to_lowercase()) {
            return false;
        }
        // Matched on the executable as well as the name: the Start Menu says
        // "Google Chrome" and App Paths says "chrome", and both run the same
        // binary. Only a target comparison collapses that.
        match apps::target_key(record) {
            Some(target) => targets.insert(target),
            None => true,
        }
    };

    for app in &shortcuts {
        if keep(app, &mut names, &mut targets) {
            out.push(registry::app_record(
                &app.name,
                &app.path,
                app.icon_source.clone(),
                apps::categorize(app),
            ));
        }
    }

    for app in &packaged {
        if keep(app, &mut names, &mut targets) {
            out.push(registry::app_record(
                &app.name,
                &app.path,
                app.icon_source.clone(),
                apps::categorize(app),
            ));
        }
    }

    for app in &on_path {
        if keep(app, &mut names, &mut targets) {
            // A PATH entry is still categorised by where it resolves, so a
            // System32 tool reads as "System". "Command Line" is reserved for
            // things that are applications only by virtue of being on PATH.
            let kind = match apps::categorize(app) {
                "Application" => "Command Line",
                other => other,
            };
            out.push(registry::executable_record(&app.name, &app.path, kind));
        }
    }

    println!(
        "[sill] {} shortcuts, {} packaged, {} on PATH",
        shortcuts.len(),
        packaged.len(),
        on_path.len()
    );

    out
}

/// Swaps one summon key for another.
///
/// The old binding is released first: leaving it registered means the previous
/// combination keeps summoning the launcher, which looks like the setting was
/// ignored.
fn rebind_summon(app: &AppHandle, previous: &str, next: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if let Err(err) = app.global_shortcut().unregister(previous) {
        crate::say!("could not release {previous}: {err}");
    }

    register_summon_shortcut(app, next);
}

/// Registers or removes Sill from the user's startup entries.
///
/// Reads the current state first, so this is safe to call on every launch to
/// reconcile the preference with what is actually in the registry. Somebody
/// who removed the entry by hand should not find it silently back.
fn apply_autostart(app: &AppHandle, enabled: bool) {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();

    match manager.is_enabled() {
        Ok(current) if current == enabled => return,
        Err(err) => crate::say!("could not read the startup entry: {err}"),
        _ => {}
    }

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    if let Err(err) = result {
        crate::say!("could not change the startup entry: {err}");
    }
}

/// Sizes the launcher from the appearance preference.
///
/// Re-centres afterwards. Growing a centred window from its top left corner
/// walks it down and right across the screen every time the row count changes.
fn apply_window_size(app: &AppHandle, appearance: &preferences::Appearance) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let size = tauri::LogicalSize::new(
        f64::from(appearance.window_width.clamp(560, 1100)),
        appearance.window_height(),
    );

    if let Err(err) = window.set_size(size) {
        crate::say!("could not resize the launcher: {err}");
        return;
    }

    let _ = window.center();
}

/// Whether two dictation settings differ in a way the hook cares about.
///
/// Compared by serialisation rather than field by field: every field reaches
/// the service, so any change at all has to be pushed down, and a hand-written
/// comparison is one more place to forget a new field.
fn same_dictation(
    a: &dictation::models::DictationSettings,
    b: &dictation::models::DictationSettings,
) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

/// Pushes dictation settings into the service and arms or removes the hook.
///
/// The hook fires on a thread with no route back to the frontend, so the
/// service has to hold its own copy of everything the trigger needs.
fn apply_dictation(app: &AppHandle, settings: &dictation::models::DictationSettings) {
    let service = app.state::<dictation::service::DictationService>();
    service.set_settings(settings.clone());

    if !settings.enabled {
        service.disable_hotkey();
        return;
    }

    let mut chord = match dictation::hotkey::chord_from_shortcut(
        &settings.shortcut_modifier,
        &settings.shortcut_key,
    ) {
        Ok(chord) => chord,
        Err(err) => {
            crate::say!("dictation shortcut is unusable: {err}");
            return;
        }
    };

    let (finish, cancel) =
        dictation::hotkey::end_keys(&settings.finish_key, &settings.cancel_key);
    chord.finish = finish;
    chord.cancel = cancel;

    match service.enable_hotkey(app, chord) {
        // Logged on success as well as failure. Only logging failures meant
        // silence could mean either "armed fine" or "never ran", which is
        // the one thing a log has to be able to tell apart.
        Ok(()) => crate::say!(
            "dictation hook armed for {}+{} (finish {}, cancel {})",
            settings.shortcut_modifier,
            settings.shortcut_key,
            settings.finish_key,
            settings.cancel_key
        ),
        Err(err) => crate::say!("could not arm the dictation hook: {err}"),
    }
}

const TRAY_ID: &str = "sill-tray";

/// Shows or hides the notification area icon.
///
/// The tray is the only visible sign that a launcher is running: it has no
/// taskbar button by design, so without it there is nothing to click and no
/// way to tell it apart from not running at all.
fn apply_tray(app: &AppHandle, enabled: bool) {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    if !enabled {
        app.remove_tray_by_id(TRAY_ID);
        return;
    }

    if app.tray_by_id(TRAY_ID).is_some() {
        return;
    }

    let Some(icon) = app.default_window_icon().cloned() else {
        crate::say!("no bundled icon, so there is nothing to put in the tray");
        return;
    };

    let build = || -> tauri::Result<()> {
        let show = MenuItem::with_id(app, "tray-show", "Open Sill", true, None::<&str>)?;
        let settings = MenuItem::with_id(app, "tray-settings", "Settings", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "tray-quit", "Quit Sill", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &settings, &quit])?;

        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .tooltip("Sill")
            .menu(&menu)
            // The menu belongs to the right button. A left click summons,
            // which is what every launcher tray icon does.
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "tray-show" => summon::toggle_main(app),
                "tray-settings" => {
                    let handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(err) = open_settings(handle, None).await {
                            crate::say!("could not open settings: {err}");
                        }
                    });
                }
                "tray-quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                // A click reports both the press and the release. Acting on
                // both would summon and immediately dismiss again.
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    summon::toggle_main(tray.app_handle());
                }
            })
            .build(app)?;

        Ok(())
    };

    if let Err(err) = build() {
        crate::say!("could not create the tray icon: {err}");
    }
}

/// Reloads snippets into both the search index and the keyboard hook.
///
/// One function because they must never disagree: a snippet the launcher can
/// find but the hook cannot expand, or the reverse, is worse than neither.
fn reload_snippets(app: &AppHandle) {
    let loaded = snippets::store::load(&snippets::store::path(app));

    if let Some(expander) = app.try_state::<snippets::expander::Expander>() {
        expander.set_snippets(loaded.clone());
    }

    let records: Vec<CommandRecord> = loaded
        .iter()
        .map(|snippet| {
            registry::snippet_record(
                &snippet.id,
                &snippet.name,
                &snippet.keyword,
                &snippet.content,
            )
        })
        .collect();

    if let Some(state) = app.try_state::<RegistryState>() {
        let registry = state.inner.clone();
        tauri::async_runtime::spawn(async move {
            registry.lock().await.snippets = records;
        });
    }
}

/// Reloads quicklinks into the search index.
fn reload_quicklinks(app: &AppHandle) {
    let dir = quicklinks::store::data_dir(app);
    let loaded = quicklinks::store::load(&quicklinks::store::path(&dir));

    let records: Vec<CommandRecord> = loaded
        .iter()
        .map(|link| {
            registry::quicklink_record(
                &link.id,
                &link.name,
                &link.keyword,
                &link.link,
                link.needs_argument(),
            )
        })
        .collect();

    if let Some(state) = app.try_state::<RegistryState>() {
        let registry = state.inner.clone();
        tauri::async_runtime::spawn(async move {
            registry.lock().await.quicklinks = records;
        });
    }
}

/// Rebuilds the index in the background.
fn reload_index(app: &AppHandle) {
    let handle = app.clone();
    let state = app.state::<RegistryState>().inner().clone();
    let sources = app.state::<PrefsState>().inner.blocking_lock().sources.clone();

    tauri::async_runtime::spawn(async move {
        let fresh = tokio::task::spawn_blocking(move || scan_everything(&sources))
            .await
            .unwrap_or_default();

        if fresh.is_empty() {
            return;
        }

        let total = fresh.len();
        state.inner.lock().await.commands = fresh;
        println!("[sill] reindexed {total} entries");
        let _ = handle.emit("sill://registry-updated", total);
    });
}

/// Binds the summon key.
///
/// A failure here is reported rather than fatal: another app may already hold
/// the combination, and a launcher that refuses to start because its hotkey is
/// taken is worse than one you have to click into.
fn register_summon_shortcut(app: &AppHandle, accelerator: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let handle = app.clone();
    let result = app.global_shortcut().on_shortcut(accelerator, move |_, _, event| {
        // Fires on both press and release; acting on both would toggle twice
        // and leave the window exactly as it was.
        if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            summon::toggle_main(&handle);
        }
    });

    match result {
        Ok(()) => println!("[sill] summon key registered: {accelerator}"),
        Err(err) => crate::say!("could not register {accelerator}: {err}"),
    }
}

/// Dismisses the launcher when it loses focus.
///
/// Clicking away is a dismissal, the same as Escape. Without this the window
/// would sit on top of whatever the user switched to, which is the single most
/// irritating thing an always-on-top launcher can do.
fn watch_focus(app: &AppHandle, dismiss_on_blur: bool) {
    // Opening devtools takes focus, which would dismiss the launcher the
    // instant you tried to inspect it. This makes debugging possible without
    // changing what a real user gets.
    if std::env::var_os("SILL_NO_AUTOHIDE").is_some() {
        println!("[sill] SILL_NO_AUTOHIDE set: the launcher will not dismiss on focus loss");
        return;
    }

    if !dismiss_on_blur {
        return;
    }

    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let handle = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            // Focus is already gone, so the previous window must not be
            // restored on top of whatever the user just clicked.
            let _ = handle.hide();
        }
    });
}

/// Performs an action Raycast implements itself rather than handing to the
/// extension.
///
/// `Action.CopyToClipboard` and friends carry no `onAction`; they declare what
/// they want done through their props and the launcher is expected to do it.
/// Treating them as broken because they have no callback would silently kill
/// the most common action in the whole ecosystem.
#[tauri::command]
async fn perform_builtin(
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

        // Paste needs synthetic keyboard input, which is the Win32 work in M5.
        // Copying is the honest half of it, and saying so beats pretending.
        "Action.Paste" => {
            let content = text_of(props.get("content"))
                .ok_or_else(|| "that action carried nothing to paste".to_string())?;
            app.clipboard()
                .write_text(content)
                .map_err(|e| e.to_string())?;
            Ok("Copied (paste injection is not built yet)".to_string())
        }

        other => Err(format!("{other} is not a built-in Sill can perform")),
    }
}

/// The icon for a launchable, as a data URI.
///
/// Requested lazily per row rather than resolved for the whole index: a
/// machine has hundreds of Start Menu entries and only a handful are ever on
/// screen. Results are cached, misses included.
#[tauri::command]
async fn app_icon(path: String) -> Option<String> {
    icons::icon_data_uri(&path)
}

/// Closes Sill entirely.
///
/// A launcher is normally dismissed rather than quit, so this is deliberately
/// only reachable from the menu: there is no accidental path to it.
#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Puts the launcher away. Bound to Escape in the UI.
#[tauri::command]
fn dismiss(window: tauri::WebviewWindow) {
    summon::hide(&window);
}

/// Hides the launcher, so whatever was in front of it comes back.
///
/// Used by the built-ins that act on another application: a dictation started
/// from the launcher would otherwise be pasted into the launcher.
fn dismiss_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        summon::hide(&window);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, so a second launch is turned away before it installs a
        // keyboard hook or opens the clipboard database. Two of either is
        // worse than none: the hooks both fire, the shortcut registration
        // loses a race, and the launcher looks broken rather than doubled.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Someone tried to start Sill again, which almost always means
            // they wanted the window they already have.
            summon::toggle_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(HostState(Arc::new(tokio::sync::Mutex::new(None))))
        .manage(RegistryState {
            inner: Arc::new(tokio::sync::Mutex::new(Registry {
                commands: Vec::new(),
                own_settings: settings_index::records(),
                snippets: Vec::new(),
                quicklinks: Vec::new(),
                frecency: Frecency::default(),
                frecency_path: PathBuf::new(),
            })),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let (tx, rx) = mpsc::unbounded_channel();
            forward_events(handle.clone(), rx);

            // Cloned out before the task so no Tauri borrow crosses an await.
            let slot = app.state::<HostState>().inner().clone();
            let host_js = dev_host_js();

            // Started on the async runtime: spawning the child registers it
            // with the Tokio reactor, and `setup` itself is not running in one.
            tauri::async_runtime::spawn(async move {
                if !host_js.exists() {
                    eprintln!(
                        "[sill] extension host bundle missing at {}. Run: npm --prefix host run build",
                        host_js.display()
                    );
                    return;
                }

                match ExtHost::spawn(&PathBuf::from("node"), &host_js, tx).await {
                    Ok(host) => *slot.0.lock().await = Some(Arc::new(host)),
                    Err(err) => crate::say!("could not start the extension host: {err}"),
                }
            });

            // Preferences first: the hotkey and the backdrop both come from
            // them, so reading them later would mean applying a default and
            // then immediately replacing it.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            // Before anything that might have something to report.
            log::open(&data_dir);

            let prefs_path = preferences::path(&data_dir);
            let prefs = preferences::Preferences::load(&prefs_path);

            if let Some(window) = app.get_webview_window("main") {
                summon::apply_backdrop(
                    &window,
                    prefs.appearance.backdrop,
                    prefs.appearance.tint_alpha,
                );
            }

            apply_window_size(&handle, &prefs.appearance);
            register_summon_shortcut(&handle, &prefs.hotkey.summon);
            apply_tray(&handle, prefs.general.show_in_tray);
            apply_autostart(&handle, prefs.general.open_at_login);
            watch_focus(&handle, prefs.hotkey.dismiss_on_blur);

            // Dictation holds a low-level keyboard hook and, when local, a
            // whisper server process. Both are managed state so they live
            // exactly as long as the app does.
            app.manage(dictation::service::DictationService::new());
            app.manage(dictation::server::WhisperServer::default());
            app.manage(dictation::panel::PanelState::default());

            // The hook reads its snippets from here, so they have to be
            // loaded before it is armed or the first keyword never fires.
            let expander = snippets::expander::Expander::new();
            expander.set_enabled(prefs.snippets.expand_keywords);
            if prefs.snippets.expand_keywords {
                snippets::expander::watch(&handle, &expander);
            }
            app.manage(expander);

            // Acted on in Rust rather than the frontend: a keyword is typed
            // into another application entirely, and the launcher's webview
            // is usually not running when it happens.
            {
                use tauri::Listener;
                let handle = handle.clone();
                app.listen("snippets:expand", move |event| {
                    let Ok((id, backspaces)) =
                        serde_json::from_str::<(String, usize)>(event.payload())
                    else {
                        return;
                    };

                    // Off the hook's thread: typing the replacement sends
                    // input, and a hook callback must not sit and wait.
                    let handle = handle.clone();
                    std::thread::spawn(move || {
                        let expander = handle.state::<snippets::expander::Expander>();
                        if let Err(err) = snippets::commands::type_snippet(
                            handle.clone(),
                            expander,
                            id,
                            backspaces,
                        ) {
                            crate::say!("could not expand a snippet: {err}");
                        }
                    });
                });
            }

            // The history is a SQLite file that has to exist before anything
            // can read it, and the watcher owns a thread of its own.
            if let Some(history) = clipboard::monitor::Clipboard::open(&handle) {
                if prefs.clipboard.retain_days > 0 {
                    match history
                        .store()
                        .prune(prefs.clipboard.retain_days, now_seconds())
                    {
                        Ok(0) => {}
                        Ok(gone) => crate::say!("pruned {gone} old clipboard entries"),
                        Err(err) => crate::say!("could not prune the clipboard: {err}"),
                    }
                }
                history.set_rules(clipboard::monitor::Rules {
                    enabled: prefs.clipboard.enabled,
                    keep_images: prefs.clipboard.keep_images,
                    ignored_apps: prefs.clipboard.ignored_apps.clone(),
                });
                if prefs.clipboard.enabled {
                    clipboard::monitor::watch(&handle, &history);
                }
                app.manage(history);
            }

            // After the manage calls above: this resolves the service out of
            // managed state, which panics if it is not there yet.
            apply_dictation(&handle, &prefs.dictation);

            app.manage(PrefsState {
                inner: Arc::new(tokio::sync::Mutex::new(prefs)),
                path: Arc::new(prefs_path),
            });

            load_registry(app, &handle);

            // After the registry and the expander are both managed, since it
            // fills in each of them.
            reload_snippets(&handle);
            reload_quicklinks(&handle);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quicklinks::commands::list_quicklinks,
            quicklinks::commands::save_quicklink,
            quicklinks::commands::delete_quicklink,
            quicklinks::commands::open_quicklink,
            get_preferences,
            set_preferences,
            open_settings,
            quit_app,
            search_commands,
            search_files,
            open_path,
            launch_command,
            load_extension,
            activate_handler,
            unload_extension,
            perform_builtin,
            app_icon,
            diagnostics,
            rebuild_index,
            open_data_folder,
            open_log,
            clear_usage_history,
            dictation::commands::list_audio_input_devices,
            dictation::commands::set_dictation_settings,
            dictation::commands::get_dictation_settings,
            dictation::commands::get_dictation_panel_status,
            dictation::commands::is_dictation_listening,
            dictation::commands::start_dictation,
            dictation::commands::confirm_dictation,
            dictation::commands::cancel_dictation,
            dictation::commands::list_whisper_models,
            dictation::commands::get_local_dictation_status,
            dictation::commands::install_local_dictation,
            dictation::commands::remove_whisper_model,
            dictation::commands::stop_whisper_server,
            dictation::commands::dictation_history,
            dictation::commands::dictation_stats,
            dictation::commands::last_transcription,
            dictation::commands::forget_transcription,
            dictation::commands::clear_dictation_history,
            dictation::commands::dictation_hook_state,
            dictation::commands::reset_dictation_hook,
            clipboard::commands::clipboard_search,
            clipboard::commands::clipboard_entry,
            clipboard::commands::clipboard_paste,
            clipboard::commands::clipboard_pin,
            clipboard::commands::clipboard_delete,
            clipboard::commands::clipboard_clear,
            clipboard::commands::clipboard_count,
            list_own_settings,
            snippets::commands::list_snippets,
            snippets::commands::save_snippet,
            snippets::commands::delete_snippet,
            snippets::commands::expand_snippet,
            snippets::commands::type_snippet,
            dismiss
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
