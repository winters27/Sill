//! Types crossing the dictation IPC boundary, plus the plain audio-format
//! descriptions the pure capture logic reasons about.

use crate::dictation::provider::ProviderConfig;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// A microphone the user can pick in settings.
///
/// `id` is cpal's stable `DeviceId` rendered through `Display`; cpal
/// documents that as the form applications should persist, and it round-trips
/// back to a device via `FromStr` plus `host.device_by_id`. Names are for
/// humans only: two identical headsets report the same name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// A concrete input format to open a stream with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

/// One entry of what a device advertises, flattened out of cpal's
/// `SupportedStreamConfigRange` so the selection logic stays a pure function
/// over plain numbers and can be tested without a sound card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedRange {
    pub channels: u16,
    pub min_rate: u32,
    pub max_rate: u32,
}

impl SupportedRange {
    /// The rate in this range closest to what dictation wants.
    ///
    /// Exactly 16 kHz when the range covers it, otherwise the nearest edge:
    /// downsampling discards detail the model cannot use anyway, while
    /// upsampling invents nothing, so a rate above the target beats one
    /// below it.
    pub fn best_rate_for(&self, target: u32) -> u32 {
        if self.min_rate <= target && target <= self.max_rate {
            target
        } else if self.min_rate > target {
            self.min_rate
        } else {
            self.max_rate
        }
    }
}

/// Everything the dictation trigger needs, cached in Rust because the hook
/// fires on a thread with no access to the frontend's settings store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DictationSettings {
    pub enabled: bool,
    /// The trigger, in the `{modifier, key}` form the app's shortcut recorder
    /// produces (e.g. `"Control+Alt"` and `"D"`). Kept as the recorder's own
    /// strings rather than a virtual-key code so the settings tab can bind
    /// straight to `ShortcutRecorder`; `hotkey::chord_from_shortcut` converts
    /// and validates on the way in.
    pub shortcut_modifier: String,
    pub shortcut_key: String,
    /// Which arm of `providers::transcription_request` handles this.
    pub provider_id: String,
    /// Which whisper model the local server runs. Ignored by every other
    /// provider, which serve whatever model their endpoint is configured
    /// with.
    pub model_id: String,
    /// Where to send the audio, and what to authenticate with.
    pub provider: ProviderConfig,
    /// cpal device id. `None` follows the system default microphone.
    pub device_id: Option<String>,
    /// ISO language code. `None` lets the model auto-detect.
    pub language: Option<String>,
    /// What happens to the finished transcript.
    pub output_mode: OutputMode,
    /// Follow whatever Windows Sound settings call the default input.
    ///
    /// When off, `device_priority` decides instead: the first entry that is
    /// actually present is used. That is what makes a headset take over the
    /// moment it is plugged in and hand back when it is not.
    pub use_system_microphone: bool,
    /// Preferred devices, best first, used when `use_system_microphone` is off.
    pub device_priority: Vec<String>,
    /// Mute everything else while recording.
    ///
    /// Music playing through speakers is picked up by the microphone and
    /// transcribed as words, which is the single most common way a dictation
    /// comes back with something nobody said.
    pub mute_while_recording: bool,
    /// Which key finishes a dictation, in the recorder's own key naming.
    pub finish_key: String,
    /// Which key throws one away.
    pub cancel_key: String,
    /// Ask before discarding a recording.
    pub confirm_cancel: bool,
    /// Standing guidance sent as the head of every transcription prompt.
    pub custom_instructions: String,
    /// Name the frontmost application in the prompt, and file the transcript
    /// against it in the history.
    pub app_context: bool,
    /// Keep finished transcripts.
    pub keep_history: bool,
    /// Play the bundled cues when a dictation starts and stops.
    pub sound_enabled: bool,
    /// Words and names the model should get right: proper nouns, jargon,
    /// anything it reliably mangles. Sent as the transcription prompt, which
    /// is what every Whisper-compatible endpoint uses to bias decoding.
    pub vocabulary: String,
}

/// Where a finished transcript goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Put it on the clipboard and synthesise a paste into whatever has
    /// focus. The text appearing is its own confirmation.
    #[default]
    Paste,
    /// Put it on the clipboard only. Nothing is injected, so this is the safe
    /// choice for password fields, terminals, and anywhere a stray paste
    /// would be destructive. The panel says so, since otherwise nothing
    /// visible happens at all.
    Clipboard,
    /// Neither. The transcript goes to the history and nowhere else, which is
    /// what "dictate a note" means.
    None,
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            shortcut_modifier: "Control+Alt".to_string(),
            shortcut_key: "D".to_string(),
            // Local by default: no key to obtain and no audio leaves the
            // machine before the user has chosen otherwise.
            provider_id: "local".to_string(),
            model_id: crate::dictation::assets::DEFAULT_MODEL.to_string(),
            provider: ProviderConfig::default(),
            device_id: None,
            language: None,
            output_mode: OutputMode::Paste,
            use_system_microphone: true,
            device_priority: Vec::new(),
            // Off by default: muting the machine is a surprising thing for a
            // launcher to do without being asked.
            mute_while_recording: false,
            finish_key: "Enter".to_string(),
            cancel_key: "Escape".to_string(),
            confirm_cancel: false,
            custom_instructions: String::new(),
            app_context: true,
            keep_history: true,
            sound_enabled: true,
            vocabulary: String::new(),
        }
    }
}

/// Progress through "get local dictation working", emitted as
/// `dictation:setup`.
///
/// Deliberately not the runtimes installer's `runtime_download_progress`:
/// the frontend's shared `runtimeService` tracks that one globally, so
/// borrowing it would make the Runtimes page report an install that is not
/// happening. Serde conventions match it, though, so the two read the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "stage")]
pub enum SetupProgress {
    /// Fetching whisper.cpp itself.
    Engine,
    #[serde(rename_all = "camelCase")]
    EngineDownload {
        bytes_downloaded: u64,
        total_bytes: u64,
    },
    #[serde(rename_all = "camelCase")]
    Model {
        bytes_downloaded: u64,
        total_bytes: u64,
    },
    Verifying,
    /// Spawning the server and waiting for it to answer. The model is loaded
    /// during this stage, so it is slow enough to need saying.
    Starting,
    Ready,
    #[serde(rename_all = "camelCase")]
    Failed {
        error: String,
    },
}

impl SetupProgress {
    pub fn emit(&self, app: &AppHandle) {
        if let Err(e) = app.emit("dictation:setup", self) {
            crate::say!("[dictation] could not emit setup progress: {e}");
        }
    }
}

/// What local dictation still needs before it can run, for the settings tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSetupStatus {
    /// whisper.cpp is installed.
    pub engine_installed: bool,
    /// The selected model is downloaded.
    pub model_installed: bool,
    /// Which model that is.
    pub model_id: String,
    /// Bytes still to fetch, so the button can say what it will cost.
    pub download_bytes: u64,
    /// The server is up and answering.
    pub server_running: bool,
    /// Live details while it is, so the panel can show more than a word.
    pub server: Option<crate::dictation::server::ServerSnapshot>,
    /// Which whisper.cpp build is installed, or would be.
    pub engine_version: String,
    /// The selected model's display name, so the panel need not look it up.
    pub model_label: String,
    /// Roughly how much memory the selected model holds while resident.
    ///
    /// Measured per model rather than derived from the file size: whisper
    /// allocates compute buffers well beyond the weights, so a 465 MB file
    /// sits at about 649 MB resident.
    pub model_memory_bytes: u64,
}
