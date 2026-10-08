//! The local dictation models, downloaded on demand.
//!
//! Most are whisper models, one file each, served by the whisper.cpp server.
//! Parakeet is the exception: four ONNX files in a directory of their own,
//! run inside Sill by `dictation::parakeet` with no server at all. Both kinds
//! are offered in the same picker because to the person choosing, they are
//! the same kind of thing.
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

/// HuggingFace revision every whisper model URL is pinned to.
const REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";
const WHISPER_REPO: &str = "ggerganov/whisper.cpp";

/// Parakeet TDT 0.6B v3 exported to ONNX, int8. CC-BY-4.0, from NVIDIA's
/// `nvidia/parakeet-tdt-0.6b-v3`; credited in resources/NOTICE.
const PARAKEET_REPO: &str = "istupakov/parakeet-tdt-0.6b-v3-onnx";
const PARAKEET_REVISION: &str = "8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce";

/// Which engine runs a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// The whisper.cpp server, a process of its own.
    Whisper,
    /// Parakeet, loaded into Sill itself.
    Parakeet,
}

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
    pub engine: Engine,
}

/// One file of a model.
struct Part {
    file: &'static str,
    sha256: &'static str,
    size_bytes: u64,
}

struct Model {
    id: &'static str,
    label: &'static str,
    engine: Engine,
    repo: &'static str,
    revision: &'static str,
    /// The directory a many-file model lives in, under the models directory.
    /// `None` for a one-file model, which sits there directly.
    dir: Option<&'static str>,
    parts: &'static [Part],
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
        engine: Engine::Whisper,
        repo: WHISPER_REPO,
        revision: REVISION,
        dir: None,
        parts: &[Part {
            file: "ggml-tiny.en.bin",
            sha256: "sha256:921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
            size_bytes: 77_704_715,
        }],
        memory_bytes: 171_966_464,
    },
    Model {
        id: "base.en",
        label: "Base (fast)",
        engine: Engine::Whisper,
        repo: WHISPER_REPO,
        revision: REVISION,
        dir: None,
        parts: &[Part {
            file: "ggml-base.en.bin",
            sha256: "sha256:a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
            size_bytes: 147_964_211,
        }],
        memory_bytes: 266_338_304,
    },
    Model {
        id: "small.en",
        label: "Small (recommended)",
        engine: Engine::Whisper,
        repo: WHISPER_REPO,
        revision: REVISION,
        dir: None,
        parts: &[Part {
            file: "ggml-small.en.bin",
            sha256: "sha256:c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
            size_bytes: 487_614_201,
        }],
        memory_bytes: 680_525_824,
    },
    /*
     * Parakeet TDT 0.6B v3, int8.
     *
     * Measured against `small.en` on this machine (2026-10-03, 16 threads):
     * the same speed on clean speech, and on a 95-second dictation with
     * pauses of two to eight seconds whisper dropped two of eight sentences
     * (28% word errors) where Parakeet kept all of them (2.7%). Punctuation
     * and capitals come from the model. It takes no prompt, so the
     * Vocabulary and app context settings do nothing with it.
     *
     * Memory is the private peak transcribing a 15-second clip: it grows
     * with the clip, to 1.26 GB at 88 seconds and 1.62 GB at 142.
     */
    Model {
        id: "parakeet-v3",
        label: "Parakeet v3 (fast, punctuates, keeps up with pauses)",
        engine: Engine::Parakeet,
        repo: PARAKEET_REPO,
        revision: PARAKEET_REVISION,
        dir: Some("parakeet-tdt-0.6b-v3-int8"),
        parts: &[
            Part {
                file: "encoder-model.int8.onnx",
                sha256: "sha256:6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09",
                size_bytes: 652_183_999,
            },
            Part {
                file: "decoder_joint-model.int8.onnx",
                sha256: "sha256:eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70",
                size_bytes: 18_202_004,
            },
            Part {
                file: "nemo128.onnx",
                sha256: "sha256:a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f",
                size_bytes: 139_764,
            },
            Part {
                file: "vocab.txt",
                sha256: "sha256:d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
                size_bytes: 93_939,
            },
        ],
        memory_bytes: 753_926_144,
    },
    Model {
        id: "medium.en",
        label: "Medium (most accurate, slowest)",
        engine: Engine::Whisper,
        repo: WHISPER_REPO,
        revision: REVISION,
        dir: None,
        parts: &[Part {
            file: "ggml-medium.en.bin",
            sha256: "sha256:cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
            size_bytes: 1_533_774_781,
        }],
        memory_bytes: 1_875_902_464,
    },
];

pub const DEFAULT_MODEL: &str = "small.en";

fn model(id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|model| model.id == id)
}

fn url_for(model: &Model, part: &Part) -> String {
    format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        model.repo, model.revision, part.file
    )
}

impl Model {
    fn size_bytes(&self) -> u64 {
        self.parts.iter().map(|part| part.size_bytes).sum()
    }

    /// Where the model is: its one file, or its directory.
    fn path_in(&self, dir: &std::path::Path) -> PathBuf {
        match self.dir {
            Some(own) => dir.join(own),
            None => dir.join(self.parts[0].file),
        }
    }

    /// Where one of its files goes.
    fn part_in(&self, dir: &std::path::Path, part: &Part) -> PathBuf {
        match self.dir {
            Some(own) => dir.join(own).join(part.file),
            None => dir.join(part.file),
        }
    }

    fn installed_in(&self, dir: &std::path::Path) -> bool {
        self.parts.iter().all(|part| self.part_in(dir, part).is_file())
    }
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

/// Path a model occupies once installed, whether or not it is there yet: the
/// file for a whisper model, the directory for Parakeet.
pub fn model_path(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let model = model(id)
        .ok_or_else(|| DictationError::NotFound(format!("Unknown dictation model '{id}'")))?;
    Ok(model.path_in(&models_dir(app)?))
}

/// Which engine runs `id`. Unknown ids are whisper's, which is what every
/// id was before there was a choice.
pub fn engine_of(id: &str) -> Engine {
    model(id).map_or(Engine::Whisper, |model| model.engine)
}

/// Every model, with whether it is downloaded.
pub fn list(app: &AppHandle) -> Vec<WhisperModel> {
    MODELS
        .iter()
        .map(|model| WhisperModel {
            id: model.id.to_string(),
            label: model.label.to_string(),
            size_bytes: model.size_bytes(),
            installed: models_dir(app)
                .map(|dir| model.installed_in(&dir))
                .unwrap_or(false),
            engine: model.engine,
        })
        .collect()
}

pub fn is_installed(app: &AppHandle, id: &str) -> bool {
    match (model(id), models_dir(app)) {
        (Some(model), Ok(dir)) => model.installed_in(&dir),
        _ => false,
    }
}

/// Downloads `id` if it is not already present.
///
/// Progress goes out as `dictation:setup` so the settings panel can draw a
/// bar for a download that takes minutes.
pub async fn ensure(app: &AppHandle, id: &str) -> Result<PathBuf> {
    let model = model(id)
        .ok_or_else(|| DictationError::NotFound(format!("Unknown dictation model '{id}'")))?;
    let dir = models_dir(app)?;
    if model.installed_in(&dir) {
        return Ok(model.path_in(&dir));
    }

    let total = model.size_bytes();
    eprintln!(
        "[sill] downloading the {} model ({:.0} MB)",
        model.id,
        total as f64 / (1024.0 * 1024.0)
    );

    // One bar for the whole model: the files already fetched count as done,
    // so a many-file model fills one bar rather than four.
    let mut before = 0u64;
    for part in model.parts {
        let destination = model.part_in(&dir, part);
        if destination.is_file() {
            before += part.size_bytes;
            continue;
        }
        let folder = destination
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| dir.clone());
        std::fs::create_dir_all(&folder)?;

        // Staged beside the destination, so the rename that commits it stays
        // inside one filesystem and therefore stays atomic. Downloading
        // straight onto the destination would leave a partial file that every
        // "is it installed" check reads as a finished one.
        let staging = folder.join(format!(".{}.partial", part.file));

        fetch::download_to(&fetch::client(), &url_for(model, part), &staging, |done, _| {
            SetupProgress::Model {
                bytes_downloaded: before + done,
                // The published sizes, not the CDN's length: a missing length
                // must not render as a full bar, and the sizes are known here.
                total_bytes: total,
            }
            .emit(app);
        })
        .await?;

        SetupProgress::Verifying.emit(app);
        fetch::verify(&staging, part.sha256)?;
        fetch::commit(&staging, &destination)?;
        before += part.size_bytes;
    }

    Ok(model.path_in(&dir))
}

/// Deletes `id` if it is installed. Returns whether anything was removed.
pub fn remove(app: &AppHandle, id: &str) -> Result<bool> {
    let model = model(id)
        .ok_or_else(|| DictationError::NotFound(format!("Unknown dictation model '{id}'")))?;
    let path = model.path_in(&models_dir(app)?);
    match model.dir {
        Some(_) if path.is_dir() => std::fs::remove_dir_all(&path)?,
        None if path.is_file() => std::fs::remove_file(&path)?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// Published byte size of `id`, for showing a download size before starting.
pub fn size_of(id: &str) -> Option<u64> {
    model(id).map(Model::size_bytes)
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
        let parts: Vec<&Part> = MODELS.iter().flat_map(|model| model.parts).collect();
        for (i, a) in MODELS.iter().enumerate() {
            for b in MODELS.iter().skip(i + 1) {
                assert_ne!(a.id, b.id);
                assert!(a.dir.is_none() || a.dir != b.dir, "{} and {} share a directory", a.id, b.id);
            }
        }
        for (i, a) in parts.iter().enumerate() {
            for b in parts.iter().skip(i + 1) {
                assert_ne!(a.file, b.file);
                assert_ne!(a.sha256, b.sha256);
            }
        }
    }

    /// Parakeet is a directory of four files and whisper's are one file each;
    /// both must come out of the same catalogue the same way.
    #[test]
    fn a_many_file_model_lives_in_its_own_directory() {
        let dir = std::path::Path::new("models");
        let parakeet = model("parakeet-v3").expect("offered");
        assert_eq!(parakeet.engine, Engine::Parakeet);
        assert_eq!(parakeet.path_in(dir), dir.join("parakeet-tdt-0.6b-v3-int8"));
        assert_eq!(
            parakeet.part_in(dir, &parakeet.parts[0]),
            dir.join("parakeet-tdt-0.6b-v3-int8").join("encoder-model.int8.onnx")
        );

        let small = model("small.en").expect("offered");
        assert_eq!(small.path_in(dir), dir.join("ggml-small.en.bin"));
        assert_eq!(engine_of("small.en"), Engine::Whisper);
        assert_eq!(engine_of("parakeet-v3"), Engine::Parakeet);
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
            for part in model.parts {
                let url = url_for(model, part);
                assert!(url.contains(model.revision), "{url}");
                assert_eq!(model.revision.len(), 40, "{}", model.id);
                assert!(!url.contains("/main/"), "{url}");
                assert!(url.ends_with(part.file), "{url}");
            }
        }
    }

    #[test]
    fn resident_memory_always_exceeds_the_file() {
        // The file size is not a usable estimate: whisper allocates compute
        // buffers past the weights. Anyone tempted to drop the measured
        // figure and use `size_bytes` gets caught here.
        for model in MODELS {
            assert!(
                model.memory_bytes > model.size_bytes(),
                "{} claims to hold less than it weighs",
                model.id
            );
        }
    }

    #[test]
    fn every_digest_is_a_prefixed_sha256() {
        for part in MODELS.iter().flat_map(|model| model.parts) {
            assert!(part.sha256.starts_with("sha256:"), "{}", part.file);
            assert_eq!(part.sha256.len(), "sha256:".len() + 64, "{}", part.file);
        }
    }

    #[test]
    fn models_are_offered_smallest_first() {
        // The picker shows them in order, and a user scanning for "the small
        // one" should not have to read every size.
        let sizes: Vec<u64> = MODELS.iter().map(Model::size_bytes).collect();
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
            assert_eq!(size_of(model.id), Some(model.size_bytes()));
        }
        assert_eq!(size_of("large-v3"), None);
    }

    #[test]
    fn the_staging_name_cannot_collide_with_an_installed_model() {
        // A half-downloaded file that happens to be named like a finished one
        // would read as installed on the next launch.
        let files: Vec<&str> = MODELS
            .iter()
            .flat_map(|model| model.parts)
            .map(|part| part.file)
            .collect();
        for file in &files {
            let staged = format!(".{file}.partial");
            assert!(!files.contains(&staged.as_str()), "{staged}");
        }
    }
}
