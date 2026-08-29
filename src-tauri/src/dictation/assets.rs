//! The whisper models, downloaded on demand.
//!
//! A model is data, not a program: it is never executed, never listed as an
//! installed runtime, and lives beside its siblings rather than in a
//! versioned directory of its own. It shares the engine's download and verify
//! primitives and nothing else.
//!
//! URLs are pinned to a HuggingFace commit rather than `main`. The repo is
//! stable, but a pinned digest against a moving reference is a trap that
//! only springs the day someone touches the file.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::dictation::error::{DictationError, Result};
use crate::dictation::fetch;
use crate::dictation::models::SetupProgress;

/// HuggingFace revision every model URL is pinned to.
const REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

/// One downloadable model, as the settings picker sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModel {
    /// Stable id, also the file name on disk.
    pub id: String,
    /// What to show in a picker.
    pub label: String,
    pub size_bytes: u64,
    /// Whether it is already downloaded.
    pub installed: bool,
}

struct Model {
    id: &'static str,
    label: &'static str,
    file: &'static str,
    sha256: &'static str,
    size_bytes: u64,
    /// Resident working set once loaded, measured rather than derived.
    ///
    /// whisper allocates compute buffers well past the weights, so the file
    /// size is not a usable estimate: a 465 MB model sits at about 649 MB.
    memory_bytes: u64,
}

/// The models offered, smallest first.
///
/// `small.en` is the default rather than `base.en`: this has to be at least
/// as accurate as the tool it replaces, and a resident server pays the load
/// cost once.
const MODELS: &[Model] = &[
    Model {
        id: "tiny.en",
        label: "Tiny (fastest, least accurate)",
        file: "ggml-tiny.en.bin",
        sha256: "sha256:921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        size_bytes: 77_704_715,
        memory_bytes: 171_966_464,
    },
    Model {
        id: "base.en",
        label: "Base (fast)",
        file: "ggml-base.en.bin",
        sha256: "sha256:a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
        size_bytes: 147_964_211,
        memory_bytes: 266_338_304,
    },
    Model {
        id: "small.en",
        label: "Small (recommended)",
        file: "ggml-small.en.bin",
        sha256: "sha256:c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        size_bytes: 487_614_201,
        memory_bytes: 680_525_824,
    },
    Model {
        id: "medium.en",
        label: "Medium (most accurate, slowest)",
        file: "ggml-medium.en.bin",
        sha256: "sha256:cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
        size_bytes: 1_533_774_781,
        memory_bytes: 1_875_902_464,
    },
];

pub const DEFAULT_MODEL: &str = "small.en";

fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|model| model.id == id)
}

fn url_for(model: &Model) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/{REVISION}/{}",
        model.file
    )
}

/// Where models live: `<app data>/whisper-models/`.
fn models_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| DictationError::Platform(format!("app data dir: {e}")))?
        .join("whisper-models");
    Ok(dir)
}

/// Path a model occupies once installed, whether or not it is there yet.
pub fn model_path(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let model = model(id)
        .ok_or_else(|| DictationError::NotFound(format!("Unknown whisper model '{id}'")))?;
    Ok(models_dir(app)?.join(model.file))
}

/// Every model, with whether it is downloaded.
pub fn list(app: &AppHandle) -> Vec<WhisperModel> {
    MODELS
        .iter()
        .map(|model| WhisperModel {
            id: model.id.to_string(),
            label: model.label.to_string(),
            size_bytes: model.size_bytes,
            installed: models_dir(app)
                .map(|dir| dir.join(model.file).is_file())
                .unwrap_or(false),
        })
        .collect()
}

pub fn is_installed(app: &AppHandle, id: &str) -> bool {
    model_path(app, id)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

/// Downloads `id` if it is not already present.
///
/// Progress goes out as `dictation:setup` so the settings panel can draw a
/// bar for a download that takes minutes.
pub async fn ensure(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let model = model(id)
        .ok_or_else(|| DictationError::NotFound(format!("Unknown whisper model '{id}'")))?;
    let dir = models_dir(app)?;
    let destination = dir.join(model.file);
    if destination.is_file() {
        return Ok(destination);
    }
    std::fs::create_dir_all(&dir)?;

    eprintln!(
        "[sill] downloading the {} model ({:.0} MB)",
        model.id,
        model.size_bytes as f64 / (1024.0 * 1024.0)
    );

    // Staged beside the destination, so the rename that commits it stays
    // inside one filesystem and therefore stays atomic. Downloading straight
    // onto the destination would leave a partial file that every "is it
    // installed" check reads as a finished one.
    let staging = dir.join(format!(".{}.partial", model.file));

    fetch::download_to(
        &fetch::client(),
        &url_for(model),
        &staging,
        |done, total| {
            SetupProgress::Model {
                bytes_downloaded: done,
                // The CDN reports a length, but a missing one must not render as
                // a full bar, and the published size is known here anyway.
                total_bytes: if total > 0 { total } else { model.size_bytes },
            }
            .emit(app);
        },
    )
    .await?;

    SetupProgress::Verifying.emit(app);
    fetch::verify(&staging, model.sha256)?;
    fetch::commit(&staging, &destination)?;

    Ok(destination)
}

/// Deletes `id` if it is installed. Returns whether anything was removed.
pub fn remove(app: &AppHandle, id: &str) -> Result<bool> {
    let path = model_path(app, id)?;
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&path)?;
    Ok(true)
}

/// Published byte size of `id`, for showing a download size before starting.
pub fn size_of(id: &str) -> Option<u64> {
    model(id).map(|model| model.size_bytes)
}

/// Roughly what `id` holds in memory once the server has loaded it.
pub fn memory_of(id: &str) -> Option<u64> {
    model(id).map(|model| model.memory_bytes)
}

/// Display name for `id`.
pub fn label_of(id: &str) -> Option<&'static str> {
    model(id).map(|model| model.label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_has_a_distinct_id_and_file() {
        for (i, a) in MODELS.iter().enumerate() {
            for b in MODELS.iter().skip(i + 1) {
                assert_ne!(a.id, b.id);
                assert_ne!(a.file, b.file);
                assert_ne!(a.sha256, b.sha256);
            }
        }
    }

    #[test]
    fn the_default_model_is_one_of_the_offered_models() {
        assert!(model(DEFAULT_MODEL).is_some());
    }

    #[test]
    fn urls_pin_a_revision_rather_than_a_branch() {
        // A digest pinned against `main` breaks silently the day the file is
        // touched, and only for people downloading after that.
        for model in MODELS {
            let url = url_for(model);
            assert!(url.contains(REVISION), "{url}");
            assert!(!url.contains("/main/"), "{url}");
            assert!(url.ends_with(model.file), "{url}");
        }
    }

    #[test]
    fn resident_memory_always_exceeds_the_file() {
        // The file size is not a usable estimate: whisper allocates compute
        // buffers past the weights. Anyone tempted to drop the measured
        // figure and use `size_bytes` gets caught here.
        for model in MODELS {
            assert!(
                model.memory_bytes > model.size_bytes,
                "{} claims to hold less than it weighs",
                model.id
            );
        }
    }

    #[test]
    fn every_digest_is_a_prefixed_sha256() {
        for model in MODELS {
            assert!(model.sha256.starts_with("sha256:"), "{}", model.id);
            assert_eq!(model.sha256.len(), "sha256:".len() + 64, "{}", model.id);
        }
    }

    #[test]
    fn models_are_offered_smallest_first() {
        // The picker shows them in order, and a user scanning for "the small
        // one" should not have to read every size.
        let sizes: Vec<u64> = MODELS.iter().map(|m| m.size_bytes).collect();
        let mut sorted = sizes.clone();
        sorted.sort_unstable();
        assert_eq!(sizes, sorted);
    }

    #[test]
    fn an_unknown_model_is_not_resolvable() {
        assert!(model("large-v3").is_none());
    }

    #[test]
    fn a_size_is_published_for_every_offered_model_and_nothing_else() {
        // Settings shows the download size before the user commits to it, so
        // a model with no published size would render a blank button.
        for model in MODELS {
            assert_eq!(size_of(model.id), Some(model.size_bytes));
        }
        assert_eq!(size_of("large-v3"), None);
    }

    #[test]
    fn the_staging_name_cannot_collide_with_an_installed_model() {
        // A half-downloaded file that happens to be named like a finished one
        // would read as installed on the next launch.
        let staged: Vec<String> = MODELS
            .iter()
            .map(|model| format!(".{}.partial", model.file))
            .collect();
        for name in &staged {
            assert!(!MODELS.iter().any(|model| model.file == name), "{name}");
        }
    }
}
