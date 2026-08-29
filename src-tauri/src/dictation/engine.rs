//! Installing whisper.cpp's server binary.
//!
//! Sill is Windows only, so this installs exactly one published artifact into
//! one place rather than carrying a general runtime manager. Everything about
//! the archive below was measured against the real download, not read off a
//! release page.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::dictation::error::{DictationError, Result};
use crate::dictation::fetch;

/// The published build. `b4938` is whisper.cpp 1.9.3.
pub const VERSION: &str = "1.9.3+b4938";

const URL: &str =
    "https://github.com/ggml-org/whisper.cpp/releases/download/b4938/whisper-bin-x64.zip";
const SHA256: &str = "sha256:c2a4b60edb11f7e11a9191ffb50929535527d4d91c9903dbe3e554583bbbc63d";

/// Compressed size of the archive, for showing a download size up front.
pub const ARCHIVE_BYTES: u64 = 8_361_840;

/// Every file in the archive sits under this one directory.
const ARCHIVE_ROOT: &str = "Release";

/// The binary that gets run.
pub const ENTRY: &str = "whisper-server.exe";

/// What to extract, out of the archive's 38 files.
///
/// whisper.cpp's Windows release is built with `BUILD_SHARED_LIBS=ON` and
/// `GGML_BACKEND_DL=ON`, so the server is useless without `whisper.dll` and
/// **all nine** `ggml-cpu-*.dll` variants: ggml picks one at runtime by CPU
/// capability, and which one is not knowable at install time. Taking only the
/// executable produces `cannot open shared object file` and nothing else.
///
/// Skipping the rest (llama.dll, parakeet, eight test executables, wchess,
/// SDL2.dll) takes 21 MB of archive down to 10 MB on disk.
const INCLUDE: &[&str] = &["whisper-server.exe", "whisper.dll", "ggml"];

/// Where the engine lives: `<app data>/whisper-engine/<version>/`.
///
/// Versioned, so a future build installs alongside rather than over a running
/// server, and so an interrupted upgrade cannot leave a half-replaced set of
/// libraries that load but disagree with each other.
pub fn install_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| DictationError::Platform(format!("app data dir: {e}")))?
        .join("whisper-engine")
        .join(VERSION))
}

/// The server binary's path, whether or not it is installed yet.
pub fn binary_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(install_dir(app)?.join(ENTRY))
}

pub fn is_installed(app: &AppHandle) -> bool {
    binary_path(app).map(|path| path.is_file()).unwrap_or(false)
}

/// Downloads and installs the engine if it is not already there.
pub async fn ensure(app: &AppHandle, mut progress: impl FnMut(u64, u64)) -> Result<PathBuf> {
    let binary = binary_path(app)?;
    if binary.is_file() {
        return Ok(binary);
    }

    let dir = install_dir(app)?;
    std::fs::create_dir_all(&dir)?;

    let archive = dir.join(".engine.zip.partial");
    fetch::download_to(&fetch::client(), URL, &archive, |done, total| {
        progress(done, if total > 0 { total } else { ARCHIVE_BYTES });
    })
    .await?;

    fetch::verify(&archive, SHA256)?;

    let staging = dir.join(".staging");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging)?;

    let extracted = unpack(&archive, &staging);
    let _ = std::fs::remove_file(&archive);

    if let Err(err) = extracted {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(err);
    }

    // Moved file by file rather than renaming the staging directory over the
    // install directory: the archive was downloaded *into* the install
    // directory, so renaming over it would take the staging path out from
    // under itself.
    for entry in std::fs::read_dir(&staging)? {
        let entry = entry?;
        std::fs::rename(entry.path(), dir.join(entry.file_name()))?;
    }
    let _ = std::fs::remove_dir_all(&staging);

    if !binary.is_file() {
        return Err(DictationError::Other(format!(
            "The whisper archive did not contain {ENTRY}"
        )));
    }

    Ok(binary)
}

/// Extracts the wanted files into `into`, flattening the archive's root away.
fn unpack(archive: &Path, into: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| DictationError::Other(format!("The whisper archive is unreadable: {e}")))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| DictationError::Other(format!("Could not read archive entry: {e}")))?;

        if entry.is_dir() {
            continue;
        }

        let Some(name) = wanted_name(entry.name()) else {
            continue;
        };

        let mut out = std::fs::File::create(into.join(name))?;
        std::io::copy(&mut entry, &mut out)?;
    }

    Ok(())
}

/// The flat file name to extract an archive path as, or `None` to skip it.
///
/// A pure function, so the include set can be checked against real archive
/// paths without downloading 8 MB. It also carries the traversal guard: an
/// entry naming a parent directory is skipped rather than escaping the
/// install directory.
fn wanted_name(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");

    if path.split('/').any(|part| part == "..") {
        return None;
    }

    let (root, name) = path.split_once('/')?;
    if root != ARCHIVE_ROOT || name.contains('/') {
        return None;
    }

    INCLUDE
        .iter()
        .any(|wanted| name.starts_with(wanted))
        .then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_and_every_library_it_loads_are_taken() {
        // ggml picks its CPU backend at runtime by capability, so which
        // variant this machine needs is not knowable at install time. Taking
        // fewer than all of them works until it does not.
        for path in [
            "Release/whisper-server.exe",
            "Release/whisper.dll",
            "Release/ggml.dll",
            "Release/ggml-base.dll",
            "Release/ggml-cpu-haswell.dll",
            "Release/ggml-cpu-sandybridge.dll",
        ] {
            assert!(
                wanted_name(path).is_some(),
                "{path} has to be installed or the server cannot start"
            );
        }
    }

    #[test]
    fn the_rest_of_the_archive_is_left_behind() {
        for path in [
            "Release/llama.dll",
            "Release/SDL2.dll",
            "Release/wchess.exe",
            "Release/test-whisper.exe",
            "Release/parakeet.exe",
        ] {
            assert!(
                wanted_name(path).is_none(),
                "{path} is part of the 11 MB dictation never uses"
            );
        }
    }

    #[test]
    fn names_are_flattened_out_of_the_archive_root() {
        assert_eq!(
            wanted_name("Release/whisper.dll").as_deref(),
            Some("whisper.dll")
        );
    }

    #[test]
    fn a_backslash_separated_entry_is_understood() {
        // Zip mandates forward slashes, but archives written on Windows by
        // careless tooling do carry backslashes.
        let separator = char::from(92);
        let path = format!("Release{separator}whisper.dll");
        assert_eq!(wanted_name(&path).as_deref(), Some("whisper.dll"));
    }

    #[test]
    fn a_traversal_entry_is_refused() {
        assert!(wanted_name("Release/../whisper.dll").is_none());
        assert!(wanted_name("../whisper.dll").is_none());
    }

    #[test]
    fn a_file_outside_the_root_is_ignored() {
        assert!(wanted_name("whisper.dll").is_none());
        assert!(wanted_name("Other/whisper.dll").is_none());
        assert!(wanted_name("Release/sub/whisper.dll").is_none());
    }

    #[test]
    fn the_version_key_is_semver_parseable() {
        // Build metadata after `+` is semver legal; a bare build number is
        // not, and anything comparing versions would silently skip it.
        let (core, build) = VERSION.split_once('+').expect("a build metadata suffix");
        assert_eq!(
            core.split('.').count(),
            3,
            "{core} should be major.minor.patch"
        );
        assert!(!build.is_empty());
    }
}
