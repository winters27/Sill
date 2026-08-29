//! What the application holds while it runs.
//!
//! Gathered here rather than left at the top of `lib.rs` because every command
//! module needs them, and a state type defined next to the code that starts
//! the app reads as if only that code owns it.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::exthost::ExtHost;
use crate::preferences;
use crate::registry::{CommandRecord, Frecency};

/// Holds the extension host, if one is running.
///
/// It is a Node process, and starting it with the app cost 38 MB of resident
/// memory on every machine, including the overwhelming majority of sessions
/// where no extension is ever opened. So it starts on the first extension
/// launch and shuts itself down again once nothing has used it, which is the
/// same lifecycle `dictation::server` gives the whisper process.
///
/// The slot is an `Arc` so it can be cloned out of Tauri's state and moved
/// into an async task. Holding a `State<'_, _>` across an await would borrow
/// the app handle for the life of the task.
#[derive(Clone)]
pub(crate) struct HostState {
    pub(crate) inner: Arc<tokio::sync::Mutex<Option<Arc<ExtHost>>>>,
    /// Built once and reused across host restarts. It owns `LocalStorage`,
    /// which is a file on disk, and the event sender the window listens on.
    pub(crate) api: Arc<crate::exthost::ApiLayer>,
    pub(crate) host_js: Arc<PathBuf>,
    /// When the host was last asked for, which is what the watchdog measures.
    pub(crate) last_used: Arc<std::sync::Mutex<std::time::Instant>>,
}

/// The user's own preferences.
#[derive(Clone)]
pub(crate) struct PrefsState {
    pub(crate) inner: Arc<tokio::sync::Mutex<preferences::Preferences>>,
    pub(crate) path: Arc<PathBuf>,
}

/// The installed command registry and its ranking state.
#[derive(Clone)]
pub(crate) struct RegistryState {
    pub(crate) inner: Arc<tokio::sync::Mutex<Registry>>,
}

pub(crate) struct Registry {
    pub(crate) commands: Vec<CommandRecord>,
    /// Sill's own settings, shaped as commands.
    ///
    /// Built once at startup: the catalogue is a `const` and cannot change
    /// while the app runs, so rebuilding it per keystroke would be pure cost.
    pub(crate) own_settings: Vec<CommandRecord>,
    /// Snippets, shaped as commands.
    ///
    /// Held here rather than read per query: the previous version parsed
    /// `snippets.json` off disk on every keystroke, which is a filesystem
    /// round trip per character typed. Refreshed whenever a snippet changes.
    pub(crate) snippets: Vec<CommandRecord>,
    /// Quicklinks, shaped as commands. Held for the same reason as snippets.
    pub(crate) quicklinks: Vec<CommandRecord>,
    pub(crate) frecency: Frecency,
    pub(crate) frecency_path: PathBuf,
}

pub(crate) fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}
