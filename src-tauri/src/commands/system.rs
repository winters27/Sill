//! Everything the launcher can do to itself or to the machine.

use crate::reload_index;

use tauri::{AppHandle, State};

use crate::registry::Frecency;
use crate::state::{data_dir, RegistryState};
use crate::{icons, log, summon};

/// Rescans every enabled source.
///
/// Returns as soon as the scan is queued rather than waiting for it: the
/// launcher keeps answering from the old index and re-queries when
/// `sill://registry-updated` lands.
#[tauri::command]
pub(crate) fn rebuild_index(app: AppHandle) {
    reload_index(&app);
}

/// Opens the log in whatever reads a text file.
#[tauri::command]
pub(crate) fn open_log() -> Result<(), String> {
    let path = log::path().ok_or_else(|| "The log has not been opened".to_string())?;
    tauri_plugin_opener::open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Reveals the folder holding preferences, the index cache and the log.
#[tauri::command]
pub(crate) fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let dir = data_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    tauri_plugin_opener::open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// Forgets which entries have been launched, so ranking starts over.
#[tauri::command]
pub(crate) async fn clear_usage_history(registry: State<'_, RegistryState>) -> Result<(), String> {
    let mut guard = registry.inner.lock().await;
    guard.frecency = Frecency::default();
    let path = guard.frecency_path.clone();
    guard.frecency.save(&path).map_err(|e| e.to_string())
}

/// The icon for a launchable, as a data URI.
///
/// Requested lazily per row rather than resolved for the whole index: a
/// machine has hundreds of Start Menu entries and only a handful are ever on
/// screen. Results are cached, misses included.
#[tauri::command]
pub(crate) async fn app_icon(path: String) -> Option<String> {
    icons::icon_data_uri(&path)
}

/// Closes Sill entirely.
///
/// A launcher is normally dismissed rather than quit, so this is deliberately
/// only reachable from the menu: there is no accidental path to it.
#[tauri::command]
pub(crate) fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Puts the launcher away. Bound to Escape in the UI.
#[tauri::command]
pub(crate) fn dismiss(window: tauri::WebviewWindow) {
    summon::hide(&window);
}
