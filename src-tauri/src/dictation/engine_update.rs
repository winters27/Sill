//! Keeping the whisper engine current, on upstream's cadence rather than
//! Sill's.
//!
//! A newer whisper.cpp is found by asking upstream (see [`upstream`]), offered
//! in the dictation settings, and installed only when somebody presses the
//! button. Nothing here applies itself: replacing the engine restarts the
//! server and reloads the model, which is not something to do to somebody in
//! the middle of their day without asking.
//!
//! ## Why there is no timer
//!
//! The check runs when the local server's settings are on screen, which is the
//! only moment anybody could act on the answer, and not again for
//! [`ASK_AGAIN_AFTER`]. The answer is kept on disk so a restart does not ask
//! again. A machine where nobody opens those settings never asks at all, and
//! one with dictation off or on a cloud provider never reaches the network
//! from here.
//!
//! ## What stands in for reviewing a build
//!
//! Upstream builds are not measured by hand before they are offered, so each
//! one proves itself on the machine before it replaces anything: the archive
//! matches the digest GitHub publishes, every library the server loads is in
//! it, `--help` still lists every option Sill passes, and the server then loads
//! the model in use and answers. Only after all of that is it made active and
//! the old build deleted. A build that fails is remembered, with the reason,
//! so it is not offered as though nothing had happened.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::dictation::engine::{self, Build};
use crate::dictation::error::{DictationError, Result};
use crate::dictation::models::SetupProgress;
use crate::dictation::server::{SwitchFailed, WhisperServer};
use crate::dictation::service::DictationService;
use crate::dictation::{assets, fetch, upstream};

/// How long an answer is good for.
///
/// Six hours, like the extension store's: whisper.cpp releases every few
/// weeks, and the settings panel is opened far more often than that.
pub const ASK_AGAIN_AFTER: i64 = 6 * 60 * 60;

/// The shape of the file on disk. Bumped when [`Known`] changes.
const FORMAT: u32 = 1;

/// A build that did not work on this machine, and what went wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rejected {
    pub version: String,
    pub reason: String,
}

/// What upstream last said, and when.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Known {
    pub format: u32,
    /// Seconds since the epoch of the last answer, or zero for never.
    pub checked_at: i64,
    /// Upstream's current release build.
    pub latest: Option<Build>,
    pub rejected: Option<Rejected>,
    /// Why the last attempt to ask did not get an answer.
    pub error: Option<String>,
}

impl Known {
    /// Whether the answer is recent enough not to ask again.
    ///
    /// A file from an older Sill is stale whatever its timestamp says, and a
    /// clock that has gone backwards reads as stale rather than as fresh for
    /// the next six hours.
    pub fn is_fresh(&self, now: i64) -> bool {
        self.format == FORMAT
            && self.checked_at > 0
            && now >= self.checked_at
            && now - self.checked_at < ASK_AGAIN_AFTER
    }
}

/// Where the answer is kept: beside the builds it is about.
fn known_path(root: &Path) -> PathBuf {
    root.join("upstream.json")
}

/// The last answer, or an empty one. An unreadable file costs one check.
pub fn load(root: &Path) -> Known {
    std::fs::read_to_string(known_path(root))
        .ok()
        .and_then(|text| serde_json::from_str::<Known>(&text).ok())
        .filter(|known| known.format == FORMAT)
        .unwrap_or_default()
}

pub fn save(root: &Path, known: &Known) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let text = serde_json::to_string_pretty(known)
        .map_err(|e| DictationError::Other(format!("could not write the engine check: {e}")))?;
    let staged = root.join(".upstream.json.partial");
    std::fs::write(&staged, format!("{text}\n"))?;
    fetch::commit(&staged, &known_path(root))
}

/// What the settings card draws about the engine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStanding {
    /// The build that runs, or `None` before setup.
    pub active: Option<String>,
    /// A newer build than the one running.
    pub update: Option<Build>,
    /// Set when that newer build already failed here, and why.
    pub rejected: Option<Rejected>,
    /// Why the last check did not get an answer.
    pub error: Option<String>,
}

/// The standing, from what is installed and what upstream last said.
///
/// Nothing is offered before setup: a first install fetches the newest build
/// itself, so an offer then would be an update to something that is not there.
pub fn standing(active: Option<&str>, known: &Known) -> EngineStanding {
    let update = active.and_then(|active| {
        known
            .latest
            .clone()
            .filter(|build| engine::is_newer(&build.version, active))
    });

    let rejected = known
        .rejected
        .clone()
        .filter(|rejected| update.as_ref().is_some_and(|build| build.version == rejected.version));

    EngineStanding {
        active: active.map(str::to_string),
        update,
        rejected,
        error: known.error.clone(),
    }
}

/// The engine's state, held for the run. Registered as Tauri managed state.
///
/// The settings panel reads the standing every two seconds while it is open,
/// so what is installed and what upstream said are both cached here and only
/// re-read after something changes them. Sitting closed, this is two empty
/// mutexes.
#[derive(Default)]
pub struct EngineUpdates {
    known: Mutex<Option<Known>>,
    active: Mutex<Option<Option<String>>>,
    /// One install or update at a time. They download into the same place and
    /// both end by choosing what runs.
    working: tokio::sync::Mutex<()>,
}

impl EngineUpdates {
    /// The build that runs.
    pub fn active(&self, app: &AppHandle) -> Option<String> {
        if let Ok(cached) = self.active.lock() {
            if let Some(active) = cached.as_ref() {
                return active.clone();
            }
        }

        let active = engine::active(app).map(|engine| engine.version);
        if let Ok(mut cached) = self.active.lock() {
            *cached = Some(active.clone());
        }
        active
    }

    /// Drops the cached answer, after something changed what is installed.
    fn installed_changed(&self) {
        if let Ok(mut cached) = self.active.lock() {
            *cached = None;
        }
    }

    fn known(&self, root: &Path) -> Known {
        if let Ok(cached) = self.known.lock() {
            if let Some(known) = cached.as_ref() {
                return known.clone();
            }
        }

        let known = load(root);
        if let Ok(mut cached) = self.known.lock() {
            *cached = Some(known.clone());
        }
        known
    }

    fn remember(&self, root: &Path, known: Known) {
        if let Err(err) = save(root, &known) {
            crate::say!("could not keep the engine check: {err}");
        }
        if let Ok(mut cached) = self.known.lock() {
            *cached = Some(known);
        }
    }

    fn reject(&self, root: &Path, build: &Build, err: &DictationError) {
        crate::say!("whisper.cpp {} did not work here: {err}", build.version);
        let mut known = self.known(root);
        known.format = FORMAT;
        known.rejected = Some(Rejected {
            version: build.version.clone(),
            reason: err.to_string(),
        });
        self.remember(root, known);
    }

    /// The standing right now. Never asks anything.
    pub fn standing(&self, app: &AppHandle) -> EngineStanding {
        let known = engine::engines_dir(app)
            .map(|root| self.known(&root))
            .unwrap_or_default();
        standing(self.active(app).as_deref(), &known)
    }

    /// Bytes a first setup would download for the engine.
    pub fn first_download_bytes(&self, app: &AppHandle) -> u64 {
        engine::engines_dir(app)
            .ok()
            .and_then(|root| self.known(&root).latest)
            .map_or(engine::baseline().bytes, |build| build.bytes)
    }
}

/// Whether dictation runs on the server Sill starts, which is the only case
/// where the engine is Sill's business.
fn runs_locally(app: &AppHandle) -> bool {
    let settings = app.state::<DictationService>().settings();
    let pointed_elsewhere = settings
        .provider
        .base_url
        .as_deref()
        .map(str::trim)
        .is_some_and(|base| !base.is_empty());
    settings.provider_id == "local" && !pointed_elsewhere
}

/// The token the store uses, when there is one, which lifts GitHub's limit
/// from sixty requests an hour to five thousand.
async fn github_token(app: &AppHandle) -> Option<String> {
    let held = app.try_state::<crate::state::PrefsState>()?.inner.clone();
    let prefs = held.lock().await;
    prefs
        .store
        .github_token
        .clone()
        .filter(|token| !token.trim().is_empty())
}

/// Asks upstream for its newest build, unless the last answer is still good.
///
/// `force` is somebody asking to look again, which is entitled to a fresh
/// answer. Nothing is asked for an engine that is not installed, or not the
/// one dictation uses.
pub async fn check(app: &AppHandle, force: bool) -> EngineStanding {
    let updates = app.state::<EngineUpdates>();

    let Ok(root) = engine::engines_dir(app) else {
        return updates.standing(app);
    };
    if !runs_locally(app) || updates.active(app).is_none() {
        return updates.standing(app);
    }
    if !force && updates.known(&root).is_fresh(crate::state::now_seconds()) {
        return updates.standing(app);
    }
    // An install or update is running and will leave its own answer.
    let Ok(_working) = updates.working.try_lock() else {
        return updates.standing(app);
    };

    let token = github_token(app).await;
    let asked = upstream::latest(&fetch::client(), token.as_deref()).await;

    let mut known = updates.known(&root);
    known.format = FORMAT;
    match asked {
        Ok(build) => {
            known.checked_at = crate::state::now_seconds();
            known.latest = Some(build);
            known.error = None;
        }
        // The timestamp stays put, so the next time the panel opens asks
        // again rather than waiting six hours on a network that has come back.
        Err(err) => {
            crate::say!("could not ask which whisper.cpp is current: {err}");
            known.error = Some(err);
        }
    }
    updates.remember(&root, known);

    updates.standing(app)
}

/// Replaces the running engine with the newer build the last check found.
///
/// `retry` installs a build that already failed here, for somebody who wants
/// to try it again after fixing whatever it was.
pub async fn apply(app: &AppHandle, retry: bool) -> Result<()> {
    let updates = app.state::<EngineUpdates>();
    let Ok(_working) = updates.working.try_lock() else {
        return Err(DictationError::Validation(
            "whisper.cpp is already being installed".to_string(),
        ));
    };

    let root = engine::engines_dir(app)?;
    let known = updates.known(&root);
    let active = updates
        .active(app)
        .ok_or_else(|| DictationError::NotFound("whisper.cpp is not installed yet".to_string()))?;

    let build = known
        .latest
        .clone()
        .filter(|build| engine::is_newer(&build.version, &active))
        .ok_or_else(|| {
            DictationError::Validation(format!("whisper.cpp {active} is already the newest"))
        })?;

    if let Some(rejected) = known.rejected.as_ref().filter(|r| r.version == build.version) {
        if !retry {
            return Err(DictationError::Validation(rejected.reason.clone()));
        }
    }

    SetupProgress::Engine.emit(app);
    let installed = engine::install(app, &build, |done, total| {
        SetupProgress::EngineDownload {
            bytes_downloaded: done,
            total_bytes: total,
        }
        .emit(app);
    })
    .await;

    let new = match installed {
        Ok(new) => new,
        Err(err) => {
            updates.reject(&root, &build, &err);
            return Err(err);
        }
    };

    if let Err(err) = prove(app, &new).await {
        return Err(match err {
            // Nothing is wrong with the build; the old server is still
            // running and still serving.
            SwitchFailed::NotTried(err) => err,
            SwitchFailed::Failed(err) => {
                let _ = engine::remove(app, &build.version);
                updates.reject(&root, &build, &err);
                err
            }
        });
    }

    engine::activate(app, &build.version)?;
    updates.installed_changed();

    let mut known = updates.known(&root);
    known.rejected = None;
    updates.remember(&root, known);

    let removed = engine::sweep(app, &build.version);
    crate::say!(
        "whisper.cpp is now {}; removed {}",
        build.version,
        if removed.is_empty() {
            "nothing".to_string()
        } else {
            removed.join(", ")
        }
    );
    Ok(())
}

/// Starts a server on `new` with a real model, putting the old one back if it
/// does not come up.
///
/// The model is the one already loaded when a server is running, otherwise
/// the one selected. With no model downloaded at all there is nothing to load,
/// and the `--help` check [`engine::install`] ran is all the proof there is.
async fn prove(app: &AppHandle, new: &engine::Engine) -> std::result::Result<(), SwitchFailed> {
    let server = app.state::<WhisperServer>();
    let running = server.snapshot().map(|live| live.model_id);

    let model_id = running
        .clone()
        .or_else(|| Some(app.state::<DictationService>().settings().model_id))
        .filter(|id| assets::is_installed(app, id));
    let Some(model_id) = model_id else {
        return Ok(());
    };

    SetupProgress::Starting.emit(app);
    match server.switch(app, &new.exe, &model_id).await {
        Ok(()) => Ok(()),
        Err(SwitchFailed::NotTried(err)) => Err(SwitchFailed::NotTried(err)),
        Err(SwitchFailed::Failed(err)) => {
            // The old server was stopped to make room. Bring it back if it
            // was up, so a failed update costs nothing but the attempt.
            if let Some(model_id) = running {
                if let Err(back) = server.ensure(app, &model_id).await {
                    crate::say!("could not restart the previous whisper.cpp: {back}");
                }
            }
            Err(SwitchFailed::Failed(err))
        }
    }
}

/// Everything local dictation needs, then the server started.
///
/// A first install takes upstream's newest build, falling back to the one
/// measured by hand when GitHub cannot be asked or the newest does not work
/// here. So setup ends with dictation working even when upstream has
/// published something broken.
pub async fn setup(app: &AppHandle, model_id: &str) -> Result<()> {
    let updates = app.state::<EngineUpdates>();
    let server = app.state::<WhisperServer>();

    let fresh = {
        let _working = updates.working.lock().await;
        if updates.active(app).is_some() {
            None
        } else {
            Some(install_first(app, &updates).await?)
        }
    };

    assets::ensure(app, model_id).await?;

    // Started here rather than on the first dictation, so the model load is
    // paid while the user is looking at a progress indicator instead of
    // while they are holding a hotkey down waiting to speak.
    SetupProgress::Starting.emit(app);
    let started = server.ensure(app, model_id).await.map(drop);

    match (started, fresh) {
        (Ok(()), _) => Ok(()),
        // The build just installed from upstream would not load a model.
        // Swap it for the measured one rather than leave setup failed.
        (Err(err), Some(build)) if build.version != engine::baseline().version => {
            let _working = updates.working.lock().await;
            let root = engine::engines_dir(app)?;
            updates.reject(&root, &build, &err);
            let _ = engine::remove(app, &build.version);

            let baseline = engine::baseline();
            install_one(app, &baseline).await?;
            engine::activate(app, &baseline.version)?;
            updates.installed_changed();

            SetupProgress::Starting.emit(app);
            server.ensure(app, model_id).await.map(drop)
        }
        (Err(err), _) => Err(err),
    }
}

/// Installs and activates the first engine this machine has. Returns the
/// build that went in.
async fn install_first(app: &AppHandle, updates: &EngineUpdates) -> Result<Build> {
    let root = engine::engines_dir(app)?;
    let token = github_token(app).await;

    let mut candidates = Vec::new();
    match upstream::latest(&fetch::client(), token.as_deref()).await {
        Ok(build) => {
            let mut known = updates.known(&root);
            known.format = FORMAT;
            known.checked_at = crate::state::now_seconds();
            known.error = None;
            let failed_before = known
                .rejected
                .as_ref()
                .is_some_and(|rejected| rejected.version == build.version);
            known.latest = Some(build.clone());
            updates.remember(&root, known);
            if !failed_before {
                candidates.push(build);
            }
        }
        Err(err) => crate::say!("installing the measured whisper.cpp, since GitHub said: {err}"),
    }

    let baseline = engine::baseline();
    if !candidates.iter().any(|build| build.version == baseline.version) {
        candidates.push(baseline);
    }

    let mut last = None;
    for build in candidates {
        match install_one(app, &build).await {
            Ok(()) => {
                engine::activate(app, &build.version)?;
                updates.installed_changed();
                return Ok(build);
            }
            Err(err) => {
                updates.reject(&root, &build, &err);
                last = Some(err);
            }
        }
    }

    Err(last.unwrap_or_else(|| DictationError::Other("No whisper.cpp build to install".into())))
}

async fn install_one(app: &AppHandle, build: &Build) -> Result<()> {
    SetupProgress::Engine.emit(app);
    engine::install(app, build, |done, total| {
        SetupProgress::EngineDownload {
            bytes_downloaded: done,
            total_bytes: total,
        }
        .emit(app);
    })
    .await
    .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(version: &str) -> Build {
        Build {
            version: version.to_string(),
            url: format!("https://github.com/ggml-org/whisper.cpp/releases/download/x/{version}.zip"),
            sha256: format!("sha256:{}", "0".repeat(64)),
            bytes: 8_573_270,
        }
    }

    fn known_with(latest: &str) -> Known {
        Known {
            format: FORMAT,
            checked_at: 1_000,
            latest: Some(build(latest)),
            ..Default::default()
        }
    }

    #[test]
    fn a_newer_build_is_offered() {
        let standing = standing(Some("1.9.3+b4938"), &known_with("1.9.4+b5130"));
        assert_eq!(standing.update.map(|b| b.version).as_deref(), Some("1.9.4+b5130"));
        assert_eq!(standing.rejected, None);
    }

    #[test]
    fn the_running_build_is_not_offered_to_itself() {
        let standing = standing(Some("1.9.4+b5130"), &known_with("1.9.4+b5130"));
        assert_eq!(standing.update, None);
    }

    #[test]
    fn an_older_build_upstream_is_not_offered() {
        // Upstream pulling a release must not read as a downgrade to install.
        let standing = standing(Some("1.9.4+b5130"), &known_with("1.9.3+b4938"));
        assert_eq!(standing.update, None);
    }

    #[test]
    fn nothing_is_offered_before_setup() {
        let standing = standing(None, &known_with("1.9.4+b5130"));
        assert_eq!(standing.update, None);
        assert_eq!(standing.active, None);
    }

    #[test]
    fn a_build_that_failed_here_says_so() {
        let mut known = known_with("1.9.4+b5130");
        known.rejected = Some(Rejected {
            version: "1.9.4+b5130".to_string(),
            reason: "This whisper.cpp no longer accepts -nt".to_string(),
        });

        let standing = standing(Some("1.9.3+b4938"), &known);
        assert!(standing.update.is_some(), "still offered, so it can be retried");
        assert_eq!(
            standing.rejected.map(|r| r.reason).as_deref(),
            Some("This whisper.cpp no longer accepts -nt")
        );
    }

    #[test]
    fn a_failure_is_forgotten_once_something_newer_is_published() {
        // A broken 1.9.4 must not hang over 1.9.5.
        let mut known = known_with("1.9.5+b5200");
        known.rejected = Some(Rejected {
            version: "1.9.4+b5130".to_string(),
            reason: "broken".to_string(),
        });

        assert_eq!(standing(Some("1.9.3+b4938"), &known).rejected, None);
    }

    #[test]
    fn an_answer_is_fresh_for_six_hours() {
        let known = known_with("1.9.4+b5130");
        assert!(known.is_fresh(1_000));
        assert!(known.is_fresh(1_000 + ASK_AGAIN_AFTER - 1));
        assert!(!known.is_fresh(1_000 + ASK_AGAIN_AFTER));
    }

    #[test]
    fn a_clock_that_went_backwards_reads_as_stale() {
        assert!(!known_with("1.9.4+b5130").is_fresh(999));
    }

    #[test]
    fn never_having_asked_is_stale() {
        assert!(!Known::default().is_fresh(1_000));
    }

    #[test]
    fn the_answer_survives_a_restart() {
        let root = tempfile::tempdir().unwrap();
        let mut known = known_with("1.9.4+b5130");
        known.rejected = Some(Rejected {
            version: "1.9.4+b5130".to_string(),
            reason: "broken".to_string(),
        });

        save(root.path(), &known).unwrap();
        assert_eq!(load(root.path()), known);
    }

    #[test]
    fn the_answer_file_is_not_mistaken_for_a_build() {
        // It lives beside the version directories.
        let root = tempfile::tempdir().unwrap();
        save(root.path(), &known_with("1.9.4+b5130")).unwrap();
        assert!(engine::installed_in(root.path()).is_empty());
        assert!(engine::sweep_in(root.path(), "1.9.4+b5130").is_empty());
        assert!(known_path(root.path()).is_file());
    }

    #[test]
    fn a_file_from_another_format_is_ignored() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(known_path(root.path()), r#"{"format":99,"checkedAt":5}"#).unwrap();
        assert_eq!(load(root.path()), Known::default());
    }
}
