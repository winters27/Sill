//! Orchestrates one dictation: hook action in, pasted text out.
//!
//! The pieces either side of this are individually tested; what lives here is
//! sequencing and lifetime. It holds at most one recording at a time and is
//! deliberately idempotent at both ends, because the hook can deliver a
//! second `Start` or a stray `Confirm` faster than a recording can wind down.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::dictation::capture::{is_silent, CaptureSession};
use crate::dictation::models::{DictationSettings, OutputMode};
use crate::dictation::panel::{self, PanelStatus};
use crate::dictation::providers::{transcription_request, TranscribeOptions};
use crate::dictation::sound;
use crate::dictation::transcriber::{build_transcription_client, transcribe};
use crate::dictation::{resample, wav};
use crate::dictation::error::{DictationError, Result};

/// How often the panel's waveform advances. 33 ms is about 30 Hz, which is
/// fast enough that the bar being drawn is the sound currently being made,
/// and matches the CSS transition on the bars.
const METER_INTERVAL: Duration = Duration::from_millis(33);

/// Pause between putting text on the clipboard and synthesising the paste.
///
/// Writing and immediately pasting races the target application's read of the
/// clipboard, which loses the transcript and pastes whatever was there
/// before.
const PASTE_SETTLE: Duration = Duration::from_millis(50);

/// How long the panel says "Copied" before it goes away. Long enough to read,
/// short enough not to sit over the window the text is headed for.
const COPIED_DWELL: Duration = Duration::from_millis(1200);

/// A recording in progress, plus the handle that stops its meter pump.
struct ActiveRecording {
    session: CaptureSession,
    meter_running: Arc<AtomicBool>,
    /// The frontmost application when recording began.
    ///
    /// Captured here rather than at delivery because by then the transcript
    /// may have been pasted, the target may have changed, and the answer
    /// would be about the wrong window.
    context: Option<String>,
    /// Set when this recording muted the machine, so only a recording that
    /// muted it unmutes it.
    muted: bool,
}

/// Owns the at-most-one in-flight dictation and the settings the hook needs.
///
/// Settings are cached here rather than read from the store per dictation
/// because the trigger arrives on the hook thread, which has no access to the
/// frontend and must not block. The settings tab pushes changes down through
/// `set_dictation_settings`.
#[derive(Default)]
pub struct DictationService {
    active: Mutex<Option<ActiveRecording>>,
    settings: Mutex<DictationSettings>,
    /// Set while a hook listener thread is running. Flipping it stops that
    /// thread, which drops the listener and uninstalls the hook.
    hotkey_stop: Mutex<Option<Arc<AtomicBool>>>,
    /// Set by the first cancel when `confirm_cancel` is on, so the second one
    /// goes through. Cleared whenever a recording starts or ends.
    cancel_armed: AtomicBool,
}

impl DictationService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn settings(&self) -> DictationSettings {
        self.settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    pub fn set_settings(&self, settings: DictationSettings) {
        if let Ok(mut current) = self.settings.lock() {
            *current = settings;
        }
    }

    pub fn is_listening(&self) -> bool {
        self.active
            .lock()
            .map(|active| active.is_some())
            .unwrap_or(false)
    }

    /// Opens the microphone and raises the panel.
    ///
    /// A second `Start` while one is already running is ignored rather than
    /// restarting: the hook guards against auto-repeat, but a deep link or a
    /// launcher command could still arrive mid-recording.
    pub fn start(&self, app: &AppHandle) -> Result<()> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| DictationError::Other("Dictation state was poisoned".to_string()))?;
        if active.is_some() {
            return Ok(());
        }

        let settings = self.settings();
        // Read before the panel appears, while the window the transcript is
        // headed for is still frontmost.
        let context = settings
            .app_context
            .then(crate::dictation::context::foreground_app)
            .flatten();

        let session = CaptureSession::start(microphone(&settings).as_deref())?;

        // Muted before the start cue, or the cue mutes itself.
        let muted = settings.mute_while_recording && crate::dictation::audio::mute(true);

        let meter_running = Arc::new(AtomicBool::new(true));
        spawn_meter_pump(app.clone(), &session, Arc::clone(&meter_running));

        if settings.sound_enabled && !muted {
            crate::dictation::sound::play(app, sound::Cue::Start);
        }
        panel::show(app, PanelStatus::Listening)?;
        self.cancel_armed.store(false, Ordering::SeqCst);
        *active = Some(ActiveRecording {
            session,
            meter_running,
            context,
            muted,
        });
        Ok(())
    }

    /// Stops recording, transcribes, and pastes.
    ///
    /// Returns as soon as the audio is in hand; transcription continues on a
    /// background task so the hook thread is never waiting on the network.
    pub fn confirm(&self, app: &AppHandle) -> Result<()> {
        let Some(recording) = self.take_active()? else {
            return Ok(());
        };
        self.cancel_armed.store(false, Ordering::SeqCst);
        let settings = self.settings();

        recording.meter_running.store(false, Ordering::SeqCst);
        if recording.muted {
            crate::dictation::audio::mute(false);
        }
        if settings.sound_enabled {
            crate::dictation::sound::play(app, sound::Cue::Stop);
        }
        let context = recording.context.clone();
        let clip = recording.session.stop();

        // Nothing was said, or the microphone is blocked. Either way pasting
        // an empty string would look like a bug, and calling a paid API for
        // silence is worse.
        if is_silent(&clip.samples) {
            crate::say!("discarded a silent recording");
            let _ = panel::hide(app);
            return Ok(());
        }

        crate::say!("captured {} samples at {} Hz; transcribing via {}",
            clip.samples.len(),
            clip.sample_rate,
            settings.provider_id
        );
        panel::show(app, PanelStatus::Transcribing)?;

        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            crate::say!("transcription task started");
            let outcome = transcribe_and_deliver(&app, clip, settings, context).await;
            let _ = panel::hide(&app);
            match &outcome {
                Ok(()) => crate::say!("transcription task finished"),
                Err(e) => crate::say!("transcription task failed: {e}"),
            }
            if let Err(e) = outcome {
                report(&app, &format!("Dictation failed: {e}"));
            }
        });

        Ok(())
    }

    /// Stops recording and throws the audio away.
    /// Discards the recording, or asks first when the setting says to.
    ///
    /// Two presses rather than a dialog: a dialog would need focus, and
    /// taking focus mid-dictation is exactly what the panel exists to avoid.
    pub fn cancel(&self, app: &AppHandle) {
        if self.settings().confirm_cancel
            && self.is_listening()
            && !self.cancel_armed.swap(true, Ordering::SeqCst)
        {
            let _ = panel::show(app, PanelStatus::Confirming);
            return;
        }

        self.cancel_armed.store(false, Ordering::SeqCst);
        if let Ok(Some(recording)) = self.take_active() {
            recording.meter_running.store(false, Ordering::SeqCst);
            if recording.muted {
                crate::dictation::audio::mute(false);
            }
            if self.settings().sound_enabled {
                sound::play(app, sound::Cue::Stop);
            }
            // Dropping the session ends the stream; the samples go with it.
            drop(recording.session);
        }
        let _ = panel::hide(app);
    }

    /// Installs the keyboard hook and starts consuming its actions.
    ///
    /// Idempotent: an existing listener is torn down first, which is what
    /// makes this safe to call every time settings change.
    #[cfg(windows)]
    pub fn enable_hotkey(
        &self,
        app: &AppHandle,
        chord: crate::dictation::hotkey::Chord,
    ) -> Result<()> {
        use crate::dictation::hotkey::{Action, HotkeyListener};
        use std::sync::mpsc::RecvTimeoutError;

        self.disable_hotkey();

        let listener = HotkeyListener::start(chord)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let app = app.clone();

        std::thread::Builder::new()
            .name("dictation-actions".to_string())
            .spawn(move || {
                // Owned here so that leaving this loop drops it and unhooks.
                let listener = listener;
                while !thread_stop.load(Ordering::SeqCst) {
                    let action = match listener.actions.recv_timeout(Duration::from_millis(200)) {
                        Ok(action) => action,
                        // A timeout is just the idle case; it exists so the
                        // stop flag is checked regularly.
                        Err(RecvTimeoutError::Timeout) => continue,
                        Err(RecvTimeoutError::Disconnected) => break,
                    };

                    crate::say!("action {action:?}");
                    let service = app.state::<DictationService>();
                    let outcome = match action {
                        Action::Start => service.start(&app),
                        Action::Confirm => service.confirm(&app),
                        Action::Cancel => {
                            service.cancel(&app);
                            Ok(())
                        }
                    };
                    if let Err(e) = outcome {
                        crate::say!("{action:?} failed: {e}");
                        // The hook believes a dictation is running; without
                        // this it would keep swallowing Enter and Esc.
                        HotkeyListener::clear_listening();
                        let _ = panel::hide(&app);
                    }
                }
            })
            .map_err(|e| DictationError::Other(format!("Could not start the action thread: {e}")))?;

        // A heartbeat, because the counters are the only proof the hook is
        // alive and the settings window is a bad place to read them from: it
        // has to be open, and whatever is wrong may be the reason it cannot
        // be. Written only when something moved, so an idle machine adds
        // nothing to the log.
        let beat_stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("dictation-hook-beat".to_string())
            .spawn(move || {
                let mut last = (0u64, 0u64, 0u64, 0u64, 0u64);
                while !beat_stop.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_secs(2));
                    let f = HotkeyListener::state();
                    let now = (
                        f.own_seen,
                        f.injected_seen,
                        f.keys_seen,
                        f.chord_key_seen,
                        f.triggers_seen,
                    );
                    if now == last {
                        continue;
                    }
                    last = now;
                    crate::say!(
                        "hook: installed={} listening={} held={} own={} injected={} keys={} chordkey={} triggers={} lastmods={}",
                        f.installed,
                        f.listening,
                        f.trigger_held,
                        f.own_seen,
                        f.injected_seen,
                        f.keys_seen,
                        f.chord_key_seen,
                        f.triggers_seen,
                        f.last_modifiers.as_deref().unwrap_or("-")
                    );
                }
            })
            .map_err(|e| DictationError::Other(format!("Could not start the hook heartbeat: {e}")))?;

        if let Ok(mut slot) = self.hotkey_stop.lock() {
            *slot = Some(stop);
        }
        Ok(())
    }

    /// Stops the hook listener, if one is running.
    pub fn disable_hotkey(&self) {
        if let Ok(mut slot) = self.hotkey_stop.lock() {
            if let Some(stop) = slot.take() {
                stop.store(true, Ordering::SeqCst);
            }
        }
    }

    fn take_active(&self) -> Result<Option<ActiveRecording>> {
        self.active
            .lock()
            .map(|mut active| active.take())
            .map_err(|_| DictationError::Other("Dictation state was poisoned".to_string()))
    }
}

/// Puts a message in front of the user.
///
/// Dictation runs while the launcher is hidden, so a failure has nowhere of
/// its own to appear. The event reaches the launcher when it is open, and the
/// log always has it.
fn report(app: &AppHandle, message: &str) {
    crate::say!("{message}");
    let _ = app.emit("dictation:message", message);
}

/// Emits band energies to the panel until the recording ends.
///
/// The transform runs here rather than on the audio callback: the callback
/// must never do work that could outlast a buffer, and the waveform only
/// needs a frame every `METER_INTERVAL` regardless of how often audio
/// arrives.
fn spawn_meter_pump(app: AppHandle, session: &CaptureSession, running: Arc<AtomicBool>) {
    let sample_rate = session.format().sample_rate;
    let bands = crate::dictation::bands::Bands::new(sample_rate);
    let window = session.window_handle(bands.window_len());
    std::thread::Builder::new()
        .name("dictation-meter".to_string())
        .spawn(move || {
            let mut bands = bands;
            let mut out = [0.0f32; crate::dictation::bands::COUNT];
            while running.load(Ordering::SeqCst) {
                let samples = window();
                if samples.is_empty() {
                    out.fill(0.0);
                } else {
                    bands.compute(&samples, &mut out);
                }
                panel::emit_bands(&app, &out);
                std::thread::sleep(METER_INTERVAL);
            }
        })
        .ok();
}

/// The provider config to transcribe with, starting the bundled whisper
/// server first when that is what is selected.
///
/// The server picks an ephemeral port, so its base URL is only knowable at
/// run time and cannot be stored in settings. A base URL the user typed in
/// themselves still wins: that is how a whisper server on another machine is
/// pointed at, and starting a local one alongside it would be pure waste.
async fn local_provider_config(
    app: &AppHandle,
    settings: &DictationSettings,
) -> Result<crate::dictation::provider::ProviderConfig> {
    let configured = settings
        .provider
        .base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|base| !base.is_empty());
    if settings.provider_id != "local" || configured {
        return Ok(settings.provider.clone());
    }

    let server = app.state::<crate::dictation::server::WhisperServer>();
    let base = server.ensure(app, &settings.model_id).await?;

    Ok(crate::dictation::provider::ProviderConfig {
        base_url: Some(base),
        ..settings.provider.clone()
    })
}

async fn transcribe_and_deliver(
    app: &AppHandle,
    clip: crate::dictation::capture::CapturedClip,
    settings: DictationSettings,
    context: Option<String>,
) -> Result<()> {
    // Measured before resampling, from the sample count rather than a wall
    // clock: the clock would include however long the panel took to appear.
    let spoken_ms = (clip.samples.len() as u64 * 1_000) / clip.sample_rate.max(1) as u64;

    let samples = resample::to_target_rate(&clip.samples, clip.sample_rate)?;
    let bytes = wav::encode_mono_16bit(&samples, resample::TARGET_RATE);

    let provider = local_provider_config(app, &settings).await?;
    let request = transcription_request(
        &settings.provider_id,
        &provider,
        &TranscribeOptions {
            language: settings.language.clone(),
            // Whisper biases decoding toward whatever the prompt contains,
            // which is what makes standing instructions, the frontmost
            // application and a vocabulary list all useful here.
            prompt: crate::dictation::context::build_prompt(
                &settings.custom_instructions,
                context.as_deref(),
                &settings.vocabulary,
            ),
            ..Default::default()
        },
    )?;

    crate::say!("POST {}", request.url);
    let started = std::time::Instant::now();
    let transcript = transcribe(&build_transcription_client(), &request, bytes).await?;
    let transcribe_ms = started.elapsed().as_millis() as u64;
    crate::say!("transcript: {} chars in {transcribe_ms} ms", transcript.len());

    if transcript.is_empty() {
        return Ok(());
    }

    // Recorded before delivery, not after: the paste is the step most likely
    // to fail (an elevated target refuses synthetic input), and losing the
    // transcript as well as the paste would be the worse of the two.
    if settings.keep_history {
        let entry = crate::dictation::history::Entry {
            at: now_seconds(),
            words: crate::dictation::history::count_words(&transcript),
            text: transcript.clone(),
            spoken_ms,
            transcribe_ms,
            provider: settings.provider_id.clone(),
            model: if settings.provider_id == "local" {
                settings.model_id.clone()
            } else {
                String::new()
            },
            app: context,
        };
        if let Err(err) = crate::dictation::history::record(app, &entry) {
            crate::say!("could not record the transcript: {err}");
        }
        let _ = app.emit("dictation:recorded", &entry);
    }

    match settings.output_mode {
        OutputMode::Paste => {
            paste(app, &transcript)?;
            crate::say!("pasted");
        }
        OutputMode::Clipboard => {
            copy(app, &transcript)?;
            crate::say!("copied to clipboard");
            // Without this nothing visible happens and a dictation that
            // worked looks exactly like one that failed.
            let _ = panel::show(app, PanelStatus::Copied);
            std::thread::sleep(COPIED_DWELL);
            let _ = panel::hide(app);
        }
        OutputMode::None => {
            crate::say!("kept in history only");
            let _ = panel::show(app, PanelStatus::Copied);
            std::thread::sleep(COPIED_DWELL);
            let _ = panel::hide(app);
        }
    }
    Ok(())
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Which microphone to open, given the settings.
///
/// `None` follows the system default. Otherwise the first entry in the
/// priority list that is actually present wins, which is what makes a headset
/// take over the moment it is plugged in and hand back when it is not.
fn microphone(settings: &DictationSettings) -> Option<String> {
    if settings.use_system_microphone {
        return None;
    }

    let present = crate::dictation::capture::list_input_devices().unwrap_or_default();
    settings
        .device_priority
        .iter()
        .find(|wanted| present.iter().any(|device| &&device.id == wanted))
        .cloned()
        // Every preferred device is unplugged, so the system default is a
        // better answer than refusing to record.
        .or(None)
}

/// Puts `text` on the clipboard.
fn copy(app: &AppHandle, text: &str) -> Result<()> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    app.clipboard()
        .write_text(text.to_string())
        .map_err(|e| DictationError::Other(format!("Could not write the transcript: {e}")))
}

/// Copies `text` and synthesises the paste chord.
///
/// The panel window is declared `focus: false` and `skipTaskbar`, so showing
/// it never moves focus and whatever the user was typing into is still
/// frontmost here.
fn paste(app: &AppHandle, text: &str) -> Result<()> {
    copy(app, text)?;
    std::thread::sleep(PASTE_SETTLE);
    crate::dictation::paste::chord();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_service_is_not_listening() {
        assert!(!DictationService::new().is_listening());
    }

    #[test]
    fn settings_round_trip() {
        let service = DictationService::new();
        let settings = DictationSettings {
            provider_id: "groq".to_string(),
            language: Some("en".to_string()),
            ..Default::default()
        };

        service.set_settings(settings);

        assert_eq!(service.settings().provider_id, "groq");
        assert_eq!(service.settings().language.as_deref(), Some("en"));
    }

    #[test]
    fn confirming_when_nothing_is_recording_is_harmless() {
        // The hook only sends Confirm while it believes a dictation is live,
        // but its view can lag a recording that ended some other way.
        assert!(DictationService::new().take_active().unwrap().is_none());
    }
}
