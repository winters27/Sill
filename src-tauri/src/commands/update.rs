//! What the two windows call to ask about a newer Sill.
//!
//! Thin on purpose. Every decision about when to ask, what counts as news and
//! what to hold on to lives in [`crate::update`], so that the launcher and the
//! settings window cannot answer the question differently. These are the doors
//! into it.

use tauri::AppHandle;

use crate::update::{self, UpdateState};

/// The current state, for a window that has just opened.
///
/// Never asks anything. A window drawing itself must not be the thing that
/// opens a socket, or every summon would wait on the network before the list
/// appeared.
#[tauri::command]
pub(crate) fn update_state(app: AppHandle) -> UpdateState {
    update::state(&app)
}

/// Asks whether there is a newer Sill.
///
/// `force` is the button in settings, which means somebody is looking at the
/// answer and is entitled to a fresh one. Without it this is the summon path
/// and does nothing at all unless a day has passed.
#[tauri::command]
pub(crate) async fn check_for_update(app: AppHandle, force: bool) {
    update::check(app, force).await;
}

/// Downloads the newer Sill and runs its installer.
///
/// Returns an error the caller can show, but the state is announced either
/// way: the chin is watching `sill://update-changed` and would otherwise sit
/// on "downloading" after a failure that only the caller heard about.
#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> Result<(), String> {
    update::install(app).await
}

/// Closes Sill and starts it again, for an update already installed.
///
/// `AppHandle::restart` never returns, which is why this takes no result: a
/// caller waiting on it would wait for a process that is gone. The frontend
/// treats it as fire and forget for the same reason.
///
/// Reached only from the `ready` state. On Windows the installer usually takes
/// the process down itself, so this is the path for the case where it did not.
#[tauri::command]
pub(crate) fn restart_for_update(app: AppHandle) {
    app.restart();
}
