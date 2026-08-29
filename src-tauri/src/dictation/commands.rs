//! The command layer for dictation.
//!
//! Thin wrappers over `dictation::service`. Everything with logic worth
//! testing lives behind these, not in them, and every one flattens the
//! module's error to the string the frontend shows.

use crate::dictation::assets::{self, WhisperModel};
use crate::dictation::capture::list_input_devices;
use crate::dictation::engine;
use crate::dictation::history;
use crate::dictation::models::{
    AudioInputDevice, DictationSettings, LocalSetupStatus, SetupProgress,
};
use crate::dictation::server::WhisperServer;
use crate::dictation::service::DictationService;
use tauri::{AppHandle, State};

/// Every microphone the user could choose in settings.
#[tauri::command]
pub fn list_audio_input_devices() -> Result<Vec<AudioInputDevice>, String> {
    Ok(list_input_devices()?)
}

/// Pushes the settings tab's state down to Rust and re-arms the trigger.
///
/// Called on startup and on every change, because the hook fires on a thread
/// with no route back to the frontend's store.
#[tauri::command]
pub fn set_dictation_settings(
    app: AppHandle,
    state: State<'_, DictationService>,
    settings: DictationSettings,
) -> Result<(), String> {
    let enabled = settings.enabled;
    crate::say!(
        "settings: enabled={} shortcut={:?}+{:?} provider={} device={:?}",
        enabled,
        settings.shortcut_modifier,
        settings.shortcut_key,
        settings.provider_id,
        settings.device_id,
    );

    // Validate before storing: a shortcut that cannot be parsed should be
    // reported to the settings tab, not silently accepted and then ignored.
    #[cfg(windows)]
    let chord = if enabled {
        {
            let mut chord = crate::dictation::hotkey::chord_from_shortcut(
                &settings.shortcut_modifier,
                &settings.shortcut_key,
            )
            .map_err(String::from)?;
            let (finish, cancel) =
                crate::dictation::hotkey::end_keys(&settings.finish_key, &settings.cancel_key);
            chord.finish = finish;
            chord.cancel = cancel;
            Some(chord)
        }
    } else {
        None
    };

    state.set_settings(settings);

    // Rebinding is a teardown plus a fresh install, so this is also what
    // applies a changed chord.
    #[cfg(windows)]
    match chord {
        Some(chord) => {
            crate::say!("arming hook for {chord:?}");
            state.enable_hotkey(&app, chord).map_err(String::from)?;
            crate::say!("hook installed");
        }
        None => {
            crate::say!("disabled; hook removed");
            state.disable_hotkey();
        }
    }

    #[cfg(not(windows))]
    {
        // The low-level hook is Windows-only for now; elsewhere the commands
        // below still drive dictation from the launcher.
        let _ = (&app, enabled);
    }

    Ok(())
}

#[tauri::command]
pub fn get_dictation_settings(state: State<'_, DictationService>) -> DictationSettings {
    state.settings()
}

/// Current panel status, for the panel route to recover on mount.
///
/// See `PanelState`: without this the first dictation after launch shows an
/// empty pill, because the status event outran the route's listener.
#[tauri::command]
pub fn get_dictation_panel_status(
    state: State<'_, crate::dictation::panel::PanelState>,
) -> Option<String> {
    state
        .0
        .lock()
        .ok()
        .and_then(|status| *status)
        .map(|status| match status {
            crate::dictation::panel::PanelStatus::Listening => "listening".to_string(),
            crate::dictation::panel::PanelStatus::Transcribing => "transcribing".to_string(),
            crate::dictation::panel::PanelStatus::Copied => "copied".to_string(),
            crate::dictation::panel::PanelStatus::Confirming => "confirming".to_string(),
        })
}

#[tauri::command]
pub fn is_dictation_listening(state: State<'_, DictationService>) -> bool {
    state.is_listening()
}

/// Starts a dictation without the hotkey, for the launcher command and for
/// platforms with no hook yet.
#[tauri::command]
pub fn start_dictation(app: AppHandle, state: State<'_, DictationService>) -> Result<(), String> {
    state.start(&app).map_err(String::from)
}

#[tauri::command]
pub fn confirm_dictation(app: AppHandle, state: State<'_, DictationService>) -> Result<(), String> {
    state.confirm(&app).map_err(String::from)
}

#[tauri::command]
pub fn cancel_dictation(app: AppHandle, state: State<'_, DictationService>) {
    state.cancel(&app);
}

/// Every whisper model offered, with whether it is already downloaded.
#[tauri::command]
pub fn list_whisper_models(app: AppHandle) -> Vec<WhisperModel> {
    assets::list(&app)
}

/// What local dictation still needs before it can run.
#[tauri::command]
pub fn get_local_dictation_status(
    app: AppHandle,
    whisper: State<'_, WhisperServer>,
    state: State<'_, DictationService>,
) -> LocalSetupStatus {
    let model_id = state.settings().model_id;
    let engine_installed = engine::is_installed(&app);
    // Asked once and reused: `snapshot` reaps a dead child, so calling it
    // twice could report a server that the first call just cleared.
    let snapshot = whisper.snapshot();
    let model_installed = assets::is_installed(&app, &model_id);

    LocalSetupStatus {
        engine_installed,
        model_installed,
        download_bytes: (if model_installed {
            0
        } else {
            assets::size_of(&model_id).unwrap_or(0)
        }) + if engine_installed {
            0
        } else {
            engine::ARCHIVE_BYTES
        },
        engine_version: engine::VERSION.to_string(),
        model_label: assets::label_of(&model_id).unwrap_or(&model_id).to_string(),
        model_memory_bytes: assets::memory_of(&model_id).unwrap_or(0),
        model_id,
        server: snapshot.clone(),
        server_running: snapshot.is_some(),
    }
}

/// Downloads whisper.cpp and the selected model, then starts the server.
///
/// Split into visible stages because the model download is hundreds of
/// megabytes and the first model load takes seconds: without progress this
/// looks like a button that does nothing.
#[tauri::command]
pub async fn install_local_dictation(
    app: AppHandle,
    whisper: State<'_, WhisperServer>,
    model_id: String,
) -> Result<(), String> {
    let outcome = install_local_inner(&app, &whisper, &model_id).await;
    match &outcome {
        Ok(()) => SetupProgress::Ready.emit(&app),
        Err(e) => SetupProgress::Failed {
            error: e.to_string(),
        }
        .emit(&app),
    }
    outcome.map_err(String::from)
}

async fn install_local_inner(
    app: &AppHandle,
    whisper: &WhisperServer,
    model_id: &str,
) -> crate::dictation::error::Result<()> {
    if !engine::is_installed(app) {
        SetupProgress::Engine.emit(app);
        engine::ensure(app, |downloaded, total| {
            SetupProgress::EngineDownload {
                bytes_downloaded: downloaded,
                total_bytes: total,
            }
            .emit(app);
        })
        .await?;
    }

    assets::ensure(app, model_id).await?;

    // Started here rather than on the first dictation, so the model load is
    // paid while the user is looking at a progress indicator instead of
    // while they are holding a hotkey down waiting to speak.
    SetupProgress::Starting.emit(app);
    whisper.ensure(app, model_id).await?;
    Ok(())
}

/// Deletes a downloaded model. Stops the server first when it is the one
/// being served, since the file is open and Windows will not delete it.
#[tauri::command]
pub fn remove_whisper_model(
    app: AppHandle,
    whisper: State<'_, WhisperServer>,
    state: State<'_, DictationService>,
    model_id: String,
) -> Result<bool, String> {
    if state.settings().model_id == model_id {
        whisper.stop();
    }
    assets::remove(&app, &model_id).map_err(String::from)
}

/// What the keyboard hook currently believes.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookState {
    /// `SetWindowsHookExW` returned a handle and it has not been released.
    ///
    /// Not "a chord was stored", which happens before the hook thread even
    /// spawns and so proves nothing about whether the hook exists.
    pub armed: bool,
    /// The hook thinks a dictation is running. Stuck true, the trigger is
    /// swallowed and does nothing.
    pub listening: bool,
    /// The hook thinks the trigger key is still down. Stuck true, every press
    /// reads as an auto-repeat and is ignored.
    pub trigger_held: bool,
    /// Whether the service agrees a recording is in progress.
    ///
    /// Disagreeing with `listening` is the signature of the stuck state.
    pub recording: bool,
    /// Key events the hook has been handed since it was installed.
    ///
    /// The unambiguous liveness signal. Climbing while you type means the
    /// hook is alive and the question is what it decides; stuck at zero means
    /// it is receiving no input at all.
    pub keys_seen: u64,
    /// Key events that arrived synthesised rather than typed. Acted on.
    pub injected_seen: u64,
    /// Key events ignored for being Sill's own.
    pub own_seen: u64,
    /// Presses of the trigger key, whatever else was held.
    ///
    /// Zero while `keys_seen` climbs means the key never reaches the hook at
    /// all, which is something outside this process rather than a bug in the
    /// matching.
    pub chord_key_seen: u64,
    /// Times the trigger was seen with the right modifiers held.
    ///
    /// Climbing while the trigger does nothing narrows the fault to what
    /// happens after the match, rather than to the match itself.
    pub triggers_seen: u64,
    /// Modifiers held at the last trigger-key press, which is what says why
    /// a press that arrived did not match.
    pub last_modifiers: Option<String>,
}

#[tauri::command]
pub fn dictation_hook_state(state: State<'_, DictationService>) -> HookState {
    #[cfg(windows)]
    let facts = crate::dictation::hotkey::HotkeyListener::state();
    #[cfg(not(windows))]
    let facts = crate::dictation::hotkey::HookFacts {
        installed: false,
        listening: false,
        trigger_held: false,
        keys_seen: 0,
        injected_seen: 0,
        own_seen: 0,
        chord_key_seen: 0,
        triggers_seen: 0,
        last_modifiers: None,
    };

    HookState {
        armed: facts.installed,
        listening: facts.listening,
        trigger_held: facts.trigger_held,
        recording: state.is_listening(),
        keys_seen: facts.keys_seen,
        injected_seen: facts.injected_seen,
        own_seen: facts.own_seen,
        chord_key_seen: facts.chord_key_seen,
        triggers_seen: facts.triggers_seen,
        last_modifiers: facts.last_modifiers,
    }
}

/// Puts the hook back to idle, for when it has got stuck.
#[tauri::command]
pub fn reset_dictation_hook(app: AppHandle, state: State<'_, DictationService>) {
    #[cfg(windows)]
    crate::dictation::hotkey::HotkeyListener::reset_state();

    state.cancel(&app);
    crate::say!("dictation hook reset by hand");
}

/// Every finished dictation, newest first.
#[tauri::command]
pub fn dictation_history(app: AppHandle) -> Vec<history::Entry> {
    history::load(&app)
}

/// Counted totals for one window of history.
#[tauri::command]
pub fn dictation_stats(app: AppHandle, range: history::Range) -> history::Stats {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    history::stats(&history::load(&app), range, now)
}

/// The most recent transcript, for the launcher command of the same name.
#[tauri::command]
pub fn last_transcription(app: AppHandle) -> Option<history::Entry> {
    history::last(&app)
}

/// Deletes one entry. Returns whether anything matched.
#[tauri::command]
pub fn forget_transcription(app: AppHandle, at: i64) -> Result<bool, String> {
    history::remove(&app, at).map_err(String::from)
}

/// Deletes the whole history. Returns how many entries went.
#[tauri::command]
pub fn clear_dictation_history(app: AppHandle) -> Result<usize, String> {
    history::clear(&app).map_err(String::from)
}

/// Stops the local server, releasing the model's memory.
#[tauri::command]
pub fn stop_whisper_server(whisper: State<'_, WhisperServer>) {
    whisper.stop();
}
