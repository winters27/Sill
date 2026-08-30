pub mod action;
pub mod actions;
pub mod apps;
pub mod bindings;
pub mod calculator;
pub mod catalog;
pub mod clipboard;
pub mod commands;
pub mod dictation;
pub mod emoji;
pub mod everything_ipc;
pub mod exthost;
pub mod files;
pub mod host;
pub mod host_bridge;
pub mod icons;
pub mod input;
pub mod lnk;
pub mod log;
pub mod navigation;
pub mod object;
pub mod preferences;
pub mod quicklinks;
pub mod registry;
pub mod secrets;
pub mod selection;
pub mod settings_catalog;
pub mod settings_index;
pub mod snippets;
pub mod state;
pub mod summon;
pub mod synthetic;
pub mod text;
pub mod windowing;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use registry::{CommandRecord, Frecency};

use commands::settings::open_settings;
use host::{forward_events, host_js, index_paths};
use state::{now_seconds, HostState, PrefsState, Registry, RegistryState};

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
    let (sources, aliases) = {
        let prefs = handle.state::<PrefsState>();
        let prefs = prefs.inner.blocking_lock();
        (
            prefs.sources.clone(),
            registry::Aliases::new(&prefs.aliases),
        )
    };
    let index_paths = index_paths(&handle);

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
            registry.aliases = aliases.clone();
            drop(registry);
            let _ = handle.emit("sill://registry-updated", 0);
        }

        // The scan then rebuilds the index from scratch and replaces it
        // wholesale. Merging into the cache instead would mean an uninstalled
        // application never disappeared.
        let fresh = tokio::task::spawn_blocking(move || scan_everything(&sources, &index_paths))
            .await
            .unwrap_or_default();

        if fresh.is_empty() {
            return;
        }

        let mut registry = state.inner.lock().await;
        registry.commands = fresh;

        // Set even when the cache was empty, which the block above skipped.
        registry.frecency_path = frecency_path;
        registry.aliases = aliases;

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
pub(crate) fn scan_everything(
    sources: &preferences::Sources,
    index_paths: &[PathBuf],
) -> Vec<registry::CommandRecord> {
    // Sill's own commands are never optional; they are how the launcher is
    // configured and repaired.
    let mut out = registry::builtins();
    // Every index that exists, deduplicated by id: an extension installed
    // under the data directory and the same one built from the repository are
    // the same command, and the installed copy is the one that wins.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in index_paths {
        for command in registry::load_index(path) {
            if seen.insert(command.id.clone()) {
                out.push(command);
            }
        }
    }

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
    let registry_sources = sources.packaged_apps || sources.app_paths || sources.installed_programs;
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
pub(crate) fn rebind_summon(app: &AppHandle, previous: &str, next: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if let Err(err) = app.global_shortcut().unregister(previous) {
        crate::say!("could not release {previous}: {err}");
    }

    register_summon_shortcut(app, next);
}

/// Opens the launcher straight into the window switcher.
///
/// An empty accelerator means the feature is off, which is not the same as a
/// bad one: registering "" fails and would log an error every launch.
fn register_switcher_shortcut(app: &AppHandle, accelerator: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if accelerator.trim().is_empty() {
        return;
    }

    let handle = app.clone();
    let result = app
        .global_shortcut()
        .on_shortcut(accelerator, move |_, _, event| {
            if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                summon::show_switcher(&handle);
            }
        });

    if let Some(conflicts) = app.try_state::<HotkeyConflicts>() {
        conflicts.note(accelerator, result.is_ok());
    }

    match result {
        Ok(()) => println!("[sill] switcher key registered: {accelerator}"),
        Err(err) => crate::say!("could not register {accelerator}: {err}"),
    }
}

pub(crate) fn rebind_switcher(app: &AppHandle, previous: &str, next: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if !previous.trim().is_empty() {
        if let Err(err) = app.global_shortcut().unregister(previous) {
            crate::say!("could not release {previous}: {err}");
        }
    }

    register_switcher_shortcut(app, next);
}

/// Registers or removes Sill from the user's startup entries.
///
/// Reads the current state first, so this is safe to call on every launch to
/// reconcile the preference with what is actually in the registry. Somebody
/// who removed the entry by hand should not find it silently back.
pub(crate) fn apply_autostart(app: &AppHandle, enabled: bool) {
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
pub(crate) fn apply_window_size(app: &AppHandle, appearance: &preferences::Appearance) {
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
pub(crate) fn same_dictation(
    a: &dictation::models::DictationSettings,
    b: &dictation::models::DictationSettings,
) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

/// Pushes dictation settings into the service and arms or removes the hook.
///
/// The hook fires on a thread with no route back to the frontend, so the
/// service has to hold its own copy of everything the trigger needs.
pub(crate) fn apply_dictation(app: &AppHandle, settings: &dictation::models::DictationSettings) {
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

    let (finish, cancel) = dictation::hotkey::end_keys(&settings.finish_key, &settings.cancel_key);
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
pub(crate) fn apply_tray(app: &AppHandle, enabled: bool) {
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
pub(crate) fn reload_snippets(app: &AppHandle) {
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
pub(crate) fn reload_quicklinks(app: &AppHandle) {
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
pub(crate) fn reload_index(app: &AppHandle) {
    let handle = app.clone();
    let state = app.state::<RegistryState>().inner().clone();
    let sources = app
        .state::<PrefsState>()
        .inner
        .blocking_lock()
        .sources
        .clone();
    let index_paths = index_paths(app);

    tauri::async_runtime::spawn(async move {
        let fresh = tokio::task::spawn_blocking(move || scan_everything(&sources, &index_paths))
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
/// Accelerators Sill asked for and did not get.
///
/// Registration fails when another application already owns the combination,
/// and Windows gives no way to find out which. **Until this existed the
/// failure was silent**: the settings window showed the key the user had
/// chosen, the key did nothing, and only a log line said why. A shortcut that
/// looks bound and is not is worse than one that is obviously off.
#[derive(Default)]
pub struct HotkeyConflicts {
    taken: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl HotkeyConflicts {
    fn note(&self, accelerator: &str, ok: bool) {
        let Ok(mut taken) = self.taken.lock() else {
            return;
        };

        if ok {
            taken.remove(accelerator);
        } else {
            taken.insert(accelerator.to_string());
        }
    }

    pub fn all(&self) -> Vec<String> {
        self.taken
            .lock()
            .map(|taken| taken.iter().cloned().collect())
            .unwrap_or_default()
    }
}

fn register_summon_shortcut(app: &AppHandle, accelerator: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let handle = app.clone();
    let result = app
        .global_shortcut()
        .on_shortcut(accelerator, move |_, _, event| {
            // Fires on both press and release; acting on both would toggle twice
            // and leave the window exactly as it was.
            if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                summon::toggle_main(&handle);
            }
        });

    if let Some(conflicts) = app.try_state::<HotkeyConflicts>() {
        conflicts.note(accelerator, result.is_ok());
    }

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

/// Hides the launcher, so whatever was in front of it comes back.
///
/// Used by the built-ins that act on another application: a dictation started
/// from the launcher would otherwise be pasted into the launcher.
pub(crate) fn dismiss_main(app: &AppHandle) {
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
        .manage(actions::builtins())
        .manage(RegistryState {
            inner: Arc::new(tokio::sync::Mutex::new(Registry {
                commands: Vec::new(),
                own_settings: settings_index::records(),
                snippets: Vec::new(),
                quicklinks: Vec::new(),
                frecency: Frecency::default(),
                frecency_path: PathBuf::new(),
                aliases: registry::Aliases::default(),
            })),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let (tx, rx) = mpsc::unbounded_channel();
            forward_events(handle.clone(), rx);

            // Preferences first: the hotkey and the backdrop both come from
            // them, so reading them later would mean applying a default and
            // then immediately replacing it.
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            // Before anything that might have something to report.
            log::open(&data_dir);

            // Before any shortcut is registered, so a refusal is recorded
            // rather than dropped.
            app.manage(HotkeyConflicts::default());

            // The host itself is not started here. Nothing has asked for an
            // extension yet, and starting Node on the chance that something
            // might is 38 MB resident for a session that usually never opens
            // one. `host_of` brings it up on the first launch that needs it.
            //
            // The API layer is built now and outlives every host process,
            // because `LocalStorage` is a file: an extension that saved a
            // token before an idle shutdown has to still have it afterwards.
            let storage = match exthost::Storage::open(&exthost::storage::path(&data_dir)) {
                Ok(storage) => storage,
                Err(err) => {
                    // Not fatal. An extension that cannot save is worse than
                    // one that can, and far better than a launcher that
                    // refuses to start because of it.
                    crate::say!("extension storage unavailable, falling back to memory: {err}");
                    exthost::Storage::memory().expect("an in-memory store always opens")
                }
            };

            app.manage(HostState {
                inner: Arc::new(tokio::sync::Mutex::new(None)),
                api: Arc::new(exthost::ApiLayer::new(
                    tx,
                    host_bridge::SillBridge::new(handle.clone()),
                    Arc::new(storage),
                )),
                host_js: Arc::new(host_js(&handle)),
                last_used: Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
            });

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
            register_switcher_shortcut(&handle, &prefs.hotkey.switcher);
            // Nothing was bound before, so everything in the list is new.
            bindings::apply(&handle, &[], &prefs.bindings);
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
                    secrets: prefs.clipboard.secrets,
                });
                if prefs.clipboard.enabled {
                    clipboard::monitor::watch(&handle, &history);
                }
                app.manage(history);
            }

            // After the manage calls above: this resolves the service out of
            // managed state, which panics if it is not there yet.
            apply_dictation(&handle, &prefs.dictation);

            // Started here rather than waited for. The walk is over a second
            // of work, and nothing needs it until somebody types: file search
            // answers from an empty index for that second and from a full one
            // afterwards, which is a better first run than a launcher that
            // will not open until it has read the disk.
            {
                let roots = prefs.files.indexed_roots();
                if !roots.is_empty() {
                    // Managed here rather than as a default, because it needs
                    // to know where this machine keeps application data.
                    app.manage(state::CatalogState {
                        cache: Arc::new(
                            Some(state::data_dir(&handle).join("file-index.bin")),
                        ),
                        ..Default::default()
                    });

                    let catalog = app.state::<state::CatalogState>();

                    // Last run's index first, so searching works immediately,
                    // then a fresh walk behind it.
                    catalog.warm(&roots);
                    catalog.rebuild(roots.clone());

                    if let Some(watcher) =
                        state::CatalogWatcher::start(catalog.inner().clone(), roots)
                    {
                        app.manage(watcher);
                    }
                }
            }

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
            commands::settings::get_preferences,
            commands::settings::set_preferences,
            commands::settings::open_settings,
            commands::system::quit_app,
            commands::search::search_commands,
            commands::search::search_files,
            commands::search::search_windows,
            commands::search::search_emoji,
            commands::search::file_search_missing,
            commands::search::list_drives,
            commands::search::index_folder,
            commands::search::start_file_search,
            commands::settings::hotkey_conflicts,
            commands::settings::set_alias,
            commands::settings::index_rows,
            commands::settings::set_command_hotkey,
            commands::settings::set_hidden,
            commands::settings::navigation_chords,
            commands::settings::navigation_keys,
            commands::settings::emoji_tones,
            commands::search::list_monitors,
            commands::search::open_path,
            commands::launch::launch_command,
            commands::launch::record_use,
            commands::launch::query_history,
            commands::launch::actions_for,
            commands::launch::run_action,
            commands::launch::undo_action,
            commands::extensions::load_extension,
            commands::extensions::activate_handler,
            commands::extensions::unload_extension,
            commands::launch::perform_builtin,
            commands::system::app_icon,
            commands::diagnostics::diagnostics,
            commands::system::rebuild_index,
            commands::system::open_data_folder,
            commands::system::open_log,
            commands::system::clear_usage_history,
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
            clipboard::commands::clipboard_keep_current,
            clipboard::commands::clipboard_last_skipped,
            clipboard::commands::clipboard_merge,
            clipboard::commands::clipboard_collections,
            clipboard::commands::clipboard_create_collection,
            clipboard::commands::clipboard_rename_collection,
            clipboard::commands::clipboard_delete_collection,
            clipboard::commands::clipboard_add_to_collection,
            clipboard::commands::clipboard_remove_from_collection,
            clipboard::commands::clipboard_collection_entries,
            commands::settings::list_own_settings,
            snippets::commands::list_snippets,
            snippets::commands::save_snippet,
            snippets::commands::delete_snippet,
            snippets::commands::expand_snippet,
            snippets::commands::type_snippet,
            commands::system::dismiss
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
