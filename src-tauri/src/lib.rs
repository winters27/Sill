pub mod action;
pub mod action_keys;
pub mod actions;
pub mod activity;
pub mod ai;
pub mod app_volume;
pub mod apps;
pub mod apps_watch;
pub mod audio;
pub mod automation;
pub mod bindings;
pub mod bounded;
pub mod browsers;
pub mod bundle;
pub mod calculator;
pub mod capture;
pub mod catalog;
pub mod clipboard;
pub mod colour;
pub mod commands;
pub mod complete;
pub mod content;
pub mod controls;
pub mod dates;
pub mod desktops;
pub mod dialog;
pub mod displays;
pub mod dictation;
pub mod emoji;
pub mod everything_ipc;
pub mod explorer;
pub mod extension_install;
pub mod exthost;
pub mod files;
pub mod files_ops;
pub mod fonts;
pub mod games;
pub mod hello;
pub mod hooks;
pub mod host;
pub mod host_bridge;
pub mod hotkeys;
pub mod hyper;
pub mod icons;
pub mod images;
pub mod input;
pub mod job;
pub mod json_store;
pub mod jumplists;
pub mod keysheet;
pub mod layouts;
pub mod lazy_windows;
pub mod leavings;
pub mod live;
pub mod lnk;
pub mod log;
pub mod media;
pub mod meter;
pub mod navigation;
pub mod notes;
pub mod object;
pub mod ocr;
pub mod outside;
pub mod placement;
pub mod preferences;
pub mod preferences_transfer;
pub mod previews;
pub mod privacy;
pub mod processes;
pub mod profiles;
pub mod profiles_store;
pub mod qr;
pub mod quicklinks;
pub mod radios;
pub mod reach;
pub mod recycle_bin;
pub mod registry;
pub mod scripts;
pub mod secrets;
pub mod selection;
pub mod session;
pub mod settings_catalog;
pub mod settings_index;
pub mod shell;
pub mod sleep;
pub mod snippets;
pub mod state;
pub mod status;
pub mod store;
/// Behaviour tests that used to be a Cargo binary each. See `suite/mod.rs`.
#[cfg(test)]
mod suite;
pub mod summon;
pub mod sums;
pub mod synthetic;
pub mod system;
pub mod taps;
pub mod terminals;
pub mod text;
pub mod timers;
pub mod timing;
pub mod tts;
pub mod uia;
pub mod update;
pub mod utilities;
pub mod weather;
pub mod webchrome;
pub mod websearch;
pub mod welcome;
pub mod windowing;
pub mod zones;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use registry::{CommandRecord, Frecency};

use host::{forward_events, host_js, index_paths};
use state::{now_seconds, HostState, PrefsState, RegistryState};

/// What the setting means, in one place both callers use.
///
/// A switch rather than a list of levels, because there are two and one of
/// them is the floor. `log.rs` explains why there is nothing below it.
pub(crate) fn log_level(detailed: bool) -> log::Level {
    if detailed {
        log::Level::Detailed
    } else {
        log::Level::Normal
    }
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
         * Held for the whole of the first build, and put down however it ends.
         *
         * What the launcher draws over an empty list depends on it: "still
         * reading what is installed" while this is up, "no results for that
         * word" once it is down. Two of the paths below give up early, so it
         * is a guard rather than a store at the end.
         */
        let building = state::FirstScan::hold(state.first_scan.clone());

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

            adopt_commands(&handle, &state, cached, Some(aliases.clone())).await;

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
        adopt_commands(&handle, &state, fresh, Some(aliases)).await;

        if let Some(text) = text {
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(err) = registry::write_cache(&cache_path, &text) {
                    // A missing cache only costs a slower next start.
                    crate::say!("could not write the index cache: {err}");
                }
            });
        }

        println!("[sill] indexed {total} entries");

        // Before the announcement, not after it. The window asks again when
        // it hears this, and an answer of "still reading" to a question asked
        // because the reading finished would be the wrong one by a frame.
        drop(building);

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

    // Folders somebody named themselves, walked exactly as the Start Menu is.
    let mine = apps::scan_folders(&sources.folders);

    // Games, which no other source above can see: Steam writes a Start Menu
    // entry for almost nothing, so a machine full of them looks empty.
    let games = if sources.games {
        games::scan()
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

    // Before the registry sources: somebody who named a folder meant the copy
    // in it, and a name collision should not hand the row to a guess made from
    // an uninstall entry.
    for app in &mine {
        if keep(app, &mut names, &mut targets) {
            out.push(registry::app_record(
                &app.name,
                &app.path,
                app.icon_source.clone(),
                apps::categorize(app),
            ));
        }
    }

    for app in &games {
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
            /*
             * A program on PATH is a command-line program, wherever it lives.
             *
             * This used to ask `categorize`, which answers by where a thing
             * resolves, and then rename only its "Application" answer. Every
             * other answer passed through, so **656 of the 1,142 entries on
             * this machine said "System"** because `reg`, `runonce` and
             * `openfiles` sit under the Windows directory. They then drew
             * under the heading "Command Line" while saying "System" on the
             * right: two different answers to "what is this" on one row.
             *
             * It also made "System" mean three unrelated things in one list,
             * since an application in the Windows folder and a Windows toggle
             * both say it too, and a toggle is a thing you flip.
             *
             * Where it lives is not what kind of thing it is, and where it
             * lives is already on the row: `executable_record` puts the full
             * path in the subtitle, which is the only thing telling three
             * Pythons apart.
             */
            out.push(registry::executable_record(
                &app.name,
                &app.path,
                "Command Line",
            ));
        }
    }

    println!(
        "[sill] {} shortcuts, {} packaged, {} on PATH, {} in named folders, {} games",
        shortcuts.len(),
        packaged.len(),
        on_path.len(),
        mine.len(),
        games.len()
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
///
/// `paused` is private mode, and it is a parameter rather than something read
/// here so that every caller has to answer it. The two that apply preferences
/// pass what the preferences say; the hook's own health check passes what the
/// service already holds, because re-arming a hook is not the moment to change
/// what private mode is set to.
pub(crate) fn apply_dictation(
    app: &AppHandle,
    settings: &dictation::models::DictationSettings,
    paused: bool,
) {
    let service = app.state::<dictation::service::DictationService>();
    service.set_settings(settings.clone());
    // Before the arming below, so there is no instant in which the hook is
    // armed and the refusal is not in place.
    service.set_paused(paused);

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
                &link.tags,
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

/**
Puts a freshly scanned set of commands in the index, and rebuilds what the
installed extensions contribute to the action panel.

**The only place `index.commands` is replaced**, and `verify:source` holds that.
The two things here are one fact read twice: the index says which extension
commands exist, and the action registry says which of them can be run on a
file. A second place that set one without the other is the shape this codebase
already knows by heart, and here it would be an action panel offering to run a
command out of an extension that has just been uninstalled.

The contribution is built from the list about to land rather than read back
afterwards, so there is no window in which the two disagree and no second
lookup to get wrong.

`aliases` only when the caller has some; the scan carries them and a reindex
does not.

**Both kinds of contributed action are built here**, and that is the same rule
arriving a second time. An extension declares its actions in a manifest the
index carries; a configured MCP server declares its own in the preferences.
`ActionRegistry::contribute` replaces the whole list, so anything rebuilding one
half without the other would silently drop the other half: installing an
extension would take away every MCP action until the next settings save, and
saving a setting would take away every extension action until the next scan.
One funnel, both halves, every time. `verify:source` holds all of it.

Reading the MCP declarations here rather than taking them as a parameter is
what keeps the preferences the one source of truth for them. Nothing is started
and nothing is asked of any server: see [`actions::mcp::contributed`].
*/
pub(crate) async fn adopt_commands(
    app: &AppHandle,
    state: &RegistryState,
    commands: Vec<registry::CommandRecord>,
    aliases: Option<registry::Aliases>,
) {
    let mut contributed = actions::extension::contributed(&commands);

    // Held for one clone and let go, before anything else here runs. The
    // settings lock is taken by every save and by the AI gate, and this is on
    // the path a rescan takes.
    let servers = match app.try_state::<PrefsState>() {
        Some(prefs) => prefs.inner.lock().await.mcp.servers.clone(),
        // Only reachable before preferences are managed, which is before any
        // scan has run. An empty list is the honest answer rather than a panic.
        None => Vec::new(),
    };

    contributed.extend(actions::mcp::contributed(&servers));

    state.update_index(move |index| {
        index.commands = commands;
        if let Some(aliases) = aliases {
            index.aliases = aliases;
        }
    });

    app.state::<action::ActionRegistry>()
        .contribute(contributed);
}

/// Rebuilds what is contributed, without rescanning anything.
///
/// For a settings save that changed the MCP servers. The index's commands have
/// not moved, so they are read back out and handed straight to the funnel
/// rather than a second route being opened to the action registry: the whole
/// point of `adopt_commands` being the only caller of `contribute` is that
/// there is nowhere else for the two halves to come apart.
pub(crate) async fn readopt_commands(app: &AppHandle) {
    let state = app.state::<RegistryState>().inner().clone();
    let commands = state.index().commands.clone();

    adopt_commands(app, &state, commands, None).await;
}

/// Rebuilds the index in the background.
pub(crate) fn reload_index(app: &AppHandle) {
    use std::sync::atomic::Ordering;

    let handle = app.clone();
    let state = app.state::<RegistryState>().inner().clone();

    // One scan at a time. Six source switches pressed in a row are six saves,
    // and each scan is a PowerShell round trip plus every Start Menu shortcut
    // on the machine. The request that arrives during one is remembered rather
    // than dropped, so the last switch pressed is in the index that ends up
    // being kept: dropping it would leave the list disagreeing with the screen
    // until something else asked for a scan.
    if state
        .scanning
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        state.rescan.store(true, Ordering::Release);
        return;
    }

    let prefs = app.state::<PrefsState>().inner.clone();
    let index_paths = index_paths(app);
    let workspaces = profiles_store::path(app);

    tauri::async_runtime::spawn(async move {
        loop {
            // Read per pass rather than once, because the pass exists to pick
            // up whatever changed while the previous one was running.
            let sources = prefs.lock().await.sources.clone();
            let (paths, places) = (index_paths.clone(), workspaces.clone());

            let fresh =
                tokio::task::spawn_blocking(move || scan_everything(&sources, &paths, &places))
                    .await
                    .unwrap_or_default();

            if !fresh.is_empty() {
                let total = fresh.len();
                adopt_commands(&handle, &state, fresh, None).await;
                println!("[sill] reindexed {total} entries");
                let _ = handle.emit("sill://registry-updated", total);
            }

            state.scanning.store(false, Ordering::Release);

            if !state::take_repeat(&state.rescan, &state.scanning) {
                return;
            }
        }
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

/// Whether Sill has ever run here, and whether that can be answered yet.
///
/// ## The fact, and where it comes from
///
/// A first run is a start with no settings file. That is the fact itself
/// rather than a flag about it: nothing has to be written to make it true, it
/// stops being true the moment anything saves, and it is right on a machine
/// Sill has been uninstalled from and put back on. A `welcomed: true` field
/// would be a second thing that could be wrong on its own.
///
/// ## Why the answer is withheld for a moment
///
/// Tauri creates the launcher's window **before** `setup` runs, so the page is
/// already loading while the summon key is still being registered. The one
/// sentence the welcome exists to get right is what that registration
/// answered, and answering the window early would mean answering it "the key
/// is fine" because nothing had asked for it yet. So `asked` is set once the
/// key has been offered to Windows, and until then this says nothing at all.
/// The window asks on mount and again when Rust tells it the launcher was
/// opened for this, and exactly one of those two lands after the key.
pub struct FirstRun {
    /// The welcome is owed and has not been handed over.
    ///
    /// Taken rather than read, so a summon after the welcome has been shown
    /// does not put it back on screen over whatever somebody is doing.
    owed: std::sync::atomic::AtomicBool,
    /// The summon key has been registered and the outcome recorded.
    asked: std::sync::atomic::AtomicBool,
    /// There was no settings file, so writing one is what records that Sill
    /// has now been set up here.
    unwritten: bool,
}

impl FirstRun {
    /// `unwritten` is deliberately not `owed`.
    ///
    /// Forcing the welcome with `SILL_FIRST_RUN` on a machine that already has
    /// settings must not write over them, and a settings file that is already
    /// there is already the record that Sill has been set up here.
    fn new(owed: bool, unwritten: bool) -> Self {
        Self {
            owed: std::sync::atomic::AtomicBool::new(owed),
            asked: std::sync::atomic::AtomicBool::new(false),
            unwritten,
        }
    }

    /// Says the summon key has been offered to Windows and answered.
    fn key_was_asked_for(&self) {
        self.asked.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Whether a welcome is owed, without taking it.
    ///
    /// For the startup path, which decides what else to do about a refused key
    /// but is not the thing that hands the welcome over.
    fn owed(&self) -> bool {
        self.owed.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Whether a welcome is owed, and takes it if one is.
    ///
    /// False while the summon key has not been asked for: not "no welcome",
    /// but "not yet", and the window asks again when Rust says the launcher
    /// was opened for this.
    ///
    /// Taken rather than read, so the welcome is handed over once. The window
    /// asks on mount and on the event, and only one of those two should put a
    /// welcome on screen.
    pub(crate) fn take(&self) -> bool {
        use std::sync::atomic::Ordering;

        self.asked.load(Ordering::Acquire) && self.owed.swap(false, Ordering::AcqRel)
    }

    /// Whether the registration outcome is known, which is what makes the
    /// welcome answerable at all.
    pub(crate) fn answerable(&self) -> bool {
        self.asked.load(std::sync::atomic::Ordering::Acquire)
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

/// The settings panel holding the row that sets the summon key.
///
/// It has moved twice. `P1-11` wrote `general`; `P5-06` moved every hotkey
/// under Shortcuts and this followed; on 2026-09-05 a key went back to the
/// panel of the thing it does, so the summon key is under General again. Each
/// time, the window went on opening a panel that no longer had the row until
/// this caught up, which is why `verify-source` reads the settings catalogue
/// and refuses a section here that does not hold that row.
const SUMMON_SECTION: &str = "general";

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
///
/// **Except on a first run**, where the welcome is about to say the same thing
/// with more room and the fix on it. `welcome::also_open_settings` holds that
/// rule and a test says so.
fn report_summon_trouble(app: &AppHandle, summon: &str, first_run: bool) {
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
        Some(SUMMON_SECTION),
    );

    if !welcome::also_open_settings(true, first_run) {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = commands::settings::open_settings(handle, Some(SUMMON_SECTION.to_string())).await;
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

                /*
                 * After the window, never before it.
                 *
                 * This is the one keystroke Sill can be certain somebody made,
                 * which makes it the only moment a keyboard hook that has
                 * counted nothing is provably not being called. It is two
                 * atomic loads and a comparison, and it still goes after the
                 * summon, because nothing belongs between a key being pressed
                 * and the launcher being on screen.
                 */
                hooks::check(&handle, hooks::Cause::Typed);
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
            /*
             * In the log rather than only on a console, unlike the failure
             * below, which was already there.
             *
             * The asymmetry was the problem: the log said when the key could
             * not be taken and never said which key was taken, so the one
             * fact that decides whether the launcher can be reached at all
             * was the one fact nothing recorded. It matters because the key
             * is a setting and because registration can be refused by
             * whatever else on the machine already owns the chord, so what
             * the preferences ask for and what actually answers are two
             * different things. `measure-keystroke.ps1` presses what this
             * line names; assuming Alt+Space, which is what
             * `measure-summon.ps1` does, presses nothing on a machine where
             * somebody changed it.
             */
            crate::say!("summon key registered: {accelerator}");
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

    /*
     * Whether Sill has run here, asked before the file is read.
     *
     * `Preferences::load` puts an unreadable file aside and can leave one
     * behind it, so this is the last moment the question has a clean answer.
     *
     * `SILL_FIRST_RUN` forces it, which is how the welcome is looked at again
     * without throwing away somebody's settings to do it. It does not make the
     * file absent, so nothing is written and nothing is lost.
     */
    let fresh = !prefs_path.exists();
    let forced = std::env::var_os("SILL_FIRST_RUN").is_some();

    // Said out loud, like `SILL_NO_AUTOHIDE` below. A welcome appearing on a
    // machine that has been set up for months is alarming without a line
    // saying it was asked for.
    if forced && !fresh {
        println!("[sill] SILL_FIRST_RUN set: showing the welcome without writing anything");
    }

    app.manage(FirstRun::new(fresh || forced, fresh));

    let prefs = preferences::Preferences::load(&prefs_path);

    // Straight away, so a session started to reproduce a fault is detailed
    // from its first line rather than from whenever settings happened to be
    // opened.
    log::set_level(log_level(prefs.general.detailed_log));

    /*
     * Private mode, before any window exists and therefore before any capture
     * command can be reached.
     *
     * Windows are created before the setup hook runs, so a mirror pointed at
     * the preferences later would leave a window in which Sill was in private
     * mode and photographing the screen anyway. The same reasoning as the
     * emoji corpus below.
     */
    let privacy = privacy::Privacy::default();
    privacy.set(prefs.privacy.paused);
    app.manage(privacy);

    app.manage(PrefsState {
        inner: Arc::new(tokio::sync::Mutex::new(prefs)),
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
    app.manage(state::Fresh::<Vec<processes::Process>>::new(
        processes::FRESH_FOR,
    ));
    // Windows' own time zone table, read the first time a city is asked
    // about and held for an hour. Nothing reads it before then.
    app.manage(state::Fresh::<Arc<Vec<zones::Zone>>>::new(zones::FRESH_FOR));
    // The installed fonts, read the first time `font` is typed and held ten
    // minutes. Nothing reads them before then.
    app.manage(state::Fresh::<Arc<Vec<String>>>::new(fonts::FRESH_FOR));
    /*
     * What is playing, which is read only when somebody asks about it.
     *
     * Managed unconditionally like the four above, and for the reason the
     * comment under `CatalogState` gives: Tauri resolves a command's state
     * before the body can read a preference and decide, so a state managed on
     * a condition is a state some command cannot be called at all. Empty costs
     * a mutex holding a `None`.
     */
    app.manage(state::Fresh::<Option<media::NowPlaying>>::new(
        media::FRESH_FOR,
    ));
    /*
     * The Recent folder's listing, for the same reason as the four above.
     *
     * A directory of a few hundred shortcuts, read when a query asks and held
     * for a few seconds so typing one character at a time reads it once. Behind
     * an `Arc` because `Fresh` hands back a clone and three hundred strings per
     * keystroke would cost more than the listing does.
     */
    app.manage(state::Fresh::<Arc<Vec<files::Trace>>>::new(
        files::RECENT_FRESH_FOR,
    ));
    /*
     * What every application remembers having opened, for the same reason.
     *
     * A different list from the one above: that is the Recent folder, which is
     * the shell's own shortcuts, and this is the two hundred jump lists behind
     * the taskbar's right-click menus. Read only when a query's first word
     * asks for it, held for a few seconds because the words after it narrow
     * the list, and bounded to three hundred documents.
     */
    app.manage(state::Fresh::<Vec<jumplists::Recent>>::new(
        jumplists::FRESH_FOR,
    ));
    /*
     * The terminals and shells this machine has, for the same reason again.
     *
     * Terminal's settings file and the WSL keys in the registry, read when a
     * query's first word asks for a terminal and held for a minute: it changes
     * when somebody edits their settings, which is not while they are typing.
     */
    app.manage(state::Fresh::<Vec<terminals::Profile>>::new(
        terminals::FRESH_FOR,
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
    // Managed here, and unconditionally, because Tauri hands managed state out
    // by type and sets it once: a watcher managed only when there were folders
    // to watch could never be created later, so adding a first folder in
    // Settings indexed it once and then stopped noticing it.
    app.manage(state::Watching::default());

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
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            /*
             * The second launch may be carrying something.
             *
             * Windows starts a protocol handler by running it, so a
             * `sill://run/...` address somebody clicked arrives here as the
             * command line of a process that is about to be turned away, and
             * so does `sill run`. Both are read in one place; see
             * [`outside::arrived`].
             *
             * Asked before the toggle rather than after, because a launcher
             * window flashing open and shut is not what somebody clicking a
             * link asked for, and because the card the request raises decides
             * for itself which window it needs.
             */
            if outside::arrived(app, &argv) {
                return;
            }

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
        // Nothing happens here at startup: no socket is opened until a summon
        // asks, and then at most once a day. The restart afterwards is
        // `AppHandle::restart`, which is core, so there is no second plugin.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(actions::builtins())
        // A mutex around a small enum. Nothing is checked, downloaded or timed
        // until a summon asks, and then at most once a day.
        .manage(update::Updates::default())
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
        .manage(commands::system::Choosing::default())
        .manage(sums::Sums::default())
        // Nothing is asked at rest: a lock around a `None` until somebody
        // presses Enter on a row that would end the session.
        .manage(system::Asked::default())
        .manage(activity::Activity::default())
        .manage(meter::Meter::default())
        // Empty until somebody opens the store, and empty again the moment
        // they leave it. Nothing here fetches, warms up or refreshes.
        .manage(store::StoreState::default())
        .manage(tts::sapi::Sapi::default())
        // An empty `Vec` behind a lock, and it stays empty. The file is not
        // opened until a query's first word asks for a note, and on a machine
        // where notes are switched off that never happens.
        .manage(notes::Notes::default())
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
            scanning: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            rescan: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            // True until the first scan lands. The window exists before the
            // hook that starts that scan, so the honest answer to an early
            // question is "still reading" rather than "nothing here".
            first_scan: Arc::new(std::sync::atomic::AtomicBool::new(true)),
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
                    // The same one the commands report from, taken rather than
                    // made: two of these would mean the panel showed openings
                    // nobody had and the extension layer recorded openings
                    // nobody could see.
                    app.state::<timing::Timings>().inner().clone(),
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

            /*
             * The summon key has now been offered to Windows and answered.
             *
             * Said before anything reads the answer, and said whatever the
             * answer was: the welcome is unanswerable until this point and the
             * window may already have asked, so the flag is what releases it.
             */
            let first_run = {
                let first = app.state::<FirstRun>();
                first.key_was_asked_for();
                first.owed()
            };

            report_summon_trouble(&handle, &prefs.hotkey.summon, first_run);

            if first_run {
                /*
                 * The settings file, written once.
                 *
                 * Its absence is what says this is a first run, so writing it
                 * is what stops the welcome being every run. Only when there
                 * was not one: `SILL_FIRST_RUN` shows the welcome again on a
                 * machine that already has settings, and that must not put
                 * them through a save.
                 */
                if app.state::<FirstRun>().unwritten {
                    let state = app.state::<PrefsState>();
                    if let Err(err) = prefs.save(&state.path) {
                        crate::say!("could not write the first settings file: {err}");
                    }
                }

                summon::show_welcome(&handle);
            }

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
            // A folder somebody added is watched on the same terms as the
            // Start Menu. Dropping a shortcut into your own tools folder and
            // having to restart to find it is the exact complaint the watcher
            // exists to answer.
            let mut watched = apps::shortcut_roots();
            watched.extend(
                prefs
                    .sources
                    .folders
                    .iter()
                    .map(|one| std::path::PathBuf::from(icons::expand_env(one))),
            );

            if let Some(watcher) = apps_watch::AppWatcher::start(handle.clone(), watched) {
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
            expander.set_hotkeys(hotkeys::from_prefs(&prefs));

            // Asked of the expander rather than of the preferences, because
            // two things want the hook now and only it knows whether either
            // of them does.
            if expander.wanted() {
                snippets::expander::watch(&handle, &expander);
            }
            app.manage(expander);

            /*
             * What each keyboard hook looked like the last time anybody asked,
             * and the window that says when to ask again.
             *
             * Managed before the listener is installed, because the first thing
             * a resume does is read it. Nothing here runs while the machine is
             * idle: the window procedure is only entered when a message arrives
             * and Windows only sends these when the user goes away and comes
             * back.
             */
            app.manage(hooks::Watch::default());

            if let Some(window) = app.get_webview_window("main") {
                session::watch(&window);
            }

            // The browser's own chrome, off in every window that exists at
            // this point (the launcher and the tray menu). Windows built on
            // demand are quieted where they are built.
            for window in app.webview_windows().values() {
                webchrome::quiet(window);
            }

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
                // Derived rather than filled in, and by the same function the
                // settings apply uses, so private mode cannot be honoured on
                // a settings save and forgotten at startup. That difference
                // would be invisible: a launcher started in private mode
                // would record everything until somebody opened settings.
                history.set_rules(crate::privacy::clipboard_rules(&prefs));

                // The count cap, once, beside the retention prune above. Both
                // run again from the recording path; this is for the history
                // that grew past a limit set while Sill was not running.
                match history.store().trim_to(prefs.clipboard.max_entries, None) {
                    Ok(0) => {}
                    Ok(gone) => crate::say!("trimmed {gone} clipboard entries past the limit"),
                    Err(err) => crate::say!("could not trim the clipboard: {err}"),
                }
                if crate::privacy::clipboard_rules(&prefs).enabled {
                    clipboard::monitor::watch(&handle, &history);
                }
                app.manage(history);
            }

            // After the manage calls above: this resolves the service out of
            // managed state, which panics if it is not there yet.
            apply_dictation(
                &handle,
                &crate::privacy::dictation_settings(&prefs),
                prefs.privacy.paused,
            );

            // The standing sign that private mode is on, put up at startup as
            // well as when it is switched on. It persists across restarts on
            // purpose, and a mode that survived silently would be the failure
            // it exists to prevent.
            crate::privacy::report(&handle, prefs.privacy.paused);

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

                        handle
                            .state::<state::Watching>()
                            .re_root(catalog.inner().clone(), roots);
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

            /*
             * The command line this process was started with.
             *
             * Clicking a `sill://` link while Sill is not running starts it,
             * so the address arrives here rather than at the single instance
             * callback, and a launcher that only read the second launch would
             * answer every link except the first one of the day. Same
             * function, same gate; the only difference is which of the two
             * ways in Windows chose.
             */
            let started_with: Vec<String> = std::env::args().collect();
            outside::arrived(&handle, &started_with);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            quicklinks::commands::list_quicklinks,
            quicklinks::commands::save_quicklink,
            quicklinks::commands::quicklink_scheme_to_allow,
            quicklinks::commands::delete_quicklink,
            quicklinks::commands::open_quicklink,
            quicklinks::commands::export_quicklinks,
            quicklinks::commands::import_quicklinks,
            commands::settings::get_preferences,
            commands::settings::set_preferences,
            commands::settings::export_preferences,
            commands::settings::import_preferences,
            commands::settings::reset_panel,
            commands::settings::resettable_panels,
            commands::settings::open_settings,
            commands::system::quit_app,
            commands::automation::automations,
            commands::automation::schedulable,
            commands::automation::schedule,
            commands::automation::unschedule,
            commands::mcp::mcp_tools,
            commands::notes::note_read,
            commands::notes::note_write,
            commands::notes::note_forget,
            commands::search::search_commands,
            commands::search::search_elsewhere,
            commands::search::complete_path,
            commands::search::browser_profiles,
            commands::search::search_engines,
            commands::search::default_browser,
            commands::launch::extract_text_from_last_image,
            commands::system::begin_capture,
            commands::system::cancel_capture,
            commands::system::capture_purpose,
            commands::system::chose_area,
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
            commands::search::painted,
            commands::ai::ai_ready,
            commands::ai::ai_hello,
            commands::ai::ai_ask,
            commands::ai::ai_follow_up,
            commands::ai::ai_new,
            commands::ai::ai_attach,
            commands::ai::ai_stop,
            commands::ai::ai_limits,
            commands::ai::open_ask,
            commands::ai::ai_decide,
            commands::ai::ai_outstanding,
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
            commands::search::file_preview,
            commands::search::forget_file_previews,
            commands::search::timings,
            commands::search::search_app_volume,
            commands::search::search_processes,
            commands::search::search_controls,
            commands::launch::search_destinations,
            commands::search::search_emoji,
            commands::search::file_search_missing,
            commands::search::list_drives,
            commands::search::index_folder,
            commands::search::start_file_search,
            commands::search::start_everything,
            commands::search::index_building,
            commands::settings::hotkey_conflicts,
            commands::update::update_state,
            commands::update::check_for_update,
            commands::update::install_update,
            commands::update::restart_for_update,
            commands::settings::status_troubles,
            commands::settings::note_unreadable,
            commands::settings::forget_unreadable,
            commands::settings::set_alias,
            commands::settings::index_rows,
            commands::settings::set_command_hotkey,
            commands::settings::set_hidden,
            commands::settings::set_pinned,
            commands::settings::navigation_chords,
            commands::settings::navigation_keys,
            commands::settings::action_shortcuts,
            commands::settings::keyboard_reference,
            commands::settings::key_owners,
            commands::settings::welcome,
            commands::settings::terminal_profiles,
            commands::settings::emoji_tones,
            commands::search::list_monitors,
            commands::search::open_path,
            commands::launch::launch_command,
            commands::launch::record_use,
            commands::launch::query_history,
            commands::launch::actions_for,
            commands::launch::run_action,
            commands::launch::undo_action,
            commands::extensions::activate_handler,
            commands::extensions::unload_extension,
            commands::extensions::install_extension,
            commands::extensions::extension_grants,
            commands::extensions::extension_stored_fields,
            commands::extensions::remember_extension_field,
            commands::extensions::extension_resources,
            commands::extensions::revoke_extension_grant,
            commands::extensions::pick_files,
            commands::store::store_browse,
            commands::store::store_close,
            commands::store::store_prepare,
            commands::store::store_install,
            commands::store::store_discard,
            commands::store::store_uninstall,
            commands::store::store_pins,
            commands::store::installed_extensions,
            commands::store::grant_extension_permission,
            commands::store::extension_preferences,
            commands::store::set_extension_preference,
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
            commands::system::make_workspace_portable,
            commands::system::forget_workspace,
            commands::system::machine_reading,
            commands::system::find_place,
            commands::system::weather_now,
            commands::system::world_clocks,
            commands::system::throw_confetti,
            commands::system::finish_confetti,
            commands::system::forget_machine_reading,
            commands::system::activity,
            commands::system::undo_activity,
            commands::system::clear_activity,
            commands::diagnostics::diagnostics,
            commands::diagnostics::export_diagnostics,
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
    use super::{DismissOnBlur, FirstRun, HotkeyConflicts};

    /**
    The welcome is not answered before the summon key has been asked for.

    This is the ordering the whole first run turns on. Tauri creates the
    launcher's window **before** the `setup` hook that registers the key, so
    the page can ask what to show while nothing has yet asked Windows for
    anything. Answering then would answer "the key is fine", which on this
    machine is false and has been at every start for weeks.

    So the page asks twice, on mount and again when Rust says the launcher was
    opened for this, and this is the flag that decides which of the two gets an
    answer.
    */
    #[test]
    fn nothing_is_owed_until_the_summon_key_has_been_asked_for() {
        let first = FirstRun::new(true, true);

        assert!(
            !first.answerable(),
            "answerable before the key was asked for"
        );
        assert!(!first.take(), "the welcome was handed over before the key");

        first.key_was_asked_for();

        assert!(first.answerable());
        assert!(first.take(), "the welcome was lost by asking too early");
    }

    /// And it is handed over once.
    ///
    /// Both the mount question and the event ask, and a welcome that came back
    /// on the second would appear over whatever somebody had started doing.
    #[test]
    fn the_welcome_is_handed_over_exactly_once() {
        let first = FirstRun::new(true, true);
        first.key_was_asked_for();

        assert!(first.take());
        assert!(!first.take(), "the welcome came back a second time");
    }

    /// A start that is not a first run owes nothing, whenever it is asked.
    #[test]
    fn a_start_that_is_not_a_first_run_owes_nothing() {
        let first = FirstRun::new(false, false);
        first.key_was_asked_for();

        assert!(first.answerable());
        assert!(!first.take());
    }

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
