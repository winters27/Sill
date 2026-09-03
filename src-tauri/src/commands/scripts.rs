//! Running a script from the launcher, watching it, and stopping it.
//!
//! ## Why this is not the action
//!
//! `sill.script.run` runs a script and reports one line, which is right for a
//! script that prints nothing and for the model, which cannot watch anything.
//! A person running one from the launcher wants three things an action cannot
//! give them: to be asked for the arguments the script declares, to see the
//! output while it is still arriving, and to stop it.
//!
//! So this returns a job as soon as the script starts rather than when it
//! finishes, and the result arrives as an event. An action that blocked until
//! a script ended could not be cancelled by definition: the thing that would
//! cancel it is waiting for it.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::shell::{Ended, Stop};

const DONE: &str = "sill://script-done";

/// Every script running right now, by job.
///
/// A handle rather than the child itself: stopping is all anybody outside
/// needs to do, and holding the process here would mean two owners for
/// something that already knows how to end itself.
#[derive(Default)]
pub struct Running {
    jobs: Mutex<HashMap<String, Stop>>,
    next: AtomicU64,
}

impl Running {
    pub fn new() -> Self {
        Self::default()
    }

    fn start(&self, stop: Stop) -> String {
        let job = format!("job{}", self.next.fetch_add(1, Ordering::Relaxed));

        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job.clone(), stop);
        }

        job
    }

    fn finish(&self, job: &str) {
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.remove(job);
        }
    }

    fn stop(&self, job: &str) -> bool {
        let held = self
            .jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(job).cloned());

        match held {
            Some(stop) => {
                stop.stop();
                true
            }
            None => false,
        }
    }
}

/// What a finished script produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finished {
    pub job: String,
    pub title: String,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub ended: Ended,
    pub truncated: bool,
    pub took_ms: u64,
}

/// What a script asks to be told before it runs, in order.
///
/// The placeholders from its own header, so the launcher can ask using the
/// words the author chose. A script asking for "branch" should say "branch"
/// rather than "argument 1".
#[tauri::command]
pub(crate) fn script_arguments(path: String) -> Vec<String> {
    let Some(script) = crate::scripts::read(std::path::Path::new(&path)) else {
        return Vec::new();
    };

    crate::scripts::asks(&script)
}

/// Starts a script and answers with the job, not the result.
///
/// The result arrives on `sill://script-done`. Returning the job first is what
/// makes stopping possible at all: a command that answered when the script
/// ended could not be cancelled, because the thing that would cancel it would
/// be waiting on it.
#[tauri::command]
pub(crate) async fn run_script(
    app: AppHandle,
    path: String,
    args: Vec<String>,
) -> Result<String, String> {
    let script = crate::scripts::read(std::path::Path::new(&path))
        .ok_or_else(|| format!("{path} is not a script command"))?;

    let prefs = app
        .try_state::<crate::state::PrefsState>()
        .map(|prefs| prefs.inner.clone());

    let (timeout, allowed) = match &prefs {
        Some(prefs) => {
            let held = prefs.lock().await;
            (
                std::time::Duration::from_secs(held.scripts.timeout_seconds.max(1)),
                held.scripts.elevated.clone(),
            )
        }
        None => (crate::shell::DEFAULT_TIMEOUT, Vec::new()),
    };

    // Before the job is made, so a script that cannot run answers the caller
    // rather than starting a job that reports a failure by event a moment
    // later. The window has somewhere to show this; a job it has not been
    // told about yet does not.
    let plan = crate::scripts::plan(&script, &allowed)?;

    let stop = Stop::new();
    let job = app.state::<Running>().start(stop.clone());

    let running = job.clone();
    let title = script.title.clone();

    tauri::async_runtime::spawn(async move {
        let ran = crate::shell::run(
            &crate::shell::Setup::new(script.shell, &path)
                .with(&args)
                .in_folder(&plan.directory)
                .and_environment(&plan.environment)
                .within(timeout)
                .as_administrator(plan.elevated),
            &stop,
        )
        .await;

        // The job is forgotten before the result is announced. A window that
        // reacts by asking what is still running must not be told about one
        // that has just ended.
        app.state::<Running>().finish(&running);

        let finished = match ran {
            Ok(ran) => Finished {
                job: running,
                title,
                stdout: ran.stdout,
                stderr: ran.stderr,
                code: ran.code,
                ended: ran.ended,
                truncated: ran.truncated,
                took_ms: ran.took_ms,
            },
            // A script that could not be started still has to answer, or the
            // window waits for an event that is never coming and shows
            // "Running" for ever.
            Err(why) => Finished {
                job: running,
                title,
                stdout: String::new(),
                stderr: why,
                code: None,
                ended: Ended::Finished,
                truncated: false,
                took_ms: 0,
            },
        };

        let _ = app.emit(DONE, finished);
    });

    Ok(job)
}

/// Stops one, killing whatever it started along with it.
///
/// Answering `false` for a job that has already finished is not an error: the
/// race between somebody pressing stop and a script ending on its own is one
/// the script usually wins, and reporting that as a failure would put an error
/// on screen for the ordinary case.
#[tauri::command]
pub(crate) fn cancel_script(running: State<'_, Running>, job: String) -> bool {
    running.stop(&job)
}
