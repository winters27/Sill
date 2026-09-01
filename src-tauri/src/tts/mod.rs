//! Reading text out loud, in whichever voice has been set up.
//!
//! P2.4. The same text the transforms act on, spoken instead of rewritten, so
//! a clipboard row and a selection are the same thing to a voice as they are
//! to a rewrite.
//!
//! ## Three ways to say something, and only one of them is any good
//!
//! [`sapi`] is what Windows already has. It needs nothing installed and it
//! sounds like 2010, because the voices it can reach are from then: the neural
//! ones Windows 11 ships are registered where only Narrator and Edge can read
//! them. It is the fallback, not the choice.
//!
//! [`http`] is anything speaking OpenAI's `/v1/audio/speech`, which is one
//! request shape and a great many services: OpenAI itself, and every local
//! server that copied it. One adapter reaches a paid cloud voice and a model
//! running on this machine, which is the same reason the chat window has one
//! HTTP adapter rather than six.
//!
//! [`piper`] is a neural voice Sill fetches and runs itself, for somebody who
//! wants a good voice with no key and no server. Nothing is downloaded until
//! it is asked for.
//!
//! ## Why everything is asked for as a WAV
//!
//! Playback is `PlaySoundW`, which Sill already used for the dictation cues.
//! It takes a file, plays it without blocking, and **stops whatever it was
//! playing when it is handed something new**, which is exactly the interrupting
//! behaviour a launcher wants and means "stop" is a call with no file in it.
//! It reads WAV and nothing else, so every provider is asked for WAV, and the
//! alternative was an audio decoding stack for a feature that reads out a
//! paragraph.
//!
//! The whole clip is fetched before any of it plays. A sentence is immediate; a
//! long article waits a beat. Streaming would mean decoding chunks into an
//! output stream, which is a different and much larger piece of machinery.

pub mod http;
pub mod piper;
pub mod sapi;

use serde::{Deserialize, Serialize};

use crate::dictation::provider::ProviderConfig;

/// Which voice says it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Whatever Windows has. Always available, never good.
    #[default]
    System,
    /// Anything speaking OpenAI's `/v1/audio/speech`.
    Http,
    /// The neural voice Sill downloads and runs itself.
    Piper,
}

/// How text is read aloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TtsSettings {
    pub engine: Engine,
    /// Where to send the text, and what to authenticate with.
    ///
    /// The same type dictation uses, so a key here is sealed by the same code
    /// and shown by the same field. **Its path is named in `SEALED`**; a
    /// provider config that is not is written to the file in plain text.
    pub provider: ProviderConfig,
    /// Which voice the HTTP provider should use, in that provider's naming.
    pub voice: String,
    /// Which downloaded Piper voice to speak with.
    pub piper_voice: String,
}

impl Default for TtsSettings {
    fn default() -> Self {
        Self {
            engine: Engine::System,
            provider: ProviderConfig::default(),
            // OpenAI's own default, and the name Kokoro and the other
            // compatible servers all answer to as well.
            voice: "alloy".to_string(),
            piper_voice: piper::DEFAULT_VOICE.to_string(),
        }
    }
}

/// Says something, in whichever voice is set up.
///
/// The dispatch is here rather than in the caller so the action, the binding
/// and anything else that ever wants a voice all get the same answer about
/// which one is in use.
pub async fn aloud(app: &tauri::AppHandle, text: &str) -> Result<(), String> {
    let text = text.trim();

    if text.is_empty() {
        return Err("There is nothing to read".to_string());
    }

    let settings = settings_of(app).await;

    match settings.engine {
        Engine::System => {
            use tauri::Manager;
            app.state::<sapi::Sapi>().aloud(text)
        }
        Engine::Http => {
            let wav = http::speak(&settings, text).await?;
            play_bytes(app, &wav)
        }
        Engine::Piper => {
            let wav = piper::speak(app, &settings.piper_voice, text).await?;
            play_bytes(app, &wav)
        }
    }
}

/// Stops mid-sentence, whichever voice is talking.
///
/// Both halves, every time, and deliberately: the engine can be changed while
/// something is still being said, and a stop that only reached the current
/// setting would leave the previous one talking with no way to quiet it.
pub fn stop(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let said = app.state::<sapi::Sapi>().stop();
    stop_playback();
    said
}

/// What is set up, read the way every other async path reads preferences.
///
/// `.lock().await`, not `blocking_lock`. Tokio's blocking lock **panics when
/// it is called from a runtime thread**, and every caller here is an async
/// command, so the first version of this took the whole feature down rather
/// than only the button that happened to find it.
async fn settings_of(app: &tauri::AppHandle) -> TtsSettings {
    use tauri::Manager;

    let Some(prefs) = app.try_state::<crate::state::PrefsState>() else {
        return TtsSettings::default();
    };

    let settings = prefs.inner.lock().await.tts.clone();
    settings
}

/// Where a fetched clip is written before it is played.
///
/// One file, overwritten. `PlaySoundW` holds the file open while it plays, so
/// the previous sound is stopped before the next is written rather than
/// writing over something Windows is reading.
fn spoken_path(app: &tauri::AppHandle) -> std::path::PathBuf {
    crate::state::data_dir(app).join("spoken.wav")
}

/// Writes a clip and plays it, replacing whatever was playing.
pub fn play_bytes(app: &tauri::AppHandle, wav: &[u8]) -> Result<(), String> {
    if !looks_like_wav(wav) {
        // Worth naming rather than playing silence. Every provider here is
        // asked for WAV, so anything else means the request was answered by
        // something that ignored the format, and the bytes are usually an
        // error page.
        return Err("the voice answered with something that is not a WAV".to_string());
    }

    let path = spoken_path(app);

    stop_playback();
    std::fs::write(&path, wav).map_err(|err| format!("could not write the clip: {err}"))?;
    play_file(&path);

    Ok(())
}

/// Whether these bytes begin the way a RIFF/WAVE file does.
///
/// Cheap and worth it: an endpoint that refuses a request answers with JSON or
/// HTML and a 200 is not guaranteed, so without this the failure is a moment
/// of silence rather than a message.
pub fn looks_like_wav(bytes: &[u8]) -> bool {
    bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

#[cfg(windows)]
fn play_file(path: &std::path::Path) {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT};

    let wide = HSTRING::from(path.as_os_str());

    // SAFETY: the string outlives the call, which returns immediately because
    // of SND_ASYNC.
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(wide.as_ptr()),
            None,
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

#[cfg(windows)]
fn stop_playback() {
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE};

    // A null name is how PlaySound is told to stop, rather than to play
    // nothing.
    // SAFETY: no buffers are passed.
    unsafe {
        let _ = PlaySoundW(None, None, SND_PURGE | SND_ASYNC);
    }
}

#[cfg(not(windows))]
fn play_file(_path: &std::path::Path) {}

#[cfg(not(windows))]
fn stop_playback() {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal RIFF/WAVE header, which is all `looks_like_wav` reads.
    fn wav_header() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes
    }

    #[test]
    fn a_wav_is_recognised_and_an_error_page_is_not() {
        assert!(looks_like_wav(&wav_header()));

        // What an endpoint that refused actually answers with.
        assert!(!looks_like_wav(br#"{"error":{"message":"Incorrect API key"}}"#));
        assert!(!looks_like_wav(b"<!DOCTYPE html><html>502 Bad Gateway"));
        // An MP3, which is what a provider returns when the format was ignored.
        assert!(!looks_like_wav(&[0xFF, 0xFB, 0x90, 0x44, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!looks_like_wav(b""));
    }

    /// The default has to be the one that needs no setting up.
    #[test]
    fn nothing_configured_still_speaks() {
        assert_eq!(TtsSettings::default().engine, Engine::System);
    }
}
