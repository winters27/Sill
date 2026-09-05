//! Whether there is a newer Sill, and the one place both windows read it from.
//!
//! Sill replaces its own executable when it updates, so whatever can answer
//! for the update URL gets to run code on this machine. That is the highest
//! consequence in the application and it is why the endpoint is signed: the
//! public key is compiled in from `tauri.conf.json`, the secret key exists
//! only in CI, and a bundle that does not verify is not installed. A checksum
//! alone would not do, because the checksum and the download come from the
//! same host.
//!
//! ## Why it does not run in the background
//!
//! The constitution is explicit that an idle launcher must not poll, and that
//! every background system has to answer why it is waking up. A daily timer
//! has no answer. So nothing here has a thread, a timer or a task: the check
//! happens when the window is summoned, which is a moment the user created,
//! and at most once a day after that. Sitting closed, this module costs a
//! `Mutex` around a small enum.
//!
//! ## Why nothing is downloaded until it is asked for
//!
//! An installer is tens of megabytes and Sill idles at a hundred and change.
//! Holding a download in memory against the chance that somebody presses the
//! button would be a permanent cost for an occasional event, which is the
//! trade the constitution refuses. So a found update is a version number and
//! release notes; the bytes arrive when the button is pressed, and the chin
//! shows them arriving.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// How long an answer is good for.
///
/// A day, because a launcher is opened dozens of times a day and none of those
/// is a reason to ask again. The first summon after this has passed pays for a
/// request that is usually a 404 or a "same version", and nothing else does.
const GOOD_FOR: Duration = Duration::from_secs(60 * 60 * 24);

/// Where the check has got to, as the two windows draw it.
///
/// One enum rather than a set of booleans, because the states are exclusive
/// and a struct of flags is how a surface ends up claiming to be downloading
/// something it already installed. `serde` writes it as a tagged object so the
/// TypeScript side is a discriminated union and the compiler can require every
/// case to be handled, which is the shape `RootList` had to be corrected into.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Progress {
    /// Nothing has been asked yet. The state at startup, and after a failure
    /// has been read, so a window that opens does not claim to know.
    Unknown,
    /// Asked, and this is the newest there is.
    UpToDate,
    /// There is a newer one, and nothing has been downloaded.
    Available {
        version: String,
        /// The release notes, as the release itself wrote them.
        notes: Option<String>,
    },
    /// The bytes are arriving, because somebody pressed the button.
    ///
    /// `percent` is `None` while the server has not said how big it is, which
    /// is common enough that a bar has to cope rather than sit at zero.
    Downloading { version: String, percent: Option<u8> },
    /// Downloaded and verified. The next step closes Sill and runs it.
    Ready { version: String },
    /// The check or the download did not work.
    ///
    /// Kept and shown in settings rather than raised as a toast: a failed
    /// update check is not something to interrupt somebody who just opened a
    /// launcher to run a command. It is a state they can go and read, which is
    /// the same rule `status.rs` follows.
    Failed { why: String },
}

/// What the windows are told, which is the state plus the version they are on.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub progress: Progress,
    /// The running build, so About can say what it is without a second call.
    pub current: String,
    /// Whether an answer is fresh enough that summoning will not re-ask.
    pub checked_recently: bool,
}

/// The one place the answer lives.
///
/// A managed service rather than a `static`, which is what rule 2 refuses, and
/// the same shape as `Status` next door.
pub struct Updates {
    inner: Mutex<Held>,
}

struct Held {
    progress: Progress,
    /// When the last check finished, successful or not.
    ///
    /// A failure counts, or a machine with no network would ask again on every
    /// single summon and make the launcher wait on a socket each time.
    asked_at: Option<Instant>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Held {
                progress: Progress::Unknown,
                asked_at: None,
            }),
        }
    }
}

impl Updates {
    /// The current state, for a window that has just opened.
    pub fn read(&self) -> (Progress, bool) {
        let held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        (
            held.progress.clone(),
            held.asked_at.is_some_and(|at| at.elapsed() < GOOD_FOR),
        )
    }

    /// Records where the check has got to, and says whether that is news.
    ///
    /// The answer is what stops an event per downloaded chunk. A percentage
    /// that has not moved is not worth waking two windows for, and rounding to
    /// whole percent before comparing is what makes that true in practice.
    fn set(&self, progress: Progress) -> bool {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if held.progress == progress {
            return false;
        }
        held.progress = progress;
        true
    }

    /// Whether a check would be worth making now.
    ///
    /// False while a download is in flight, because asking again mid-download
    /// would replace the state the progress bar is reading from.
    fn worth_asking(&self, force: bool) -> bool {
        let held = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        if matches!(held.progress, Progress::Downloading { .. }) {
            return false;
        }

        if force {
            return true;
        }

        // A found update stays found. Re-asking would only replace it with
        // itself, and the button on the chin is about to use it.
        if matches!(
            held.progress,
            Progress::Available { .. } | Progress::Ready { .. }
        ) {
            return false;
        }

        held.asked_at.is_none_or(|at| at.elapsed() >= GOOD_FOR)
    }

    fn asked_now(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).asked_at = Some(Instant::now());
    }
}

/// Tells both windows, but only when something actually changed.
///
/// A plain `emit` rather than one scoped to a label, deliberately. The scoping
/// rule exists for messages about one window, like `sill://hidden`; this is a
/// fact about the application and the launcher and the settings window both
/// want it.
fn announce(app: &AppHandle, progress: Progress) {
    let updates = app.state::<Updates>();
    if updates.set(progress.clone()) {
        let _ = app.emit("sill://update-changed", progress);
    }
}

/// The running version, as the manifest declares it.
pub fn current(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Asks whether there is a newer Sill, unless one was asked for recently.
///
/// Called on summon and by the button in settings. Everything about it is
/// cheap when the answer is already known: `worth_asking` returns false and no
/// socket is opened.
pub async fn check(app: AppHandle, force: bool) {
    if !app.state::<Updates>().worth_asking(force) {
        return;
    }

    // Marked before the request rather than after, so two summons a second
    // apart cannot both get past the gate and open two connections.
    app.state::<Updates>().asked_now();

    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(why) => {
            // No endpoint configured, which is what a development build looks
            // like. Worth saying in settings and worth saying nowhere else.
            announce(&app, Progress::Failed { why: why.to_string() });
            return;
        }
    };

    match updater.check().await {
        Ok(Some(found)) => announce(
            &app,
            Progress::Available {
                version: found.version.clone(),
                notes: found.body.clone(),
            },
        ),
        Ok(None) => announce(&app, Progress::UpToDate),
        Err(why) => announce(&app, Progress::Failed { why: why.to_string() }),
    }
}

/// Downloads the newer Sill and runs its installer.
///
/// The download and the install are one call because the plugin verifies the
/// signature between them, and splitting them would mean holding an unverified
/// installer somewhere. `percent` is reported as it arrives so the chin can
/// show something moving, which for a fifty megabyte download over a slow line
/// is the difference between working and hung.
pub async fn install(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;

    let found = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "there is no newer version to install".to_string())?;

    let version = found.version.clone();
    announce(
        &app,
        Progress::Downloading {
            version: version.clone(),
            percent: None,
        },
    );

    let mut taken: u64 = 0;
    let progress_app = app.clone();
    let progress_version = version.clone();

    let outcome = found
        .download_and_install(
            move |chunk, total| {
                taken += chunk as u64;
                // Whole percent only. Announcing every chunk would wake both
                // windows thousands of times for a bar that moves in steps of
                // one, and `announce` drops a repeat anyway.
                let percent = total.map(|total| {
                    if total == 0 {
                        0
                    } else {
                        ((taken.saturating_mul(100)) / total).min(100) as u8
                    }
                });
                announce(
                    &progress_app,
                    Progress::Downloading {
                        version: progress_version.clone(),
                        percent,
                    },
                );
            },
            || {},
        )
        .await;

    match outcome {
        Ok(()) => {
            // Reached only if the installer did not already take the process
            // down, which on Windows it usually does. Saying so is still
            // right: the state a window would draw between the two is "ready",
            // never "still downloading".
            announce(&app, Progress::Ready { version });
            Ok(())
        }
        Err(why) => {
            let why = why.to_string();
            announce(&app, Progress::Failed { why: why.clone() });
            Err(why)
        }
    }
}

/// The state, for a window that has just opened and wants to draw.
pub fn state(app: &AppHandle) -> UpdateState {
    let (progress, checked_recently) = app.state::<Updates>().read();
    UpdateState {
        progress,
        current: current(app),
        checked_recently,
    }
}
