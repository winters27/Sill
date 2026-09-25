//! Installing whisper.cpp's server binary, and choosing which one runs.
//!
//! Sill is Windows only, so this installs exactly one published artifact into
//! one place rather than carrying a general runtime manager. Everything about
//! the archive below was measured against the real download, not read off a
//! release page.
//!
//! ## Versions sit side by side
//!
//! Each build installs into `<app data>/whisper-engine/<version>/`, and a file
//! called `active` beside them names the one this machine has proven. A new
//! build is downloaded, checked and started *next to* the one in use, and only
//! becomes active once it has loaded a model and answered. So an update that
//! fails at any step leaves dictation exactly as it was.
//!
//! With no `active` file, or one naming a build that is gone, the newest
//! installed build runs. That is what an install from before the file existed
//! looks like, and it is why a newer build appearing on disk is never enough on
//! its own to switch to it: the file is written only after the proof.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::dictation::error::{DictationError, Result};
use crate::dictation::fetch;

/// One published build of the engine, as far as installing it needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Build {
    /// `major.minor.patch+bNNNN`: the release version, then the build tag the
    /// binaries are actually published under. Semver legal, and also the
    /// directory name.
    pub version: String,
    pub url: String,
    /// `sha256:` followed by the hex digest.
    pub sha256: String,
    /// Compressed size of the archive, for showing a download size up front.
    pub bytes: u64,
}

/// The build that was measured by hand when this was written.
///
/// Installed when GitHub cannot be asked for anything newer, so a first setup
/// on a machine that cannot reach the API still ends with working dictation.
/// `b4938` is whisper.cpp 1.9.3.
pub fn baseline() -> Build {
    Build {
        version: "1.9.3+b4938".to_string(),
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/b4938/whisper-bin-x64.zip"
            .to_string(),
        sha256: "sha256:c2a4b60edb11f7e11a9191ffb50929535527d4d91c9903dbe3e554583bbbc63d"
            .to_string(),
        bytes: 8_361_840,
    }
}

/// Every file in the archive sits under this one directory.
const ARCHIVE_ROOT: &str = "Release";

/// The binary that gets run.
pub const ENTRY: &str = "whisper-server.exe";

/// What to extract, out of the archive's forty or so files.
///
/// whisper.cpp's Windows release is built with `BUILD_SHARED_LIBS=ON` and
/// `GGML_BACKEND_DL=ON`, so the server is useless without `whisper.dll` and
/// **all nine** `ggml-cpu-*.dll` variants: ggml picks one at runtime by CPU
/// capability, and which one is not knowable at install time. Taking only the
/// executable produces `cannot open shared object file` and nothing else.
///
/// Skipping the rest (llama.dll, parakeet, the test executables, wchess,
/// SDL2.dll) takes 21 MB of archive down to 10 MB on disk.
const INCLUDE: &[&str] = &["whisper-server.exe", "whisper.dll", "ggml"];

/// Files a build has to have once unpacked, beyond at least one CPU backend.
///
/// A build that follows upstream is not measured by hand before it is
/// offered, so this is the measurement, taken on the machine instead. A future
/// archive that moves its root or renames a library then fails here with the
/// name of what is missing, rather than installing a server that cannot load.
const REQUIRED: &[&str] = &["whisper-server.exe", "whisper.dll", "ggml.dll", "ggml-base.dll"];

/// Prefix of the CPU backends, of which at least one has to be present.
const CPU_BACKEND: &str = "ggml-cpu-";

/// Names the build this machine has proven. Lives beside the version
/// directories, which is why it is not named like one.
const ACTIVE_FILE: &str = "active";

/// How long `--help` gets before the build is judged broken.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// An installed build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    pub version: String,
    pub exe: PathBuf,
}

/// Where every build lives: `<app data>/whisper-engine/`.
pub fn engines_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| DictationError::Platform(format!("app data dir: {e}")))?
        .join("whisper-engine"))
}

/// The build that runs, or `None` when there is none installed.
pub fn active(app: &AppHandle) -> Option<Engine> {
    let root = engines_dir(app).ok()?;
    let version = active_in(&root)?;
    Some(Engine {
        exe: root.join(&version).join(ENTRY),
        version,
    })
}

/// Marks `version` as the build to run from now on.
pub fn activate(app: &AppHandle, version: &str) -> Result<()> {
    set_active_in(&engines_dir(app)?, version)
}

/// Deletes one build. Not an error when it is already gone.
pub fn remove(app: &AppHandle, version: &str) -> Result<()> {
    let dir = engines_dir(app)?.join(version);
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(DictationError::Io(err)),
    }
}

/// Deletes every build except `keep`, and anything a crashed install left.
///
/// Best effort: a virus scanner holding a DLL open for a moment is a reason to
/// try again next time, not a reason to fail an update that has already
/// succeeded. Returns what went.
pub fn sweep(app: &AppHandle, keep: &str) -> Vec<String> {
    let Ok(root) = engines_dir(app) else {
        return Vec::new();
    };
    sweep_in(&root, keep)
}

/// Downloads, checks and unpacks `build` beside whatever is installed, without
/// making it active.
///
/// Proven as far as it can be without a model: the digest matches, every
/// required library is there, and `--help` runs and lists every option Sill
/// passes. On any failure the half-installed directory is removed, so a
/// broken build never sits on disk looking installed.
pub async fn install(
    app: &AppHandle,
    build: &Build,
    progress: impl FnMut(u64, u64),
) -> Result<Engine> {
    install_in(&engines_dir(app)?, build, progress).await
}

/// [`install`] into a plain directory.
pub async fn install_in(
    root: &Path,
    build: &Build,
    mut progress: impl FnMut(u64, u64),
) -> Result<Engine> {
    let dir = root.join(&build.version);
    let exe = dir.join(ENTRY);

    if exe.is_file() {
        return Ok(Engine {
            version: build.version.clone(),
            exe,
        });
    }

    let outcome = async {
        std::fs::create_dir_all(&dir)?;

        let archive = dir.join(".engine.zip.partial");
        fetch::download_to(&fetch::client(), &build.url, &archive, |done, total| {
            progress(done, if total > 0 { total } else { build.bytes });
        })
        .await?;

        fetch::verify(&archive, &build.sha256)?;

        let staging = dir.join(".staging");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;

        let extracted = unpack(&archive, &staging);
        let _ = std::fs::remove_file(&archive);
        extracted?;

        missing_from(&staging).map_or(Ok(()), |missing| {
            Err(DictationError::Validation(format!(
                "whisper.cpp {} is missing {missing}, so it cannot run",
                build.version
            )))
        })?;

        // Moved file by file rather than renaming the staging directory over
        // the install directory: the archive was downloaded *into* the
        // install directory, so renaming over it would take the staging path
        // out from under itself.
        for entry in std::fs::read_dir(&staging)? {
            let entry = entry?;
            std::fs::rename(entry.path(), dir.join(entry.file_name()))?;
        }
        let _ = std::fs::remove_dir_all(&staging);

        probe(&exe).await
    }
    .await;

    match outcome {
        Ok(()) => Ok(Engine {
            version: build.version.clone(),
            exe,
        }),
        Err(err) => {
            let _ = std::fs::remove_dir_all(&dir);
            Err(err)
        }
    }
}

/// Runs `--help` and checks the build still takes every option Sill passes.
///
/// A renamed flag is the likeliest way a newer whisper.cpp breaks Sill, and
/// without this it would surface as a server that exits the moment it is
/// started, with the reason buried in its stderr.
pub async fn probe(exe: &Path) -> Result<()> {
    let exe = exe.to_path_buf();
    let help = tauri::async_runtime::spawn_blocking(move || run_help(&exe))
        .await
        .map_err(|e| DictationError::Other(format!("The engine check did not finish: {e}")))??;

    let missing = missing_flags(&help, &flags_sill_passes());
    if missing.is_empty() {
        Ok(())
    } else {
        Err(DictationError::Validation(format!(
            "This whisper.cpp no longer accepts {}",
            missing.join(", ")
        )))
    }
}

/// `whisper-server --help`, stdout and stderr together.
///
/// Written to a file rather than piped, so a build that prints more than a
/// pipe buffer holds cannot block itself, and so the timeout can kill it
/// without first having to unblock a reader.
fn run_help(exe: &Path) -> Result<String> {
    let dir = exe
        .parent()
        .ok_or_else(|| DictationError::Other("The engine has no directory".to_string()))?;
    let log_path = dir.join(".help.txt");
    let log = std::fs::File::create(&log_path)?;

    let mut command = Command::new(exe);
    command
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    crate::dictation::server::no_window(&mut command);

    let mut child = command.spawn()?;
    let deadline = Instant::now() + PROBE_TIMEOUT;

    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&log_path);
            return Err(DictationError::Other(format!(
                "whisper-server --help did not finish within {}s",
                PROBE_TIMEOUT.as_secs()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let help = std::fs::read_to_string(&log_path).unwrap_or_default();
    let _ = std::fs::remove_file(&log_path);

    if !status.success() {
        return Err(DictationError::Other(format!(
            "whisper-server --help exited with {status}"
        )));
    }
    Ok(help)
}

/// The options `server::server_args` passes, taken from it rather than
/// written out again, so the two cannot drift apart.
fn flags_sill_passes() -> Vec<String> {
    crate::dictation::server::server_args(Path::new("model.bin"), 1, 1)
        .into_iter()
        .filter(|arg| arg.starts_with('-'))
        .collect()
}

/// Which of `wanted` the help text never names.
///
/// Matches whole tokens, so `-t` is not found inside `-tp` or `--threads`.
/// whisper-server lists an option as `-t N,      --threads N`, so commas are
/// separators too.
pub fn missing_flags(help: &str, wanted: &[String]) -> Vec<String> {
    let named: std::collections::HashSet<&str> = help
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| token.starts_with('-'))
        .collect();

    wanted
        .iter()
        .filter(|flag| !named.contains(flag.as_str()))
        .cloned()
        .collect()
}

/// The first required file absent from `dir`, or `None` when all are there.
fn missing_from(dir: &Path) -> Option<String> {
    if let Some(missing) = REQUIRED.iter().find(|name| !dir.join(name).is_file()) {
        return Some((*missing).to_string());
    }

    let has_backend = std::fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(CPU_BACKEND))
            })
        })
        .unwrap_or(false);

    (!has_backend).then(|| format!("{CPU_BACKEND}*.dll"))
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

// ------------------------------------------------------------------ versions

/// `1.9.4+b5130` as numbers that order the way the builds do.
///
/// Semver ignores build metadata when ordering, which is exactly the part
/// that tells two builds of one version apart, so the build number is kept as
/// a fourth field. A version with no build suffix orders before any with one.
pub fn parse_version(key: &str) -> Option<(u64, u64, u64, u64)> {
    let (core, build) = match key.split_once('+') {
        Some((core, build)) => (core, Some(build)),
        None => (key, None),
    };

    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let (major, minor, patch) = (parts.next()??, parts.next()??, parts.next()??);
    if parts.next().is_some() {
        return None;
    }

    let build = match build {
        Some(build) => build.strip_prefix('b')?.parse::<u64>().ok()?,
        None => 0,
    };

    Some((major, minor, patch, build))
}

/// Whether `candidate` is a later build than `than`.
///
/// Anything unparseable is never newer, so a malformed answer from upstream
/// cannot be offered as an update.
pub fn is_newer(candidate: &str, than: &str) -> bool {
    match (parse_version(candidate), parse_version(than)) {
        (Some(candidate), Some(than)) => candidate > than,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Builds installed under `root`, newest first.
///
/// A directory counts only when it holds the server and its name is a version:
/// a half-finished install has no server yet, and anything else in here is not
/// something to run.
pub fn installed_in(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join(ENTRY).is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| parse_version(name).is_some())
        .collect();

    found.sort_by_key(|name| std::cmp::Reverse(parse_version(name)));
    found
}

/// The build to run from `root`: the one `active` names when it is still
/// installed, otherwise the newest installed.
pub fn active_in(root: &Path) -> Option<String> {
    let installed = installed_in(root);

    let named = std::fs::read_to_string(root.join(ACTIVE_FILE))
        .ok()
        .map(|text| text.trim().to_string());

    match named {
        Some(named) if installed.contains(&named) => Some(named),
        _ => installed.into_iter().next(),
    }
}

/// Writes `active`, through a sibling and a rename so a crash mid-write cannot
/// leave a truncated name that matches nothing.
pub fn set_active_in(root: &Path, version: &str) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let staged = root.join(".active.partial");
    std::fs::write(&staged, version)?;
    fetch::commit(&staged, &root.join(ACTIVE_FILE))
}

/// [`sweep`] over a plain directory.
pub fn sweep_in(root: &Path, keep: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name == keep {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => removed.push(name),
            Err(err) => crate::say!("could not remove whisper.cpp {name} yet: {err}"),
        }
    }
    removed
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
            "Release/parakeet.dll",
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
    fn the_baseline_version_parses_with_its_build_number() {
        // A bare build number would not parse, and `installed_in` would then
        // skip the directory it installs to, forever.
        assert_eq!(parse_version(&baseline().version), Some((1, 9, 3, 4938)));
    }

    #[test]
    fn the_baseline_digest_is_a_prefixed_sha256() {
        let digest = baseline().sha256;
        assert!(digest.starts_with("sha256:"));
        assert_eq!(digest.len(), "sha256:".len() + 64);
    }

    // ── Ordering ────────────────────────────────────────────────────────────

    #[test]
    fn a_later_release_is_newer() {
        assert!(is_newer("1.9.4+b5130", "1.9.3+b4938"));
        assert!(!is_newer("1.9.3+b4938", "1.9.4+b5130"));
    }

    #[test]
    fn the_build_number_orders_two_builds_of_one_version() {
        // Semver would call these equal, which would hide a rebuilt release.
        assert!(is_newer("1.9.4+b5140", "1.9.4+b5130"));
    }

    #[test]
    fn a_build_is_not_newer_than_itself() {
        assert!(!is_newer("1.9.4+b5130", "1.9.4+b5130"));
    }

    #[test]
    fn numbers_compare_as_numbers_not_text() {
        // As text, "1.10.0" sorts before "1.9.9".
        assert!(is_newer("1.10.0+b1", "1.9.9+b9999"));
    }

    #[test]
    fn a_malformed_candidate_is_never_newer() {
        for bad in ["v1.9.4", "b5130", "1.9", "1.9.4.1", "1.9.4+5130", "", "latest"] {
            assert!(
                !is_newer(bad, "1.9.3+b4938"),
                "{bad:?} must not be offered as an update"
            );
        }
    }

    // ── Which build runs ────────────────────────────────────────────────────

    fn install_fake(root: &Path, version: &str) {
        let dir = root.join(version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(ENTRY), b"").unwrap();
    }

    #[test]
    fn with_no_active_file_the_newest_installed_build_runs() {
        // What an install from before `active` existed looks like.
        let root = tempfile::tempdir().unwrap();
        install_fake(root.path(), "1.9.3+b4938");
        install_fake(root.path(), "1.9.4+b5130");

        assert_eq!(active_in(root.path()).as_deref(), Some("1.9.4+b5130"));
    }

    #[test]
    fn the_active_file_wins_over_a_newer_build_on_disk() {
        // A newer build is downloaded and checked before it is proven. Until
        // it has loaded a model, the one that works is the one that runs.
        let root = tempfile::tempdir().unwrap();
        install_fake(root.path(), "1.9.3+b4938");
        install_fake(root.path(), "1.9.4+b5130");
        set_active_in(root.path(), "1.9.3+b4938").unwrap();

        assert_eq!(active_in(root.path()).as_deref(), Some("1.9.3+b4938"));
    }

    #[test]
    fn an_active_file_naming_a_removed_build_falls_back_to_what_is_there() {
        let root = tempfile::tempdir().unwrap();
        install_fake(root.path(), "1.9.3+b4938");
        set_active_in(root.path(), "1.9.4+b5130").unwrap();

        assert_eq!(active_in(root.path()).as_deref(), Some("1.9.3+b4938"));
    }

    #[test]
    fn a_directory_without_the_server_is_not_installed() {
        // A download interrupted before unpacking leaves the directory and
        // its partial archive, and must not be picked to run.
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("1.9.4+b5130")).unwrap();

        assert!(installed_in(root.path()).is_empty());
        assert_eq!(active_in(root.path()), None);
    }

    #[test]
    fn nothing_installed_means_nothing_runs() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(active_in(root.path()), None);
        assert_eq!(active_in(&root.path().join("missing")), None);
    }

    #[test]
    fn the_sweep_keeps_one_build_and_removes_the_rest() {
        let root = tempfile::tempdir().unwrap();
        install_fake(root.path(), "1.9.2+b4800");
        install_fake(root.path(), "1.9.3+b4938");
        install_fake(root.path(), "1.9.4+b5130");
        std::fs::create_dir_all(root.path().join("1.9.5+b5200").join(".staging")).unwrap();
        set_active_in(root.path(), "1.9.4+b5130").unwrap();

        let mut removed = sweep_in(root.path(), "1.9.4+b5130");
        removed.sort();

        assert_eq!(removed, ["1.9.2+b4800", "1.9.3+b4938", "1.9.5+b5200"]);
        assert_eq!(installed_in(root.path()), ["1.9.4+b5130"]);
        assert!(
            root.path().join(ACTIVE_FILE).is_file(),
            "the pointer is a file, not a build, and must survive"
        );
    }

    // ── Layout ──────────────────────────────────────────────────────────────

    fn unpacked(names: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        dir
    }

    #[test]
    fn a_complete_build_is_missing_nothing() {
        let dir = unpacked(&[
            "whisper-server.exe",
            "whisper.dll",
            "ggml.dll",
            "ggml-base.dll",
            "ggml-cpu-haswell.dll",
        ]);
        assert_eq!(missing_from(dir.path()), None);
    }

    #[test]
    fn a_build_without_its_library_says_which_one() {
        let dir = unpacked(&["whisper-server.exe", "ggml.dll", "ggml-base.dll", "ggml-cpu-x64.dll"]);
        assert_eq!(missing_from(dir.path()).as_deref(), Some("whisper.dll"));
    }

    #[test]
    fn a_build_without_any_cpu_backend_is_incomplete() {
        // `GGML_BACKEND_DL` loads one of these at runtime; with none, the
        // server starts and then cannot compute anything.
        let dir = unpacked(&["whisper-server.exe", "whisper.dll", "ggml.dll", "ggml-base.dll"]);
        assert_eq!(missing_from(dir.path()).as_deref(), Some("ggml-cpu-*.dll"));
    }

    // ── Options ─────────────────────────────────────────────────────────────

    /// The option lines of `whisper-server --help`, as b5130 prints them.
    const HELP: &str = "\
  -h,        --help                      [default] show this help message and exit
  -t N,      --threads N                 [4      ] number of threads to use during computation
  -tp,       --temperature-inc N         [0.20   ] The increment of temperature
  -nt,       --no-timestamps             [false  ] do not print timestamps
  -m FNAME,  --model FNAME               [models/ggml-base.en.bin] model path
  --host HOST,                           [127.0.0.1] Hostname/ip-adress for the server
  --port PORT,                           [8080   ] Port number for the server
";

    #[test]
    fn every_option_sill_passes_is_in_the_help() {
        assert!(missing_flags(HELP, &flags_sill_passes()).is_empty());
    }

    #[test]
    fn the_flags_checked_are_the_flags_the_server_is_started_with() {
        // Taken from `server_args` so a flag added there is checked here
        // without anybody remembering to.
        let flags = flags_sill_passes();
        for flag in ["-m", "--host", "--port", "-t", "-nt"] {
            assert!(flags.iter().any(|f| f == flag), "{flag} missing from {flags:?}");
        }
    }

    #[test]
    fn a_renamed_option_is_reported() {
        let renamed = HELP.replace("-nt,", "-nots,");
        assert_eq!(
            missing_flags(&renamed, &flags_sill_passes()),
            ["-nt".to_string()]
        );
    }

    #[test]
    fn an_option_is_matched_whole_not_as_a_prefix() {
        // `-t` must not be found inside `-tp`.
        let help = "-tp, --temperature-inc N";
        assert_eq!(missing_flags(help, &["-t".to_string()]), ["-t".to_string()]);
    }

    /// Runs the real `--help` probe against an installed build.
    ///
    /// ```text
    /// $env:SILL_WHISPER_SERVER = "C:\...\whisper-server.exe"
    /// cargo test --lib dictation::engine::tests::probe_accepts -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs whisper-server on disk"]
    async fn probe_accepts_a_real_build() {
        let exe = std::env::var("SILL_WHISPER_SERVER").expect("set SILL_WHISPER_SERVER");
        probe(Path::new(&exe)).await.expect("the build should pass");
    }

    /// The whole install against upstream's current release, into a
    /// throwaway directory: resolve, download, digest, unpack, layout, probe.
    ///
    /// ```text
    /// cargo test --lib dictation::engine::tests::installs_upstream -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "downloads whisper.cpp from GitHub"]
    async fn installs_upstreams_current_release() {
        let build = crate::dictation::upstream::latest(&fetch::client(), None)
            .await
            .expect("upstream should resolve");
        println!("{build:?}");

        let root = tempfile::tempdir().unwrap();
        let engine = install_in(root.path(), &build, |_, _| {})
            .await
            .expect("the current release should install and pass its checks");

        assert!(engine.exe.is_file());
        assert_eq!(installed_in(root.path()), [build.version.clone()]);
        assert_eq!(active_in(root.path()), Some(build.version));
        println!("installed {}", engine.exe.display());
    }

    #[tokio::test]
    #[ignore = "downloads whisper.cpp from GitHub"]
    async fn a_download_that_fails_its_digest_leaves_nothing_behind() {
        let mut build = baseline();
        build.sha256 = format!("sha256:{}", "0".repeat(64));

        let root = tempfile::tempdir().unwrap();
        let err = install_in(root.path(), &build, |_, _| {})
            .await
            .expect_err("a wrong digest must not install");

        assert!(err.to_string().contains("checksum"), "{err}");
        assert!(
            !root.path().join(&build.version).exists(),
            "a rejected build must not sit on disk looking installed"
        );
    }
}
