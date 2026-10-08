//! Parakeet, run inside Sill rather than beside it.
//!
//! The whisper models are served by a whisper.cpp process over HTTP. Parakeet
//! needs none of that: `transcribe-rs` runs it on ONNX Runtime, linked into
//! the exe, so a transcription is a function call on the samples already in
//! hand. No server to start, no port, no request timeout to outlast.
//!
//! ## What it costs
//!
//! Nothing until the first dictation that uses it. Then the model stays
//! loaded (about 720 MB private, measured, growing with the length of the clip
//! being transcribed) until it has been idle for `IDLE_TIMEOUT`, the same
//! half hour the whisper server keeps, so a working session pays the 1.3 s
//! load once and a machine left alone gets the memory back. The idle check is
//! one sleeping task per dictation, not a timer.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};
use transcribe_rs::onnx::Quantization;

use crate::dictation::assets;
use crate::dictation::error::{DictationError, Result};

/// How long a loaded model may sit unused before it is let go. Matches the
/// whisper server's.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

struct Loaded {
    id: String,
    model: ParakeetModel,
    last_used: Instant,
}

/// The loaded model, when there is one. Managed state.
#[derive(Default)]
pub struct Parakeet {
    held: Arc<Mutex<Option<Loaded>>>,
}

impl Parakeet {
    /// Transcribes 16 kHz mono samples with model `id`, loading it first if
    /// it is not the one held.
    pub async fn transcribe(&self, app: &AppHandle, id: &str, samples: Vec<f32>) -> Result<String> {
        let dir = installed(app, id)?;
        let held = Arc::clone(&self.held);
        let id = id.to_string();

        let text = tokio::task::spawn_blocking(move || -> Result<String> {
            let mut held = lock(&held)?;
            let loaded = load_into(&mut held, &id, &dir)?;
            let result = loaded
                .model
                .transcribe_with(&samples, &ParakeetParams::default())
                .map_err(|err| DictationError::Other(format!("Parakeet could not transcribe: {err}")))?;
            loaded.last_used = Instant::now();
            Ok(result.text.trim().to_string())
        })
        .await
        .map_err(|err| DictationError::Other(format!("Parakeet stopped: {err}")))??;

        self.release_when_idle();
        Ok(text)
    }

    /// Loads model `id` now, so the first dictation does not wait for it.
    pub async fn preload(&self, app: &AppHandle, id: &str) -> Result<()> {
        let dir = installed(app, id)?;
        let held = Arc::clone(&self.held);
        let id = id.to_string();

        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut held = lock(&held)?;
            load_into(&mut held, &id, &dir)?.last_used = Instant::now();
            Ok(())
        })
        .await
        .map_err(|err| DictationError::Other(format!("Parakeet stopped: {err}")))??;

        self.release_when_idle();
        Ok(())
    }

    /// Lets the model go. Before deleting its files, which Windows refuses
    /// while they are open.
    pub fn unload(&self) {
        if let Ok(mut held) = self.held.lock() {
            if held.take().is_some() {
                crate::say!("parakeet: unloaded");
            }
        }
    }

    /// Which model is loaded, if any.
    pub fn loaded(&self) -> Option<String> {
        self.held
            .lock()
            .ok()
            .and_then(|held| held.as_ref().map(|loaded| loaded.id.clone()))
    }

    fn release_when_idle(&self) {
        let held = Arc::clone(&self.held);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(IDLE_TIMEOUT).await;
            if let Ok(mut held) = held.lock() {
                let idle = held
                    .as_ref()
                    .is_some_and(|loaded| loaded.last_used.elapsed() >= IDLE_TIMEOUT);
                if idle {
                    *held = None;
                    crate::say!("parakeet: unloaded after {} idle minutes", IDLE_TIMEOUT.as_secs() / 60);
                }
            }
        });
    }
}

/// The model's directory, or why there is none to load.
fn installed(app: &AppHandle, id: &str) -> Result<std::path::PathBuf> {
    if !assets::is_installed(app, id) {
        return Err(DictationError::NotFound(
            "The Parakeet model is not downloaded. Install it in Settings, under Dictation."
                .to_string(),
        ));
    }
    assets::model_path(app, id)
}

fn lock(held: &Mutex<Option<Loaded>>) -> Result<std::sync::MutexGuard<'_, Option<Loaded>>> {
    held.lock()
        .map_err(|_| DictationError::Other("The Parakeet model's lock was poisoned".to_string()))
}

/// The loaded model for `id`, loading it (and dropping any other) first.
fn load_into<'a>(held: &'a mut Option<Loaded>, id: &str, dir: &Path) -> Result<&'a mut Loaded> {
    if held.as_ref().is_some_and(|loaded| loaded.id == id) {
        return Ok(held.as_mut().expect("checked above"));
    }

    // Dropped before the next one loads, or both are resident at once.
    *held = None;
    let started = Instant::now();
    let model = ParakeetModel::load(dir, &Quantization::Int8)
        .map_err(|err| DictationError::Other(format!("Parakeet would not load: {err}")))?;
    crate::say!("parakeet: loaded {id} in {} ms", started.elapsed().as_millis());

    Ok(held.insert(Loaded {
        id: id.to_string(),
        model,
        last_used: Instant::now(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loads the real model and transcribes a real clip, in this crate's own
    /// build. `SILL_PARAKEET_DIR` is a directory holding the four files and
    /// `SILL_PARAKEET_WAV` a 16 kHz mono 16-bit clip.
    #[test]
    #[ignore = "needs the 670 MB model on disk"]
    fn probe_transcribes_a_real_clip() {
        let dir = std::env::var("SILL_PARAKEET_DIR").expect("SILL_PARAKEET_DIR");
        let wav = std::env::var("SILL_PARAKEET_WAV").expect("SILL_PARAKEET_WAV");
        let samples = transcribe_rs::audio::read_wav_samples(std::path::Path::new(&wav)).unwrap();

        let mut held = None;
        let loaded = load_into(&mut held, "parakeet-v3", std::path::Path::new(&dir)).unwrap();
        let started = Instant::now();
        let text = loaded
            .model
            .transcribe_with(&samples, &ParakeetParams::default())
            .unwrap()
            .text;
        println!("{:.1} s of audio in {:?}: {text}", samples.len() as f64 / 16_000.0, started.elapsed());
        assert!(text.split_whitespace().count() > 10, "{text}");
    }
}
