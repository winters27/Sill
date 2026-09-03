pub mod action;
pub mod actions;
pub mod activity;
pub mod ai;
pub mod app_volume;
pub mod apps;
pub mod apps_watch;
pub mod audio;
pub mod bindings;
pub mod browsers;
pub mod calculator;
pub mod capture;
pub mod catalog;
pub mod clipboard;
pub mod commands;
pub mod dictation;
pub mod emoji;
pub mod everything_ipc;
pub mod extension_install;
pub mod exthost;
pub mod files;
pub mod files_ops;
pub mod host;
pub mod host_bridge;
pub mod hyper;
pub mod icons;
pub mod input;
pub mod job;
pub mod json_store;
pub mod lazy_windows;
pub mod live;
pub mod lnk;
pub mod log;
pub mod meter;
pub mod navigation;
pub mod object;
pub mod ocr;
pub mod placement;
pub mod preferences;
pub mod previews;
pub mod processes;
pub mod profiles;
pub mod profiles_store;
pub mod quicklinks;
pub mod radios;
pub mod reach;
pub mod registry;
pub mod scripts;
pub mod secrets;
pub mod selection;
pub mod settings_catalog;
pub mod settings_index;
pub mod shell;
pub mod sleep;
pub mod snippets;
pub mod state;
pub mod status;
pub mod store;
pub mod summon;
pub mod synthetic;
pub mod system;
pub mod taps;
pub mod text;
pub mod timing;
pub mod tts;
pub mod utilities;
pub mod weather;
pub mod websearch;
pub mod windowing;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use registry::{CommandRecord, Frecency};

use host::{forward_events, host_js, index_paths};
use state::{now_seconds, HostState, PrefsState, RegistryState};

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
    let cache_path = registry::cache_path(&data_dir);

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
    let workspaces = profiles_store::path(&handle);

    tauri::async_runtime::spawn(async move {
        /*
         * Both files are read here rather than before the spawn.
         *
         * They were read on the main thread, which put them in front of the
         * "ready" stamp and therefore in front of the hotkey working. Neither
         * is needed for that: the index cache is what the first search reads
         * and the ranking is what a launch records into, and both of those
         * happen after somebody has already pressed the key.
         *
         * On a blocking thread, because they are disk reads and the async
         * runtime is not the place for those.
         */
        let read = {
            let frecency_path = frecency_path.clone();
            let cache_path = cache_path.clone();

            tokio::task::spawn_blocking(move || {
                (
                    Frecency::load(&frecency_path),
                    registry::load_cache(&cache_path),
                )
            })
            .await
        };

        let Ok((frecency, cached)) = read else {
            crate::say!("could not read last run's index");
            return;
        };

        /*
         * The ranking, before anything can be launched.
         *
         * Set outside the cache check on purpose. It used to be inside it, so
         * on a first run, or any run where the cache was missing, the ranking
         * history and the path to save it stayed empty until the scan
         * finished, and anything launched in that window was recorded into a
         * copy that the scan then replaced.
         */
        state.ranking.store(std::sync::Arc::new(state::Ranking {
            frecency,
            path: frecency_path,
        }));

        // Last run's index, shown immediately. Discovery costs a PowerShell
        // round trip and thousands of filesystem calls, and this is what keeps
        // a cold start from spending a second half-populated.
        if !cached.is_empty() {
            println!("[sill] {} entries from cache", cached.len());

            let aliases = aliases.clone();
            state.update_index(move |index| {
                index.commands = cached;
                index.aliases = aliases;
            });

            let _ = handle.emit("sill://registry-updated", 0);
        }

        // The scan then rebuilds the index from scratch and replaces it
        // wholesale. Merging into the cache instead would mean an uninstalled
        // application never disappeared.
        let fresh = tokio::task::spawn_blocking(move || {
            scan_everything(&sources, &index_paths, &workspaces)
        })
        .await
        .unwrap_or_default();

        if fresh.is_empty() {
            return;
        }

        let total = fresh.len();
        let text = registry::cache_text(&fresh);

        // Only what the scan produced. Snippets, quicklinks and scripts are
        // somebody else's to maintain and are left exactly as they are.
        state.update_index(move |index| {
            index.commands = fresh;
            index.aliases = aliases;
        });

        if let Some(text) = text {
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(err) = registry::write_cache(&cache_path, &text) {
                    // A missing cache only costs a slower next start.
                    crate::say!("could not write the index cache: {err}");
                }
            });
        }

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
/// The double-tap gesture as the hook wants it, or nothing when it is off.
///
/// One translation, used by startup and by the settings window, so the two
/// cannot disagree about what is bound.
pub(crate) fn tap_binding(taps: &preferences::Taps) -> Option<snippets::expander::TapBinding> {
    taps.modifier
        .map(|modifier| snippets::expander::TapBinding {
            modifier,
            // A window of nothing would mean the second tap has to arrive in the
            // same instant, which is a gesture nobody can make.
            window_ms: taps.window_ms.max(1),
        })
}

/// Blocking on purpose. It is a PowerShell round trip plus a few thousand
/// filesystem calls, so it runs on a blocking task rather than holding an
/// async worker.
pub(crate) fn scan_everything(
    sources: &preferences::Sources,
    index_paths: &[PathBuf],
    workspaces: &PathBuf,
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

    // Saved window arrangements, which are rows like anything else so they can
    // be searched, aliased and given a key of their own.
    out.extend(profiles_store::records(workspaces));

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

    registry::one_per_id(out)
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

/// Picks an area of the screen, without the launcher appearing first.
///
/// The whole value of a screenshot key is that it is one press, so this goes
/// straight to the overlay rather than opening the launcher on a command.
fn register_capture_shortcut(app: &AppHandle, accelerator: &str, whole_screen: bool) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if accelerator.trim().is_empty() {
        return;
    }

    let handle = app.clone();
    let result = app
        .global_shortcut()
        .on_shortcut(accelerator, move |_, _, event| {
            if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                return;
            }

            let app = handle.clone();
            tauri::async_runtime::spawn(async move {
                let done = if whole_screen {
                    commands::system::capture_screen(app.clone()).await
                } else {
                    commands::system::begin_capture(app.clone())
                        .await
                        .map(|()| String::new())
                };

                if let Err(reason) = done {
                    crate::say!("capture key: {reason}");
                }
            });
        });

    if let Some(conflicts) = app.try_state::<HotkeyConflicts>() {
        conflicts.note(accelerator, result.is_ok());
    }

    match result {
        Ok(()) => println!("[sill] capture key registered: {accelerator}"),
        Err(err) => crate::say!("could not register {accelerator}: {err}"),
    }
}

/// Swaps one capture key for another.
pub(crate) fn rebind_capture(app: &AppHandle, previous: &str, next: &str, whole_screen: bool) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    if !previous.trim().is_empty() {
        if let Err(err) = app.global_shortcut().unregister(previous) {
            crate::say!("could not release {previous}: {err}");
        }
    }

    register_capture_shortcut(app, next, whole_screen);
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
        Ok(current) if current == enabled => {
            status::resolved(app, AUTOSTART_TROUBLE);
            return;
        }
        // Logged and not reported. The write below is attempted anyway and its
        // result is the one that decides whether the setting took, so a failed
        // read on its own is nothing anybody can do anything about.
        Err(err) => crate::say!("could not read the startup entry: {err}"),
        _ => {}
    }

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    /*
     * Reported, because the toggle now says something untrue.
     *
     * Writing the `Run` entry is a registry write and it can be refused: by
     * policy on a managed machine, by security software, or by the key being
     * owned by another user. The switch stays where it was put, the settings
     * row reads "on", and the next time the machine starts Sill does not.
     * Nothing about that morning suggests a setting is the reason.
     */
    match result {
        Ok(()) => status::resolved(app, AUTOSTART_TROUBLE),
        Err(err) => status::report(
            app,
            AUTOSTART_TROUBLE,
            format!(
                "Sill could not change whether it opens at login, so it will not \
                 start with Windows: {err}"
            ),
            Some("general"),
        ),
    }
}

/// The one startup-entry trouble, named once so the row and the report agree.
const AUTOSTART_TROUBLE: &str = "autostart";

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

    // Centred by the same rule a summon uses, rather than by `center()`, which
    // always means the primary screen. Changing the row count while the
    // launcher is up on the second monitor used to throw it back to the first.
    match app.try_state::<placement::Placement>() {
        Some(placement) => placement::centre_for_summon(&window, placement.get()),
        None => {
            let _ = window.center();
        }
    }
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

pub(crate) const TRAY_ID: &str = "sill-tray";

/// The one tray trouble, named once so the row that switches it on and the
/// report that says it did not appear agree about which failure they mean.
const TRAY_TROUBLE: &str = "tray";

/// Shows or hides the notification area icon.
///
/// The tray is the only visible sign that a launcher is running: it has no
/// taskbar button by design, so without it there is nothing to click and no
/// way to tell it apart from not running at all.
pub(crate) fn apply_tray(app: &AppHandle, enabled: bool) {
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    if !enabled {
        app.remove_tray_by_id(TRAY_ID);
        // Turning it off is not a failure to create it, and leaving the report
        // standing would mark the row that had just been used correctly.
        status::resolved(app, TRAY_TROUBLE);
        return;
    }

    if app.tray_by_id(TRAY_ID).is_some() {
        status::resolved(app, TRAY_TROUBLE);
        return;
    }

    let Some(icon) = app.default_window_icon().cloned() else {
        status::report(
            app,
            TRAY_TROUBLE,
            "Sill has no bundled icon, so there is nothing to put in the notification area.",
            Some("general"),
        );
        return;
    };

    let build = || -> tauri::Result<()> {
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .tooltip("Sill")
            /*
             * No `tauri::menu::Menu`, deliberately.
             *
             * A native menu is drawn by the shell in the system font at the
             * system size, and takes none of Sill's design: no glass, no
             * keycaps, no glyphs. This is the one surface somebody meets
             * without opening the launcher, so it gets a real window instead.
             */
            .on_tray_icon_event(|tray, event| {
                // A click reports both the press and the release. Acting on
                // both would summon and immediately dismiss again.
                let TrayIconEvent::Click {
                    button,
                    button_state: MouseButtonState::Up,
                    position,
                    ..
                } = event
                else {
                    return;
                };

                match button {
                    MouseButton::Left => summon::toggle_main(tray.app_handle()),
                    MouseButton::Right => show_tray_menu(tray.app_handle(), position),
                    MouseButton::Middle => {}
                }
            })
            .build(app)?;

        Ok(())
    };

    /*
     * Reported, because the toggle is on and there is nothing there.
     *
     * This is the failure with the least to go on. Sill has no taskbar button
     * by design, so with no tray icon a running launcher and a launcher that
     * never started look identical, and the setting that was supposed to
     * produce one still reads as on. It cannot be reported in the tray, which
     * is the thing that failed, so the settings row is the whole of it.
     */
    match build() {
        Ok(()) => {
            status::resolved(app, TRAY_TROUBLE);
            // The icon was built with its own tooltip, so anything already
            // wrong has just been painted over. Turning the tray on must not
            // be what silences the rest of the surface.
            status::refresh(app);
        }
        Err(err) => status::report(
            app,
            TRAY_TROUBLE,
            format!("Sill could not create its notification area icon: {err}"),
            Some("general"),
        ),
    }
}

/// The tray menu's size, in logical pixels, and it is stated in one place.
///
/// The window is positioned by its top-left corner but anchors to its
/// bottom-right, so the height has to be known before it is shown.
///
/// Six rows at 30, two separators at 9, and 4 of padding top and bottom.
///
/// There is no border in that sum, and there must not be one in the page
/// either: `box-sizing: border-box` puts a border inside `height: 100vh`, so
/// a 1px one clips the last row by two pixels, which reads as a rendering
/// fault rather than as arithmetic. The window draws its own edge with an
/// inset catch instead, exactly as the launcher does.
const TRAY_MENU_SIZE: (f64, f64) = (216.0, 206.0);

/// Puts the notification-area menu at the cursor.
///
/// Anchored bottom-right rather than top-left, because the tray is in the
/// bottom-right corner of the screen and a menu drawn down-and-right from
/// there lands off the display. Clamped to the monitor's work area so it never
/// opens under the taskbar.
fn show_tray_menu(app: &AppHandle, cursor: tauri::PhysicalPosition<f64>) {
    let Some(window) = app.get_webview_window("traymenu") else {
        crate::say!("no tray menu window, so the tray has nothing to show");
        return;
    };

    let scale = window.scale_factor().unwrap_or(1.0);
    let (width, height) = TRAY_MENU_SIZE;
    let (w, h) = (width * scale, height * scale);

    // A gap, so the menu is not welded to the pointer.
    let gap = 8.0 * scale;
    let mut x = cursor.x - w;
    let mut y = cursor.y - h - gap;

    // The work area excludes the taskbar, which is exactly what must not be
    // covered. Falling back to the full monitor is better than not showing.
    if let Ok(Some(monitor)) = window.current_monitor() {
        let area = monitor.size();
        let origin = monitor.position();
        let (min_x, min_y) = (f64::from(origin.x), f64::from(origin.y));
        let max_x = min_x + f64::from(area.width) - w;
        let max_y = min_y + f64::from(area.height) - h;
        x = x.clamp(min_x, max_x.max(min_x));
        y = y.clamp(min_y, max_y.max(min_y));
    }

    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
    crate::sleep::wake(&window);
    let _ = window.show();
    let _ = window.set_focus();

    // Only a signal that it is up. The page starts each showing at the top,
    // and reads the bound hotkey itself: preferences live behind an async
    // mutex and a tray event handler is not a place to be waiting on one.
    let _ = window.emit("sill://tray-menu-shown", ());
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
        .map(|snippet| registry::snippet_record(snippet))
        .collect();

    if let Some(state) = app.try_state::<RegistryState>() {
        state.update_index(move |index| index.snippets = records);
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
        state.update_index(move |index| index.quicklinks = records);
    }
}

/// Rescans the script folders into the search index.
///
/// Scanning is a directory listing and a few kilobytes read per candidate, so
/// it happens when something changes rather than per keystroke, exactly as
/// snippets and quicklinks do. Off means no folders are touched at all, not
/// that the results are hidden afterwards.
pub(crate) fn reload_scripts(app: &AppHandle) {
    let (Some(prefs), Some(state)) = (
        app.try_state::<PrefsState>(),
        app.try_state::<RegistryState>(),
    ) else {
        return;
    };

    let (prefs, registry) = (prefs.inner.clone(), (*state).clone());

    tauri::async_runtime::spawn(async move {
        let (enabled, folders) = {
            let held = prefs.lock().await;
            (held.scripts.enabled, held.scripts.folders.clone())
        };

        let records: Vec<CommandRecord> = if enabled {
            let folders: Vec<std::path::PathBuf> =
                folders.iter().map(std::path::PathBuf::from).collect();

            // On a blocking thread: this is a directory listing plus the first
            // few kilobytes of every candidate, and doing that on the runtime
            // would stall every other task for the length of a cold folder.
            tokio::task::spawn_blocking(move || {
                scripts::scan(&folders)
                    .iter()
                    .map(registry::script_record)
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default()
        } else {
            Vec::new()
        };

        registry.update_index(move |index| index.scripts = records);
    });
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
    let workspaces = profiles_store::path(app);

    tauri::async_runtime::spawn(async move {
        let fresh = tokio::task::spawn_blocking(move || {
            scan_everything(&sources, &index_paths, &workspaces)
        })
        .await
        .unwrap_or_default();

        if fresh.is_empty() {
            return;
        }

        let total = fresh.len();
        state.update_index(move |index| index.commands = fresh);
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

/// Whether clicking away dismisses the launcher, asked at the moment it does.
///
/// Its own flag rather than a read of the preferences, because the answer is
/// needed inside a window event handler, which is synchronous, and the
/// preferences live behind an async lock. An atomic bool is also the whole of
/// what that handler needs to know.
#[derive(Default)]
pub struct DismissOnBlur(std::sync::atomic::AtomicBool);

impl DismissOnBlur {
    pub(crate) fn set(&self, yes: bool) {
        self.0.store(yes, std::sync::atomic::Ordering::Relaxed);
    }

    fn wanted(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Says out loud that the summon key never took.
///
/// Everything else about a refused key is visible in the settings window, in
/// the row that set it. The summon key is the exception that needed more,
/// because it is the key that opens the window where the message is: with it
/// taken there is no launcher, no taskbar button, and nothing on screen that
/// differs from Sill not running at all. On this machine that had already
/// happened, and the only record was one line in a log.
///
/// So the settings window is opened, once, at the section holding that row.
/// Heavy-handed for an ordinary setting, and right for this one: the
/// alternative is an application that starts and then cannot be reached.
fn report_summon_trouble(app: &AppHandle, summon: &str) {
    let taken = app
        .try_state::<HotkeyConflicts>()
        .map(|conflicts| conflicts.all().iter().any(|key| key == summon))
        .unwrap_or(false);

    if !taken {
        return;
    }

    /*
     * Through the status surface rather than straight at the tray.
     *
     * This used to write the tooltip itself, which was right when it was the
     * only thing that ever did. It is not any more: the tray icon, the startup
     * entry, the clipboard and two saved files all report now, and two writers
     * of one label means whichever ran last is the whole truth. Everything
     * goes through the one set, and the tray shows a count when there is more
     * than one thing wrong.
     */
    status::report(
        app,
        "summon-hotkey",
        format!(
            "{summon} is taken by another application, so there is no way to summon Sill. \
             Choose a different combination."
        ),
        Some("general"),
    );

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = commands::settings::open_settings(handle, Some("general".to_string())).await;
    });
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
        Ok(()) => {
            // Rebinding to a key that works is the fix for the trouble the
            // startup check reported, so it withdraws it. Without this the
            // tray would keep saying Sill cannot be summoned by the key that
            // had just summoned it.
            status::resolved(app, "summon-hotkey");
            println!("[sill] summon key registered: {accelerator}");
        }
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

    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    /*
     * Wired whatever the setting says, and the setting read when focus is
     * actually lost.
     *
     * `on_window_event` adds a handler rather than replacing one, so this
     * cannot be re-wired on save: turning the setting off and on again would
     * leave two handlers, and each further save another. Baking the preference
     * into whether the handler exists is what made this the one setting that
     * needed a restart, and it is the setting somebody most wants to try both
     * ways before deciding.
     */
    let wanted = DismissOnBlur::default();
    wanted.set(dismiss_on_blur);
    app.manage(wanted);

    let handle = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            if !handle.app_handle().state::<DismissOnBlur>().wanted() {
                return;
            }

            // Focus is already gone, so the previous window must not be
            // restored on top of whatever the user just clicked. Everything
            // else `summon::hide` does still applies, and letting the renderer
            // sleep is part of it: dismissing by clicking away is the ordinary
            // way this window goes, so a dismissal that skipped it would mean
            // the launcher almost never slept.
            let _ = handle.hide();
            summon::went_away(&handle);
        }
    });
}

/// Puts the OS material behind every window that floats over the desktop.
///
/// Both of them, and it has to be both. The launcher and the tray menu are the
/// two windows with nothing of Sill's behind them, so both need the compositor
/// to blur the desktop rather than an in-page `backdrop-filter`, which can only
/// reach content the page can see. A menu given the popover recipe instead
/// blurs nothing at all and shows the desktop straight through its own alpha.
///
/// `apply_backdrop` rounds the corners first, so this is also what keeps DWM's
/// clip in agreement with each page's own radius.
pub(crate) fn apply_backdrops(app: &AppHandle, backdrop: preferences::Backdrop, tint_alpha: u8) {
    for label in ["main", "traymenu"] {
        if let Some(window) = app.get_webview_window(label) {
            summon::apply_backdrop(&window, backdrop, tint_alpha);
        }
    }
}

/// Makes the tray menu dismiss when it loses focus.
///
/// A menu is expected to go away when you click elsewhere, and this one is a
/// real window, so nothing gives it that for free. Unconditional, unlike the
/// launcher's own dismissal: `dismiss_on_blur` is a preference about the
/// launcher, and a context menu that stayed on top of everything until it was
/// clicked would be a bug rather than a setting.
///
/// Wired once at startup rather than on each showing, because
/// `on_window_event` registers a handler rather than replacing one.
pub(crate) fn autohide_tray_menu(app: &AppHandle) {
    let Some(window) = app.get_webview_window("traymenu") else {
        return;
    };

    let handle = window.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = handle.hide();
            crate::sleep::sleep_soon(&handle);
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

/// Everything the launcher's first question needs, in place before there is a
/// window to ask it.
///
/// Tauri creates the windows declared in `tauri.conf.json` and *then* calls the
/// `setup` hook, so by the time the first line of that hook runs the launcher's
/// webview is already loading its page and can invoke. Preferences used to be
/// managed near the end of `setup`, after the saved file index had been read
/// back, and on a start that took 3.5 seconds rather than the usual 1.5 the
/// page got its first `search_commands` in first. Tauri answered "state not
/// managed for field prefs" and the root list stayed empty until the next
/// keystroke. Nothing about that was the page's fault, and it gets worse as
/// more is done during setup.
///
/// Called between `build` and `run`, which is the one place provably ahead of
/// every window: `build` hands back an `App` and Tauri does not create a window
/// until the event loop reports itself ready. Two other cures were available
/// and neither is this one. Making the window wait for a signal leaves the same
/// ordering to a listener that also has to be attached in time, and making the
/// command answer an empty list while starting hides the race rather than
/// removing it.
///
/// Only what a first paint actually resolves belongs here, which is two things.
/// Everything else those commands reach is managed on the builder, earlier
/// still, and everything the launcher holds that a first paint cannot reach is
/// still built in `setup` where it belongs.
fn manage_before_windows(app: &tauri::App) {
    let data_dir = state::data_dir(app.handle());

    // Before anything that might have something to report, which now includes
    // reading the preferences file.
    log::open(&data_dir);

    // And before anything that could fall over, so that when something does
    // there is a line saying where. A release build has no console, so without
    // this a panic is entirely silent.
    log::catch_panics();

    // Preferences first: the hotkey and the backdrop both come from them, so
    // reading them later would mean applying a default and then immediately
    // replacing it.
    let prefs_path = preferences::path(&data_dir);
    app.manage(PrefsState {
        inner: Arc::new(tokio::sync::Mutex::new(preferences::Preferences::load(
            &prefs_path,
        ))),
        path: Arc::new(prefs_path),
    });

    /*
     * The emoji corpus and the last forecast, before any window exists.
     *
     * `search_commands` resolves the first one, and the launcher can ask it a
     * question before the setup hook has run: Tauri creates every window
     * before calling that hook, so anything a first question touches has to be
     * managed here. `tests/startup_order.rs` says so, and said so about these
     * two the moment they were added in the wrong place.
     */
    app.manage(emoji::Emoji::default());
    app.manage(weather::Forecast::default());

    /*
     * Icons, remembered across runs.
     *
     * Extraction is shell and GDI calls, about a millisecond each, which is
     * nothing until the first list of a run asks for thirty at once and every
     * one of them is work the last run already did.
     *
     * Managed here rather than in setup for the same reason as the two above:
     * a first question can arrive before the setup hook runs. The file is only
     * *read* later, after the hotkey has been answered.
     */
    app.manage(icons::Icons::new(Some(data_dir.join("icons.json"))));

    /*
     * Readings of the machine that are reused for a moment.
     *
     * Each was a `static` with its own copy of the same six lines. Typing
     * "bluetooth" is eight keystrokes and was eight enumerations of the radios.
     */
    app.manage(state::Fresh::<system::Live>::new(system::FRESH_FOR));
    app.manage(state::Fresh::<Vec<registry::CommandRecord>>::new(
        windowing::FRESH_FOR,
    ));
    app.manage(state::Fresh::<Vec<app_volume::Session>>::new(
        app_volume::FRESH_FOR,
    ));

    /*
     * The file index's container, empty, whether or not anything will fill it.
     *
     * It used to be managed only when there were folders to index, which made
     * the same fault permanent rather than a race: turning the index off left
     * `search_files` and `file_search_missing` failing outright instead of
     * answering from a whole-volume indexer the way the setting's own note
     * promises. Tauri resolves a command's state before the body can read a
     * preference and decide, so a state managed on a condition is a state some
     * command cannot be called at all.
     *
     * Empty costs nothing: a pointer to a catalog with no entries in it.
     */
    app.manage(state::CatalogState {
        cache: Arc::new(Some(data_dir.join("file-index.bin"))),
        ..Default::default()
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
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
        .manage(commands::scripts::Running::new())
        .manage(timing::Timings::new())
        .manage(previews::Previews::new())
        .manage(ai::chat::Chat::new())
        .manage(ai::approval::Pending::new())
        .manage(ai::approval::Halt::new())
        // Nothing is bound yet. The port opens the first time a question goes
        // to Claude Code, which on most days is never.
        .manage(ai::mcp::link::Link::new())
        .manage(commands::system::Marking::default())
        // Nothing is asked at rest: a lock around a `None` until somebody
        // presses Enter on a row that would end the session.
        .manage(system::Asked::default())
        .manage(activity::Activity::default())
        .manage(meter::Meter::default())
        // Empty until somebody opens the store, and empty again the moment
        // they leave it. Nothing here fetches, warms up or refreshes.
        .manage(store::StoreState::default())
        .manage(tts::sapi::Sapi::default())
        // Which file or browser search is the newest, so an overtaken one can
        // stop rather than finish an answer nobody will look at.
        .manage(state::Searching::default())
        .manage(RegistryState {
            // Sill's own settings are a `const` table and cannot change while
            // the app runs, so they are here rather than filled in later.
            index: Arc::new(arc_swap::ArcSwap::from_pointee(state::Index {
                own_settings: settings_index::records(),
                ..Default::default()
            })),
            ranking: Arc::new(arc_swap::ArcSwap::default()),
            recording: Arc::new(std::sync::Mutex::new(())),
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let (tx, rx) = mpsc::unbounded_channel();
            forward_events(handle.clone(), rx);

            let data_dir = state::data_dir(&handle);

            // Before any shortcut is registered, so a refusal is recorded
            // rather than dropped.
            app.manage(HotkeyConflicts::default());

            // Before anything that can fail quietly, for the same reason.
            // Every report before this point reaches the log and nothing else.
            app.manage(status::Status::default());

            // What was said before Sill was last closed, so the list of past
            // conversations is not empty every morning. Nothing is opened:
            // yesterday's conversation is somewhere to go back to rather than
            // somewhere to be when the launcher appears.
            app.state::<ai::chat::Chat>().load(&data_dir);

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

            /*
             * One grant store, held twice.
             *
             * The API layer consults it before every call, and the settings
             * window lists and revokes through it. Two instances would mean a
             * permission revoked on screen still granted to the extension
             * running, which is the worst possible way for this to be wrong.
             */
            let grants = Arc::new(exthost::grants::Granted::new(handle.clone()));
            app.manage(grants.clone());

            app.manage(HostState {
                inner: Arc::new(tokio::sync::Mutex::new(None)),
                api: Arc::new(exthost::ApiLayer::new(
                    tx,
                    host_bridge::SillBridge::new(handle.clone()),
                    Arc::new(storage),
                    grants,
                )),
                host_js: Arc::new(host_js(&handle)),
                last_used: Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
                node: Arc::new(std::sync::Mutex::new(None)),
            });

            /*
             * The preferences, taken from managed state rather than read
             * again.
             *
             * They were loaded before this hook ran and before any window
             * existed, which is the whole point of `manage_before_windows`.
             * Reading the file a second time here would be a second source of
             * truth for one set of settings, and the two copies could disagree
             * the moment anything wrote between them.
             *
             * `blocking_lock` is what `load_registry` already does from this
             * same hook. Nothing holds this lock across work on the main
             * thread, so there is nothing here to wait behind.
             */
            let prefs = app.state::<PrefsState>().inner.blocking_lock().clone();

            apply_backdrops(
                &handle,
                prefs.appearance.backdrop,
                prefs.appearance.tint_alpha,
            );

            apply_window_size(&handle, &prefs.appearance);
            register_summon_shortcut(&handle, &prefs.hotkey.summon);
            register_switcher_shortcut(&handle, &prefs.hotkey.switcher);
            register_capture_shortcut(&handle, &prefs.hotkey.capture, false);
            register_capture_shortcut(&handle, &prefs.hotkey.capture_screen, true);
            // Nothing was bound before, so everything in the list is new.
            bindings::apply(&handle, &[], &prefs.bindings);
            apply_tray(&handle, prefs.general.show_in_tray);
            autohide_tray_menu(&handle);
            apply_autostart(&handle, prefs.general.open_at_login);
            watch_focus(&handle, prefs.hotkey.dismiss_on_blur);
            report_summon_trouble(&handle, &prefs.hotkey.summon);

            // Read on every summon, from a synchronous path, so it is kept as
            // an atomic rather than behind the preferences lock.
            let placement = placement::Placement::default();
            placement.set(prefs.appearance.summon_on);
            app.manage(placement);

            /*
             * Noticing that something was installed.
             *
             * Without this the index was built at startup and never again
             * unless somebody ran "Reload Sill Index" by hand, so anything
             * installed while Sill was running was invisible and the way to
             * find out was to fail to find it.
             */
            if let Some(watcher) =
                apps_watch::AppWatcher::start(handle.clone(), apps::shortcut_roots())
            {
                app.manage(watcher);
            }

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
            expander.set_tap_binding(tap_binding(&prefs.taps));
            expander.set_hyper(prefs.hyper.key);

            // Asked of the expander rather than of the preferences, because
            // two things want the hook now and only it knows whether either
            // of them does.
            if expander.wanted() {
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
                    retain_days: prefs.clipboard.retain_days,
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
            load_registry(app, &handle);

            /*
             * Cold start, measured at the last thing setup does.
             *
             * The number somebody cares about is "how long until the hotkey
             * works", and the hotkey is registered above. Everything after
             * this point is the index still being scanned on a background
             * task, which the launcher opens perfectly well without.
             *
             * Asked of Windows rather than measured from the first line of our
             * own code, because the loader and the runtime are part of what
             * was waited for.
             */
            if let Some(since_start) = timing::since_process_start() {
                if let Some(timings) = handle.try_state::<timing::Timings>() {
                    timings.ready(since_start);
                }
            }

            /*
             * Everything the hotkey does not need, after the hotkey works.
             *
             * The file index, the snippets and the quicklinks were all read
             * on the main thread before the stamp above, which is to say
             * before Sill answered its own key. None of them is needed then:
             * the file index is what the first *file* search reads, and the
             * other two fill lists that the launcher renders from whatever it
             * has. Reading them here costs a person nothing, because they are
             * done long before anybody finishes typing.
             *
             * On the blocking pool, in one task, because the order inside it
             * matters: `warm` puts last run's index up and `rebuild` replaces
             * it, and the other way round would show the stale one after the
             * fresh one had already arrived.
             */
            {
                let handle = handle.clone();
                let roots = prefs.files.indexed_roots();

                tauri::async_runtime::spawn_blocking(move || {
                    if !roots.is_empty() {
                        // The container is managed before any window exists.
                        // Only the filling of it waits for a preference to say
                        // there is something to fill it with.
                        let catalog = handle.state::<state::CatalogState>();

                        catalog.warm(&roots);
                        catalog.rebuild(roots.clone());

                        if let Some(watcher) =
                            state::CatalogWatcher::start(catalog.inner().clone(), roots)
                        {
                            handle.manage(watcher);
                        }
                    }

                    // Last run's icons, so the first list does not re-extract
                    // what has already been extracted. After the ready stamp,
                    // like everything else here.
                    handle.state::<icons::Icons>().warm();

                    // After the registry and the expander are both managed,
                    // since these fill in each of them.
                    reload_snippets(&handle);
                    reload_quicklinks(&handle);
                    reload_scripts(&handle);
                });
            }

            /*
             * The windows that exist at startup go to sleep too.
             *
             * `sleep_soon` was only ever armed on dismissal, so between
             * starting and the first summon-and-dismiss both renderers stayed
             * awake indefinitely. On a machine where Sill opens at login and
             * is not used until the afternoon, that is the whole morning spent
             * holding two live Chromium renderers for a window nobody has
             * looked at, which is exactly the state rule 23 is about.
             *
             * Armed rather than suspended now, because it is the same twenty
             * seconds the dismissal path waits: a launcher summoned five
             * seconds after login should not have to wake anything up.
             */
            for label in ["main", "traymenu"] {
                if let Some(window) = handle.get_webview_window(label) {
                    sleep::sleep_soon(&window);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quicklinks::commands::list_quicklinks,
            quicklinks::commands::save_quicklink,
            quicklinks::commands::delete_quicklink,
            quicklinks::commands::open_quicklink,
            quicklinks::commands::export_quicklinks,
            quicklinks::commands::import_quicklinks,
            commands::settings::get_preferences,
            commands::settings::set_preferences,
            commands::settings::open_settings,
            commands::system::quit_app,
            commands::search::search_commands,
            commands::search::search_elsewhere,
            commands::search::browser_profiles,
            commands::search::search_engines,
            commands::search::default_browser,
            commands::launch::extract_text_from_last_image,
            commands::launch::rename_path,
            commands::system::begin_capture,
            commands::system::cancel_capture,
            commands::system::capture_area,
            commands::system::capture_screen,
            commands::system::capture_targets,
            commands::system::capture_window,
            commands::system::capture_display,
            commands::system::last_image_entry,
            commands::system::open_markup,
            commands::system::markup_image,
            commands::system::finish_markup,
            commands::system::cancel_markup,
            commands::search::search_windows,
            commands::search::system_states,
            commands::search::summon_painted,
            commands::ai::ai_ready,
            commands::ai::ai_ask,
            commands::ai::ai_follow_up,
            commands::ai::ai_new,
            commands::ai::ai_attach,
            commands::ai::ai_stop,
            commands::ai::ai_limits,
            commands::ai::open_ask,
            commands::ai::ai_decide,
            commands::ai::ai_refuse_pending,
            commands::ai::ai_conversations,
            commands::ai::ai_forget,
            commands::ai::ai_forget_all,
            commands::ai::ai_resume,
            commands::ai::ai_transcript,
            commands::ai::ai_clear,
            commands::ai::ai_known,
            commands::ai::ai_named,
            commands::ai::ai_models,
            commands::search::window_preview,
            commands::search::forget_previews,
            commands::search::timings,
            commands::search::search_app_volume,
            commands::launch::move_path,
            commands::launch::search_destinations,
            commands::search::search_emoji,
            commands::search::file_search_missing,
            commands::search::list_drives,
            commands::search::index_folder,
            commands::search::start_file_search,
            commands::settings::hotkey_conflicts,
            commands::settings::status_troubles,
            commands::settings::note_unreadable,
            commands::settings::forget_unreadable,
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
            commands::extensions::install_extension,
            commands::extensions::extension_grants,
            commands::extensions::revoke_extension_grant,
            commands::store::store_browse,
            commands::store::store_close,
            commands::store::store_prepare,
            commands::store::store_install,
            commands::store::store_discard,
            commands::store::store_uninstall,
            commands::store::store_pins,
            commands::store::installed_extensions,
            commands::store::grant_extension_permission,
            commands::store::store_ready,
            commands::launch::perform_builtin,
            commands::system::app_icon,
            commands::system::piper_voices,
            commands::system::install_piper_voice,
            commands::system::remove_piper_voice,
            commands::system::speak_sample,
            commands::system::speak_piper_sample,
            snippets::commands::snippet_fields,
            snippets::commands::paste_snippet_filled,
            commands::scripts::script_arguments,
            commands::scripts::run_script,
            commands::scripts::cancel_script,
            commands::system::live_rows,
            commands::system::save_workspace,
            commands::system::restore_workspace,
            commands::system::forget_workspace,
            commands::system::machine_reading,
            commands::system::find_place,
            commands::system::weather_now,
            commands::system::forget_machine_reading,
            commands::system::activity,
            commands::system::undo_activity,
            commands::system::clear_activity,
            commands::diagnostics::diagnostics,
            commands::system::rebuild_index,
            commands::system::summon_with,
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
            snippets::commands::export_snippets,
            snippets::commands::import_snippets,
            snippets::commands::expand_snippet,
            snippets::commands::type_snippet,
            commands::system::dismiss
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // The gap between building and running is the point: the app exists and
    // not one window does. `run` takes the app by value, so there is no
    // afterwards in which this could accidentally be done late.
    manage_before_windows(&app);

    app.run(|_app, _event| {});
}

#[cfg(test)]
mod tests {
    use super::{DismissOnBlur, HotkeyConflicts};

    /// A key that binds after being refused stops being reported.
    ///
    /// The refusal is what the settings window reads, so a stale entry would
    /// paint a working shortcut red forever. This is not hypothetical: a
    /// combination is often taken because the previous owner is still closing,
    /// and Sill rebinds every time that row is edited.
    #[test]
    fn a_key_that_binds_the_second_time_stops_being_reported() {
        let conflicts = HotkeyConflicts::default();

        conflicts.note("Alt+Space", false);
        assert_eq!(conflicts.all(), vec!["Alt+Space".to_string()]);

        conflicts.note("Alt+Space", true);
        assert!(
            conflicts.all().is_empty(),
            "a key that bound is still reported as taken"
        );
    }

    /// One refused key does not hide another.
    ///
    /// Per-command shortcuts arrive here now as well as the four named ones,
    /// so there can be several at once and each has its own row to mark.
    #[test]
    fn every_refused_key_is_reported_not_just_the_last() {
        let conflicts = HotkeyConflicts::default();

        conflicts.note("Alt+Space", false);
        conflicts.note("Ctrl+Alt+W", false);

        let mut taken = conflicts.all();
        taken.sort();
        assert_eq!(
            taken,
            vec!["Alt+Space".to_string(), "Ctrl+Alt+W".to_string()]
        );
    }

    /// Whether clicking away dismisses is answered now, not at startup.
    ///
    /// The window event handler is wired once and reads this, because
    /// `on_window_event` adds handlers rather than replacing them. Wiring it
    /// per save would dismiss the launcher once per time the setting had ever
    /// been switched on.
    #[test]
    fn the_dismissal_setting_is_read_after_it_changes() {
        let blur = DismissOnBlur::default();
        blur.set(true);
        assert!(blur.wanted());

        blur.set(false);
        assert!(!blur.wanted(), "turning it off still needs a restart");
    }
}
