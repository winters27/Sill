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
//!
//! ## What running elevated costs, which is all three of them
//!
//! Windows raises a UAC prompt from exactly one call, `ShellExecuteEx` with
//! the `runas` verb, and that call takes no pipes. A medium-integrity process
//! cannot read a high-integrity one's output, cannot put it in its job object,
//! and cannot terminate it. So an elevated run has no output cap, no deadline
//! and no stop: it is handed to Windows and that is the end of Sill's
//! involvement. [`Ended::Started`] exists to say exactly that rather than
//! letting an elevated run wear the same word as one that finished.

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
            // `/c` and the script are all that is fixed here; the script and
            // its arguments are appended by `run` as one raw argument, because
            // cmd does not parse a command line the way everything else does.
            // See `cmd_line`.
            Self::Cmd => (
                "cmd.exe".into(),
                vec!["/d".into(), "/s".into(), "/c".into()],
            ),
            Self::Bash => ("bash.exe".into(), vec![target.into()]),
            Self::Python => ("python.exe".into(), vec![target.into()]),
            Self::Program => (target.into(), Vec::new()),
        }
    }
}

/**
One token, quoted so `cmd.exe` reads it as one token.

Doubling an embedded quote is cmd's own escape, the same rule the shell uses
for `for /f` and `set`. Nothing else needs escaping once the whole line is
wrapped, which is what `/s` is for.
*/
#[cfg(windows)]
fn quoted(one: &str) -> String {
    format!("\"{}\"", one.replace('"', "\"\""))
}

/**
The script and its arguments, as one string `cmd.exe` will not take apart.

**`cmd.exe` does not parse its command line the way every other program does.**
Rust quotes each argument by the C runtime rules, which cmd ignores: it splits
on its own metacharacters first, so an argument of `x&calc` passed to
`cmd /c script.bat` ran `script.bat x`, then ran `calc`. A launcher that runs
somebody's scripts must not turn an argument into a second command.

`/s` is the documented way out. With it, cmd strips exactly the first and last
quote of the string after `/c` and treats everything between them as the
command, without the usual quote processing. So the whole line is wrapped once,
and each token inside is quoted on its own. That also fixes the older,
quieter half of the same bug: a script under a path with a space, invoked with
any quoted argument, tripped cmd's rule about which quotes to strip and failed
with a message about a path nobody had typed.

`/d` is unrelated and worth having anyway: it skips `AutoRun` from the
registry, so a command somebody once put there does not run before every
script Sill starts.
*/
#[cfg(windows)]
fn cmd_line(target: &str, args: &[String]) -> String {
    let mut line = String::from("\"");
    line.push_str(&quoted(target));

    for one in args {
        line.push(' ');
        line.push_str(&quoted(one));
    }

    line.push('"');
    line
}

/**
One token, quoted by the rules the C runtime splits a command line with.

The rules that `CommandLineToArgvW` and every `main` reverse: a backslash is
ordinary unless a quote follows it, in which case the run of backslashes in
front of that quote is doubled and the quote is escaped, and a run at the very
end is doubled so it cannot escape the closing quote. Rust's own `Command`
does this for us on the ordinary path; this exists because
[`ShellExecuteEx`](elevated) takes its arguments as one already-built string
and there is no `Command` between us and it.

A token with nothing awkward in it is left bare, so a command line stays
readable in Task Manager and in a log.
*/
#[cfg(windows)]
fn one_argument(one: &str) -> String {
    let awkward = |c: char| c == ' ' || c == '\t' || c == '"' || c == '\n' || c == '\u{b}';

    if !one.is_empty() && !one.contains(awkward) {
        return one.to_string();
    }

    let mut out = String::from("\"");
    let mut slashes = 0usize;

    for c in one.chars() {
        match c {
            '\\' => {
                slashes += 1;
                out.push('\\');
            }
            '"' => {
                // The run in front of this quote, doubled, then one more for
                // the quote itself.
                for _ in 0..=slashes {
                    out.push('\\');
                }
                slashes = 0;
                out.push('"');
            }
            _ => {
                slashes = 0;
                out.push(c);
            }
        }
    }

    // A run at the end would otherwise escape the quote that closes the token.
    for _ in 0..slashes {
        out.push('\\');
    }

    out.push('"');
    out
}

/// The interpreter's own switches and the script's arguments, as one string.
///
/// For every shell but [`Shell::Cmd`], which has [`cmd_line`] because it does
/// not parse a command line the way anything else does.
#[cfg(windows)]
fn argv_line(argv: &[String], args: &[String]) -> String {
    argv.iter()
        .chain(args.iter())
        .map(|one| one_argument(one))
        .collect::<Vec<_>>()
        .join(" ")
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
    /// It was handed to Windows to run as administrator, and that is all.
    ///
    /// Not a kind of `Finished`. Sill has no exit code, no output and no way
    /// to stop it, and calling that "finished" would put "It printed nothing"
    /// on screen under a script that may still be running. Everything that
    /// reads an `Ended` has to decide what to say about this one.
    Started,
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

/**
Everything one run is given, apart from the handle that stops it.

A struct rather than more parameters. `run` took six and the four this adds
would have made ten, at which point every call site is a row of values whose
meaning is their position: three `None`s in a line, and the one that is the
working directory is whichever one it is. The builder means a call says what
it is setting.
*/
pub struct Setup<'a> {
    pub shell: Shell,
    /// A script path for every shell but [`Shell::Program`], where it is the
    /// program itself.
    pub target: &'a str,
    pub args: &'a [String],
    /// Where it runs. Checked before anything is spawned; see [`run`].
    pub cwd: Option<&'a Path>,
    /**
    Variables set for this one child, and for nothing else.

    **Never any part of a command line.** They go into the process's
    environment block, which no shell parses, so a value holding `&`, a quote,
    `%PATH%` or a newline is that value and not the start of a second command.
    That is the whole reason a script may declare one: the value is data all
    the way down, and there is no quoting to get wrong.
    */
    pub env: &'a [(String, String)],
    pub timeout: Duration,
    /// Ask Windows for administrator rights. See the module header for what
    /// that costs, which is every limit above.
    pub elevated: bool,
}

impl<'a> Setup<'a> {
    /// A plain run: no arguments, no folder, no variables, the usual deadline.
    pub fn new(shell: Shell, target: &'a str) -> Self {
        Self {
            shell,
            target,
            args: &[],
            cwd: None,
            env: &[],
            timeout: DEFAULT_TIMEOUT,
            elevated: false,
        }
    }

    pub fn with(mut self, args: &'a [String]) -> Self {
        self.args = args;
        self
    }

    pub fn in_folder(mut self, cwd: &'a Path) -> Self {
        self.cwd = Some(cwd);
        self
    }

    pub fn and_environment(mut self, env: &'a [(String, String)]) -> Self {
        self.env = env;
        self
    }

    pub fn within(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn as_administrator(mut self, elevated: bool) -> Self {
        self.elevated = elevated;
        self
    }
}

/// Whether this is somewhere a process can be started, in words.
///
/// Checked here rather than left to the spawn because Windows answers a bad
/// working directory with "The directory name is invalid. (os error 267)",
/// which names nothing and reads as a fault in the script. Callers that know
/// more say more; this is the last gate, and it holds for every caller
/// including ones written later.
fn somewhere_to_run(cwd: Option<&Path>) -> Result<(), String> {
    let Some(dir) = cwd else {
        return Ok(());
    };

    if dir.is_dir() {
        return Ok(());
    }

    let shown = dir.display();

    Err(if dir.exists() {
        format!("{shown} is a file rather than a folder, so nothing can be run in it")
    } else {
        format!("there is no folder called {shown} to run this in")
    })
}

/// What `ShellExecuteEx` is handed as its one argument string.
///
/// Pure and tested on its own, because this is the **second** place in this
/// module where somebody's script arguments are written into a line another
/// program takes apart, and the first one was the injection `P0-11` closed.
/// The two must not be allowed to disagree, so cmd's half is the same
/// `/d /s /c` and the same [`cmd_line`] the ordinary path uses.
#[cfg(windows)]
fn elevated_line(shell: Shell, target: &str, args: &[String]) -> String {
    let (_, argv) = shell.invocation(target);

    match shell {
        // cmd parses this itself, by its own rules, exactly as it does behind
        // `raw_arg`. The switches come from `invocation` so the two cannot
        // drift apart.
        Shell::Cmd => format!("{} {}", argv.join(" "), cmd_line(target, args)),
        _ => argv_line(&argv, args),
    }
}

/// Hands it to Windows with the `runas` verb, and stops being involved.
///
/// No pipes, no job object, no deadline and no stop; see the module header.
/// The window is shown rather than hidden, and deliberately: it is the only
/// thing the person will see of a run Sill cannot report on.
#[cfg(windows)]
async fn elevated(setup: &Setup<'_>, started: Instant) -> Result<Ran, String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    /// `ERROR_CANCELLED` as an `HRESULT`: somebody said no to the prompt.
    const REFUSED: u32 = 0x8007_04C7;

    let (program, _) = setup.shell.invocation(setup.target);
    let parameters = elevated_line(setup.shell, setup.target, setup.args);
    let directory = setup
        .cwd
        .map(|dir| dir.to_string_lossy().to_string())
        .unwrap_or_default();

    let file = program.clone();

    // Blocking, because the call does not return until the prompt has been
    // answered, and a person taking ten seconds to read it must not be ten
    // seconds of a runtime worker.
    let done = tokio::task::spawn_blocking(move || {
        let verb = HSTRING::from("runas");
        let file = HSTRING::from(file);
        let parameters = HSTRING::from(parameters);
        let directory = HSTRING::from(directory);

        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            // Without this the call can return before the elevated process
            // exists, and a caller that then went away would take the request
            // with it.
            fMask: SEE_MASK_NOASYNC,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(parameters.as_ptr()),
            lpDirectory: if directory.is_empty() {
                PCWSTR::null()
            } else {
                PCWSTR(directory.as_ptr())
            },
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };

        unsafe { ShellExecuteExW(&mut info) }
    })
    .await
    .map_err(|err| format!("asking for administrator rights did not finish: {err}"))?;

    match done {
        Ok(()) => Ok(Ran {
            stdout: String::new(),
            stderr: String::new(),
            code: None,
            truncated: false,
            ended: Ended::Started,
            took_ms: started.elapsed().as_millis() as u64,
        }),
        // Saying no is an answer, not a fault, and it reads as one.
        Err(err) if err.code().0 as u32 == REFUSED => {
            Err("administrator rights were refused, so nothing was run".to_string())
        }
        Err(err) => Err(format!(
            "{program} could not be started as administrator: {err}"
        )),
    }
}

/// There is no UAC here, so this only has to compile and stay coherent.
#[cfg(not(windows))]
async fn elevated(_setup: &Setup<'_>, _started: Instant) -> Result<Ran, String> {
    Err("running as administrator is a Windows thing".to_string())
}

/// The command, exactly as it will be spawned.
///
/// Its own function so a test can read what was built rather than infer it
/// from what a process did. An exit code can be right for the wrong reason:
/// the `P0-11` injection ran a second command and the first one still exited
/// zero. What a test has to be able to assert is the line itself, and the
/// other half of that line's safety is what is **not** on it, which is every
/// environment value.
fn built(setup: &Setup<'_>) -> tokio::process::Command {
    let (shell, target, args) = (setup.shell, setup.target, setup.args);
    let (program, mut argv) = shell.invocation(target);

    // Everything but cmd takes its arguments the ordinary way, where Rust's
    // quoting and the program's parsing agree.
    if shell != Shell::Cmd {
        argv.extend_from_slice(args);
    }

    let mut command = tokio::process::Command::new(&program);
    command
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    if let Some(dir) = setup.cwd {
        command.current_dir(dir);
    }

    // Into the environment block, one at a time, and never near the command
    // line. See `Setup::env`.
    for (name, value) in setup.env {
        command.env(name, value);
    }

    /*
     * The one argument Rust must not quote.
     *
     * `raw_arg` appends to the command line verbatim. That is exactly wrong
     * for every other program and exactly right for cmd under `/s`, which
     * wants one already-quoted string and strips the outer pair itself.
     */
    #[cfg(windows)]
    if shell == Shell::Cmd {
        use std::os::windows::process::CommandExt as _;
        command.as_std_mut().raw_arg(cmd_line(target, args));
    }

    // There is no cmd here, so this only has to compile and stay coherent.
    #[cfg(not(windows))]
    if shell == Shell::Cmd {
        command.arg(target).args(args);
    }

    // tokio's Command carries this itself on Windows, so no trait is needed.
    #[cfg(windows)]
    command.creation_flags(NO_WINDOW);

    command
}

/// Runs one thing and waits for it, within the limits above.
pub async fn run(setup: &Setup<'_>, stop: &Stop) -> Result<Ran, String> {
    let started = Instant::now();
    somewhere_to_run(setup.cwd)?;

    // Its own path from here. None of the limits below exist for an elevated
    // run, and pretending otherwise would mean a deadline that never fires
    // and a stop button that does nothing.
    if setup.elevated {
        return elevated(setup, started).await;
    }

    let program = setup.shell.invocation(setup.target).0;
    let mut command = built(setup);

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
        _ = tokio::time::sleep(setup.timeout) => {
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

    /**
    A real script on disk, because that is what `run` is given.

    These used to pass a command line as the target, which worked only because
    `cmd /c` re-parsed the string. Closing that hole closed this shortcut with
    it, and rightly: `Shell::of` reads an extension off a path, and both callers
    in the app pass a path. A test that exercises a path nothing in the product
    takes is testing something nobody runs.

    The directory is returned alongside the path so the caller keeps it alive;
    dropping it takes the file with it.
    */
    fn script(body: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("go.bat");
        std::fs::write(
            &path,
            format!(
                "@echo off
{body}
"
            ),
        )
        .expect("wrote the script");
        let name = path.to_string_lossy().to_string();
        (dir, name)
    }

    /// What cmd is handed, which is the only thing standing between a script
    /// argument and a second command.
    #[cfg(windows)]
    mod the_cmd_line {
        use super::*;

        fn line(target: &str, args: &[&str]) -> String {
            cmd_line(
                target,
                &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            )
        }

        #[test]
        fn the_whole_thing_is_wrapped_once_and_every_token_is_quoted() {
            assert_eq!(
                line(r"C:\a b\go.bat", &["one", "two"]),
                r##"""C:\a b\go.bat" "one" "two"""##,
                "one outer pair for /s to strip, and a pair around each token"
            );
        }

        /// The bug this exists for.
        #[test]
        fn a_metacharacter_in_an_argument_cannot_start_a_second_command() {
            let out = line("go.bat", &["x&calc", "a|b", "c>d", "e^f"]);

            for one in ["\"x&calc\"", "\"a|b\"", "\"c>d\"", "\"e^f\""] {
                assert!(out.contains(one), "{one} is not quoted in {out}");
            }

            // Nothing outside a quoted token, which is what cmd would act on.
            assert!(
                !out.contains("&calc\" \"") || out.contains("\"x&calc\""),
                "the ampersand escaped its token: {out}"
            );
        }

        #[test]
        fn a_quote_inside_an_argument_is_doubled_rather_than_ending_it() {
            assert_eq!(
                line("go.bat", &[r#"say "hi""#]),
                r##"""go.bat" "say ""hi"""""##,
            );
        }

        #[test]
        fn a_script_with_no_arguments_is_still_wrapped() {
            assert_eq!(line(r"C:\a b\go.bat", &[]), r##"""C:\a b\go.bat"""##);
        }
    }

    /**
    The whole command, as it will be spawned, for the inputs a script declares.

    Asserted on the line rather than on what a process did, because an exit
    code can be right for the wrong reason: the injection `P0-11` closed ran a
    second command and the first one still exited zero. Every declared value
    is either exactly where it was meant to go, or it is on this line, and
    these say which.
    */
    #[cfg(windows)]
    mod what_the_command_is_built_as {
        use super::*;

        fn parts(setup: &Setup<'_>) -> (String, Vec<String>) {
            let command = built(setup);
            let inner = command.as_std();

            (
                inner.get_program().to_string_lossy().to_string(),
                inner
                    .get_args()
                    .map(|one| one.to_string_lossy().to_string())
                    .collect(),
            )
        }

        fn variables(setup: &Setup<'_>) -> Vec<(String, String)> {
            built(setup)
                .as_std()
                .get_envs()
                .map(|(name, value)| {
                    (
                        name.to_string_lossy().to_string(),
                        value.unwrap_or_default().to_string_lossy().to_string(),
                    )
                })
                .collect()
        }

        /// The value cmd would take apart if it ever saw it.
        ///
        /// `&` starts a second command, a quote ends a token, `%PATH%` is
        /// expanded before anything runs, and a newline ends the line. All
        /// four are ordinary characters in an environment block and none of
        /// them is a character anywhere near this command line.
        fn nasty() -> Vec<(String, String)> {
            vec![(
                "SILL_TEST".to_string(),
                "a&calc \"quoted\" %PATH%\nsecond line".to_string(),
            )]
        }

        #[test]
        fn a_declared_variable_goes_into_the_environment_and_nowhere_else() {
            let env = nasty();
            let setup = Setup::new(Shell::Cmd, r"C:\a b\go.bat").and_environment(&env);

            let (program, args) = parts(&setup);

            assert_eq!(program, "cmd.exe");
            assert_eq!(
                args,
                vec!["/d", "/s", "/c", r##"""C:\a b\go.bat"""##],
                "an environment value reached the command line",
            );
            assert_eq!(variables(&setup), env, "the value did not arrive whole");
        }

        /// The same, for the shells that take their arguments the ordinary way.
        #[test]
        fn the_same_holds_for_an_interpreter_with_its_own_argv() {
            let env = nasty();
            let args = vec!["one two".to_string()];
            let setup = Setup::new(Shell::PowerShell, r"C:\a b\go.ps1")
                .with(&args)
                .and_environment(&env);

            let (program, argv) = parts(&setup);

            assert_eq!(program, "powershell.exe");
            assert_eq!(
                argv,
                vec![
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    r"C:\a b\go.ps1",
                    "one two",
                ],
            );
            assert_eq!(variables(&setup), env);
        }

        /// A folder is handed over as a folder, never spliced into the line.
        ///
        /// Both awkward spellings at once: a space, which would end a token,
        /// and a trailing backslash, which would escape a closing quote.
        #[test]
        fn a_working_directory_is_given_whole_and_stays_off_the_command_line() {
            let folder = Path::new(r"C:\a folder\");
            let setup = Setup::new(Shell::Cmd, "go.bat").in_folder(folder);

            assert_eq!(
                built(&setup).as_std().get_current_dir(),
                Some(Path::new(r"C:\a folder\")),
            );

            let (_, args) = parts(&setup);
            assert_eq!(args, vec!["/d", "/s", "/c", r##"""go.bat"""##]);
        }

        /// Nothing is set that was not declared.
        ///
        /// The child still inherits Sill's own environment, which is what
        /// `get_envs` does not list: this is the additions, and there should
        /// be none.
        #[test]
        fn a_run_that_declares_nothing_adds_nothing() {
            assert!(variables(&Setup::new(Shell::Cmd, "go.bat")).is_empty());
        }
    }

    /// The line `ShellExecuteEx` is handed, which is the second place a
    /// script's arguments are written into something another program parses.
    #[cfg(windows)]
    mod the_elevated_line {
        use super::*;

        #[test]
        fn cmd_gets_the_same_switches_and_the_same_wrapped_line() {
            assert_eq!(
                elevated_line(Shell::Cmd, r"C:\a b\go.bat", &["x&calc".to_string()]),
                r##"/d /s /c ""C:\a b\go.bat" "x&calc"""##,
                "the elevated path and the ordinary one must quote alike",
            );
        }

        #[test]
        fn an_interpreter_gets_its_switches_and_one_token_each() {
            assert_eq!(
                elevated_line(
                    Shell::PowerShell,
                    r"C:\a b\go.ps1",
                    &["one two".to_string()]
                ),
                r#"-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "C:\a b\go.ps1" "one two""#,
            );
        }

        /// The C runtime's rules, which are not cmd's.
        #[test]
        fn a_token_is_quoted_by_the_rules_the_runtime_reads_back() {
            assert_eq!(one_argument("plain"), "plain");
            assert_eq!(one_argument("one two"), r#""one two""#);
            assert_eq!(one_argument(r#"say "hi""#), r#""say \"hi\"""#);
            // A run of backslashes at the end is doubled, or it escapes the
            // quote that closes the token and swallows the next one.
            assert_eq!(one_argument(r"C:\a b\"), r#""C:\a b\\""#);
            // An empty argument still has to be an argument.
            assert_eq!(one_argument(""), r#""""#);
        }
    }

    /// Where it runs, before anything is started.
    mod somewhere_to_run_it {
        use super::*;

        #[test]
        fn no_folder_named_is_not_a_problem() {
            assert!(somewhere_to_run(None).is_ok());
        }

        #[test]
        fn a_real_folder_is_fine() {
            let dir = tempfile::tempdir().expect("a temp dir");
            assert!(somewhere_to_run(Some(dir.path())).is_ok());
        }

        /// The point of checking at all.
        ///
        /// Windows answers a bad working directory with "The directory name is
        /// invalid. (os error 267)", which names neither the directory nor
        /// anything a person can do about it.
        #[test]
        fn a_folder_that_is_not_there_is_named_rather_than_numbered() {
            let dir = tempfile::tempdir().expect("a temp dir");
            let missing = dir.path().join("no such folder");

            let why = somewhere_to_run(Some(&missing)).expect_err("refused");

            assert!(
                why.contains(&missing.display().to_string()),
                "it did not say which folder: {why}",
            );
            assert!(!why.contains("os error"), "it handed on the number: {why}");
        }

        #[test]
        fn a_file_where_a_folder_should_be_says_which_it_is() {
            let dir = tempfile::tempdir().expect("a temp dir");
            let file = dir.path().join("notes.txt");
            std::fs::write(&file, b"x").expect("wrote");

            let why = somewhere_to_run(Some(&file)).expect_err("refused");

            assert!(
                why.contains("file rather than a folder"),
                "it did not say what it found: {why}",
            );
        }
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
        let (_dir, path) = script("echo hello");
        let ran = run(
            &Setup::new(Shell::Cmd, &path).within(quick()),
            &Stop::never(),
        )
        .await
        .expect("ran");

        assert!(ran.stdout.contains("hello"), "stdout was {:?}", ran.stdout);
        assert_eq!(ran.code, Some(0));
        assert_eq!(ran.ended, Ended::Finished);
        assert!(!ran.truncated);
    }

    /// The other half of the structural tests: it also has to arrive.
    ///
    /// `set NAME` rather than `echo %NAME%`, deliberately. Batch expands
    /// `%NAME%` into the line before running it, so echoing a value holding
    /// `&` is the script splitting its own command, which is the script's
    /// doing and not Sill's. `set` prints the variable without re-reading it.
    #[tokio::test]
    async fn a_declared_variable_reaches_the_script_whole() {
        let said = "a&calc \"quoted\" %PATH%";
        let env = vec![("SILL_TEST".to_string(), said.to_string())];

        let (_dir, path) = script("set SILL_TEST");
        let ran = run(
            &Setup::new(Shell::Cmd, &path)
                .and_environment(&env)
                .within(quick()),
            &Stop::never(),
        )
        .await
        .expect("ran");

        assert!(
            ran.stdout.contains(&format!("SILL_TEST={said}")),
            "the value did not arrive whole: {:?}",
            ran.stdout,
        );
    }

    /// A folder with a space in it, which is the one that used to break.
    #[tokio::test]
    async fn it_runs_in_the_folder_it_was_given() {
        let (dir, path) = script("cd");
        let folder = dir.path().join("a folder");
        std::fs::create_dir(&folder).expect("made the folder");

        let ran = run(
            &Setup::new(Shell::Cmd, &path)
                .in_folder(&folder)
                .within(quick()),
            &Stop::never(),
        )
        .await
        .expect("ran");

        assert!(
            ran.stdout.trim().ends_with("a folder"),
            "it ran somewhere else: {:?}",
            ran.stdout,
        );
    }

    /// A folder that is not there is refused before anything is spawned.
    #[tokio::test]
    async fn a_missing_folder_stops_it_before_it_starts() {
        let (dir, path) = script("echo hello");
        let missing = dir.path().join("no such folder");

        let why = run(
            &Setup::new(Shell::Cmd, &path)
                .in_folder(&missing)
                .within(quick()),
            &Stop::never(),
        )
        .await
        .expect_err("refused");

        assert!(why.contains("no such folder"), "it said {why}");
    }

    /// A non-zero exit is a result, not an error.
    ///
    /// A script that fails has still run, and the caller needs its output and
    /// its code. Turning it into `Err` would throw both away and leave the
    /// window with nothing to show.
    #[tokio::test]
    async fn a_failing_command_still_reports_its_code() {
        let (_dir, path) = script("exit /b 3");
        let ran = run(
            &Setup::new(Shell::Cmd, &path).within(quick()),
            &Stop::never(),
        )
        .await
        .expect("ran");

        assert_eq!(ran.code, Some(3));
        assert_eq!(ran.ended, Ended::Finished);
    }

    #[tokio::test]
    async fn a_program_that_is_not_there_says_which_one() {
        let failed = run(
            &Setup::new(Shell::Program, "sill-nothing-is-called-this.exe").within(quick()),
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

        let (_dir, path) = script("ping -n 30 127.0.0.1 >nul");
        let ran = run(
            &Setup::new(Shell::Cmd, &path).within(Duration::from_millis(400)),
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

        let (_dir, path) = script("ping -n 30 127.0.0.1 >nul");
        let ran = run(
            &Setup::new(Shell::Cmd, &path).within(Duration::from_secs(30)),
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
        assert!(
            truncated,
            "it kept the cap and did not say it had cut anything"
        );
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
