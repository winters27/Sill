//! Reading and writing Sill's own preferences, and the window that edits them.

use crate::{
    apply_autostart, apply_dictation, apply_tray, apply_window_size, rebind_summon,
    rebind_switcher, same_dictation,
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::PrefsState;
use crate::{clipboard, preferences, settings_index, snippets, summon};

#[tauri::command]
pub(crate) async fn get_preferences(
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
pub(crate) async fn set_preferences(
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

    if previous.bindings != prefs.bindings {
        crate::bindings::apply(&app, &previous.bindings, &prefs.bindings);
    }

    if previous.hotkey.summon != prefs.hotkey.summon {
        rebind_summon(&app, &previous.hotkey.summon, &prefs.hotkey.summon);
    }

    if previous.hotkey.switcher != prefs.hotkey.switcher {
        rebind_switcher(&app, &previous.hotkey.switcher, &prefs.hotkey.switcher);
    }

    if previous.appearance.backdrop != prefs.appearance.backdrop
        || previous.appearance.tint_alpha != prefs.appearance.tint_alpha
    {
        if let Some(window) = app.get_webview_window("main") {
            summon::apply_backdrop(
                &window,
                prefs.appearance.backdrop,
                prefs.appearance.tint_alpha,
            );
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
pub(crate) async fn open_settings(app: AppHandle, section: Option<String>) -> Result<(), String> {
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

    let window =
        tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App(route.into()))
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

/// Sill's own settings, for the settings window's filter box.
///
/// Read from the same catalogue the launcher searches, so the two can never
/// disagree about what exists or which panel it is in.
#[tauri::command]
pub(crate) fn list_own_settings() -> Vec<settings_index::Setting> {
    settings_index::SETTINGS.to_vec()
}

/// Accelerators another application already owns.
///
/// Read by the settings window so a key that could not be bound says so, in
/// the row that set it, rather than working silently in the log and nowhere
/// else. Windows does not say which application took it, so neither does this.
#[tauri::command]
pub(crate) async fn hotkey_conflicts(app: AppHandle) -> Vec<String> {
    app.try_state::<crate::HotkeyConflicts>()
        .map(|conflicts| conflicts.all())
        .unwrap_or_default()
}
