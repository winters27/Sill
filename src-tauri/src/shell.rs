//! Running a command, and keeping what it does inside known limits.
//!
//! ## Why this is its own capability
//!
//! [`Capability::ShellExecution`] is not a stronger `ProcessLaunch`. Opening a
//! program is asking Windows to do what a double click does; handing over a
//! shell is handing over every other capability at once, because a shell can
//! read files, write them, reach the network and start anything. Somebody
//! agreeing to "open a program" has not agreed to that, so it is a separate
//! thing to ask for and it is never granted alongside anything else.
//!
//! ## What is bounded, and why each one is
//!
//! A script is somebody else's code and all three of these have to be true
//! before it runs, or the launcher's own promise about what it costs stops
//! being true whenever a script misbehaves.
//!
//! - **Output.** Read up to [`MOST_OUTPUT`] and no further. A script printing
//!   in a loop would otherwise be a memory leak with a progress bar. The pipes
//!   keep being drained past the cap and thrown away, because a child whose
//!   output nobody reads blocks on its own write and never exits, which turns
//!   a runaway into a hang.
//! - **Time.** A deadline, after which it is killed. Scripts wait on things
//!   that never arrive.
//! - **Cancellation.** Somebody who started it can stop it, and stopping it
//!   kills the process rather than abandoning the wait.

use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::sync::Notify;

use crate::action::Capability;

/// Everything an action that runs a script has to declare.
pub const NEEDS: &[Capability] = &[Capability::ShellExecution];

/// As much of one stream as is kept.
///
/// Generous for anything anybody reads, and small next to the memory budget.
/// A script that produces more than this is producing more than a person is
/// going to look at.
pub const MOST_OUTPUT: usize = 256 * 1024;

/// How long a script runs before it is stopped, unless told otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// How long the readers get after the process has ended.
const GRACE: Duration = Duration::from_millis(500);

/// Console programs must not flash a window. Same flag `whisper-server` uses.
#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;

/// What a script is run by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Shell {
    PowerShell,
    Cmd,
    Bash,
    Python,
    /// The file is the program. Run directly, with no interpreter in front.
    Program,
}

impl Shell {
    /// Worked out from the file's own extension.
    ///
    /// Returns `None` rather than guessing: a file Sill has no interpreter for
    /// should be left alone, not handed to whichever shell was first in a
    /// list. Running a `.txt` through PowerShell is how a note becomes a
    /// command.
    pub fn of(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();

        Some(match extension.as_str() {
            "ps1" => Self::PowerShell,
            "cmd" | "bat" => Self::Cmd,
            "sh" | "bash" => Self::Bash,
            "py" => Self::Python,
            "exe" | "com" => Self::Program,
            _ => return None,
        })
    }

    /// The program, and everything before the script's own arguments.
    fn invocation(self, target: &str) -> (String, Vec<String>) {
        match self {
            Self::PowerShell => (
                "powershell.exe".into(),
                vec![
                    "-NoProfile".into(),
                    "-NonInteractive".into(),
                    "-ExecutionPolicy".into(),
                    "Bypass".into(),
                    "-File".into(),
                    target.into(),
                ],
            ),
            Self::Cmd => ("cmd.exe".into(), vec!["/c".into(), target.into()]),
            Self::Bash => ("bash.exe".into(), vec![target.into()]),
            Self::Python => ("python.exe".into(), vec![target.into()]),
            Self::Program => (target.into(), Vec::new()),
        }
    }
}

/// What happened, once it is over.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ran {
    pub stdout: String,
    pub stderr: String,
    /// `None` when it was killed rather than exiting on its own.
    pub code: Option<i32>,
    /// Whether either stream had more than was kept.
    pub truncated: bool,
    /// Why it ended, in a word the window can show without interpreting.
    pub ended: Ended,
    pub took_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Ended {
    /// It finished by itself, whatever its exit code was.
    Finished,
    /// It ran past its deadline.
    TimedOut,
    /// Somebody stopped it.
    Cancelled,
}

/// A handle for stopping one run.
///
/// Its own type rather than a bare `Notify` so a caller cannot accidentally
/// pass the wrong channel, and so a run that nobody intends to cancel can be
/// given [`Stop::never`] and read as deliberate.
#[derive(Clone, Default)]
pub struct Stop(Arc<Notify>);

impl Stop {
    pub fn new() -> Self {
        Self::default()
    }

    /// For a run nobody is going to stop.
    pub fn never() -> Self {
        Self::default()
    }

    pub fn stop(&self) {
        self.0.notify_waiters();
    }
}

/// Reads one stream, keeping at most `MOST_OUTPUT` and draining the rest.
///
/// Draining matters more than it looks. A pipe nobody empties fills, and the
/// child blocks writing into it forever, so a script that produces too much
/// would hang rather than being truncated. Reading and discarding keeps the
/// child moving towards its own exit.
async fn drain(mut stream: impl AsyncReadExt + Unpin, into: &Held) {
    let mut buffer = [0u8; 8192];

    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let Ok(mut held) = into.lock() else { break };
                let room = MOST_OUTPUT.saturating_sub(held.0.len());

                if room == 0 {
                    held.1 = true;
                    continue;
                }

                let taking = room.min(read);
                held.0.extend_from_slice(&buffer[..taking]);

                if taking < read {
                    held.1 = true;
                }
            }
        }
    }
}

/// Bytes kept so far, and whether anything was thrown away.
///
/// Shared rather than returned, so that a reader which has not finished can
/// still be asked what it saw. Everything it read up to that point is real
/// output, and abandoning it because the stream never closed would throw away
/// the part somebody most wants when a script has misbehaved.
type Held = std::sync::Mutex<(Vec<u8>, bool)>;

fn taken(held: &Held) -> (String, bool) {
    held.lock()
        .map(|seen| (String::from_utf8_lossy(&seen.0).into_owned(), seen.1))
        .unwrap_or_default()
}

/// Runs one thing and waits for it, within the limits above.
///
/// `target` is a script path for every shell but [`Shell::Cmd`], where it is
/// the command line itself, and [`Shell::Program`], where it is the program.
pub async fn run(
    shell: Shell,
    target: &str,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
    stop: &Stop,
) -> Result<Ran, String> {
    let started = Instant::now();
    let (program, mut argv) = shell.invocation(target);
    argv.extend_from_slice(args);

    let mut command = tokio::process::Command::new(&program);
    command
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    // tokio's Command carries this itself on Windows, so no trait is needed.
    #[cfg(windows)]
    command.creation_flags(NO_WINDOW);

    let mut child = command.spawn().map_err(|err| {
        // Naming the program matters: "the system cannot find the file" about
        // an unnamed thing sends somebody looking at their own script when
        // what is missing is python.
        format!("{program} could not be started: {err}")
    })?;

    /*
     * Everything it starts dies with it.
     *
     * Killing `cmd.exe` does not kill what `cmd.exe` started, and the first
     * version of this proved it: a deadline fired on time and the call still
     * took thirty seconds, because the grandchild was alive and holding the
     * pipe. A job object is the kernel answering the question properly, and
     * `whisper-server` already needed the same guarantee.
     */
    let job = crate::job::Job::new();

    #[cfg(windows)]
    if let (Some(job), Some(handle)) = (job.as_ref(), child.raw_handle()) {
        job.adopt_raw(handle);
    }

    let out = child.stdout.take().ok_or("no stdout")?;
    let err = child.stderr.take().ok_or("no stderr")?;

    /*
     * The pipes are read alongside the wait, never before it.
     *
     * Draining first and waiting afterwards reads as the tidy order and is
     * wrong: both streams stay open until the child exits, so awaiting them is
     * awaiting the child, and the deadline and the stop below could never fire.
     * A script that loops without printing would have hung here for ever with
     * a timeout sitting unused three lines down.
     *
     * Concurrently, the child ends for one of the three reasons, its pipes
     * close, and the readers finish on their own.
     */
    let out_held: Arc<Held> = Arc::default();
    let err_held: Arc<Held> = Arc::default();

    let readers = {
        let (into_out, into_err) = (out_held.clone(), err_held.clone());
        tokio::spawn(async move { tokio::join!(drain(out, &into_out), drain(err, &into_err)) })
    };

    let (ended, code) = tokio::select! {
        status = child.wait() => match status {
            Ok(status) => (Ended::Finished, status.code()),
            Err(err) => return Err(format!("waiting for {program} failed: {err}")),
        },
        _ = tokio::time::sleep(timeout) => {
            drop(job);
            let _ = child.kill().await;
            (Ended::TimedOut, None)
        }
        _ = stop.0.notified() => {
            drop(job);
            let _ = child.kill().await;
            (Ended::Cancelled, None)
        }
    };

    /*
     * A moment for the readers to finish, and then whatever they have.
     *
     * Bounded rather than awaited, because a pipe can outlive the process that
     * was writing to it: anything the child handed its own handle to keeps it
     * open, and waiting for that is waiting for something Sill does not
     * control. The job above makes that rare; this makes it survivable.
     */
    if tokio::time::timeout(GRACE, readers).await.is_err() {
        crate::say!("{program}: its output stream stayed open after it ended");
    }

    let (stdout, out_cut) = taken(&out_held);
    let (stderr, err_cut) = taken(&err_held);

    Ok(Ran {
        stdout,
        stderr,
        code,
        truncated: out_cut || err_cut,
        ended,
        took_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick() -> Duration {
        Duration::from_secs(10)
    }

    mod picking_an_interpreter {
        use super::*;

        #[test]
        fn a_known_extension_names_its_shell() {
            for (name, expected) in [
                ("go.ps1", Shell::PowerShell),
                ("go.PS1", Shell::PowerShell),
                ("go.cmd", Shell::Cmd),
                ("go.bat", Shell::Cmd),
                ("go.sh", Shell::Bash),
                ("go.py", Shell::Python),
                ("go.exe", Shell::Program),
            ] {
                assert_eq!(Shell::of(Path::new(name)), Some(expected), "{name}");
            }
        }

        /// A file nobody has an interpreter for is left alone.
        ///
        /// Falling back to a shell would mean a note or a config file being
        /// handed to PowerShell because it happened to be in a folder that is
        /// scanned, and the first line of a text file is not a command
        /// somebody meant to run.
        #[test]
        fn anything_else_is_not_a_script() {
            for name in ["notes.txt", "data.json", "README.md", "noextension"] {
                assert_eq!(Shell::of(Path::new(name)), None, "{name}");
            }
        }
    }

    #[tokio::test]
    async fn it_reports_what_a_command_printed() {
        let ran = run(Shell::Cmd, "echo hello", &[], None, quick(), &Stop::never())
            .await
            .expect("ran");

        assert!(ran.stdout.contains("hello"), "stdout was {:?}", ran.stdout);
        assert_eq!(ran.code, Some(0));
        assert_eq!(ran.ended, Ended::Finished);
        assert!(!ran.truncated);
    }

    /// A non-zero exit is a result, not an error.
    ///
    /// A script that fails has still run, and the caller needs its output and
    /// its code. Turning it into `Err` would throw both away and leave the
    /// window with nothing to show.
    #[tokio::test]
    async fn a_failing_command_still_reports_its_code() {
        let ran = run(Shell::Cmd, "exit 3", &[], None, quick(), &Stop::never())
            .await
            .expect("ran");

        assert_eq!(ran.code, Some(3));
        assert_eq!(ran.ended, Ended::Finished);
    }

    #[tokio::test]
    async fn a_program_that_is_not_there_says_which_one() {
        let failed = run(
            Shell::Program,
            "sill-nothing-is-called-this.exe",
            &[],
            None,
            quick(),
            &Stop::never(),
        )
        .await
        .expect_err("should fail");

        assert!(
            failed.contains("sill-nothing-is-called-this.exe"),
            "the error does not name the program: {failed}",
        );
    }

    /// The one that catches the bug this module was written with.
    ///
    /// Draining both pipes and *then* waiting reads as the tidy order and
    /// cannot work: the streams stay open until the child exits, so awaiting
    /// them is awaiting the child, and the deadline never fires. It only shows
    /// up with a command that produces no output, because anything that prints
    /// keeps the reader busy and hides it.
    #[tokio::test]
    async fn a_command_that_never_prints_still_hits_its_deadline() {
        let started = Instant::now();

        let ran = run(
            Shell::Cmd,
            "ping -n 30 127.0.0.1 >nul",
            &[],
            None,
            Duration::from_millis(400),
            &Stop::never(),
        )
        .await
        .expect("ran");

        assert_eq!(ran.ended, Ended::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "it waited for the command rather than for the deadline",
        );
    }

    #[tokio::test]
    async fn stopping_one_kills_it_rather_than_leaving_it() {
        let stop = Stop::new();
        let stopping = stop.clone();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            stopping.stop();
        });

        let started = Instant::now();

        let ran = run(
            Shell::Cmd,
            "ping -n 30 127.0.0.1 >nul",
            &[],
            None,
            Duration::from_secs(30),
            &stop,
        )
        .await
        .expect("ran");

        assert_eq!(ran.ended, Ended::Cancelled);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "cancelling did not stop the wait",
        );
    }

    /// More than is kept is dropped, and the fact is reported.
    ///
    /// Tested against the reader rather than a script, because producing a
    /// quarter of a megabyte from `cmd` takes seconds and proves the same
    /// thing this does in microseconds.
    #[tokio::test]
    async fn output_past_the_cap_is_dropped_and_admitted() {
        let much = vec![b'x'; MOST_OUTPUT * 2];
        let held = Held::default();
        drain(&much[..], &held).await;

        let (kept, truncated) = taken(&held);
        assert_eq!(kept.len(), MOST_OUTPUT);
        assert!(truncated, "it kept the cap and did not say it had cut anything");
    }

    #[tokio::test]
    async fn output_under_the_cap_is_whole_and_says_so() {
        let little = b"small".to_vec();
        let held = Held::default();
        drain(&little[..], &held).await;

        let (kept, truncated) = taken(&held);
        assert_eq!(kept, "small");
        assert!(!truncated);
    }
}
