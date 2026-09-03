//! Running a subprocess with a deadline, and reading what it says while it runs.
//!
//! Installing an extension spawns two programs Sill did not write. npm talks
//! to a registry over the network and esbuild reads whatever source arrived,
//! and both were run with [`std::process::Command::output`], which waits for
//! the child to close its pipes and has no way to stop waiting. A registry
//! that accepts the connection and then says nothing leaves an install with no
//! end: the window says "Installing" until somebody quits Sill.
//!
//! ## Why the output is streamed rather than collected
//!
//! `output()` reads both pipes to EOF and hands back the whole thing at once,
//! which is two problems in one call. Nothing can be shown while it runs, so a
//! ninety-second npm install is a spinner with no content; and the buffer is
//! unbounded, so a program that decides to print a megabyte of warnings is a
//! megabyte held for the length of the install. This reads both pipes on
//! threads, hands each line to the caller as it arrives, and keeps only the
//! last [`KEPT`] bytes for the error message.
//!
//! That is the same bargain the extension host already makes with a worker's
//! stdout, and for the same reason: **a pipe nobody drains is both invisible
//! and an unbounded buffer**, and a child whose pipe buffer fills stops.
//!
//! ## Why the deadline kills a tree rather than a process
//!
//! npm is a Node program that spawns more of them. Killing the one Sill
//! started leaves its children resolving a dependency tree for a build that
//! has already been abandoned, so the child is put in a [`crate::job::Job`]
//! first and letting go of the job is what actually ends it.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How much of what a program said is kept for the error message.
///
/// The tail rather than the head: a build that failed says why at the end, and
/// npm's first sixty-four kilobytes are progress bars.
pub const KEPT: usize = 64 * 1024;

/// How often the wait wakes up to look at the clock.
///
/// Only reached when the child is saying nothing, because a line arriving is
/// what normally ends the wait. Two hundred milliseconds is imperceptible
/// against a deadline measured in minutes and is eight wakeups for a build
/// that takes a second and a half.
const POLL: Duration = Duration::from_millis(200);

/// The longest a line may be before it is cut.
///
/// A bundler that fails on a minified file will happily print the file. The
/// same cut the extension host makes on a worker's console output.
const LINE: usize = 2_000;

/// What a bounded run produced.
#[derive(Debug, Clone)]
pub struct Ran {
    /// Whether the program exited successfully.
    pub ok: bool,
    /// The tail of what it wrote, on either stream, in the order it arrived.
    pub said: String,
}

/// Runs `command` and gives up after `limit`.
///
/// `on_line` is called with each line as it arrives, on this thread, so a
/// caller can report progress without owning the plumbing. It is called for
/// stdout and stderr alike: which stream a program complains on is not
/// something the caller should have to know.
///
/// The error is the deadline having fired. A program that runs and fails is
/// `Ok` with `ok: false`, because "it refused" and "it never answered" are
/// different things and the caller says different words about them.
pub fn run(
    command: &mut Command,
    limit: Duration,
    on_line: &mut dyn FnMut(&str),
) -> Result<Ran, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("could not start {}: {err}", named(command)))?;

    // Before anything is read, so the window in which a child could spawn a
    // process outside the job is as small as it can be.
    let job = crate::job::Job::new();
    if let Some(job) = &job {
        job.adopt(&child);
    }

    let (tx, rx) = mpsc::channel::<String>();

    if let Some(stream) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || pump(BufReader::new(stream), tx));
    }
    if let Some(stream) = child.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || pump(BufReader::new(stream), tx));
    }

    // The loop below ends when every sender is gone, so this one must not
    // outlive the spawn above.
    drop(tx);

    let started = Instant::now();
    let mut said = String::new();

    loop {
        let left = limit.checked_sub(started.elapsed());

        let Some(left) = left else {
            end(&mut child, job);
            return Err(gave_up(command, limit));
        };

        match rx.recv_timeout(POLL.min(left)) {
            Ok(line) => {
                on_line(&line);
                keep(&mut said, &line);
            }
            // Both pipes are at EOF, which is the program having finished.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
        }
    }

    // The pipes closing is not quite the process exiting, so this is still a
    // wait, and it is still bounded: a child holding its handle open after
    // closing stdio is exactly the hang this exists for.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Ran {
                    ok: status.success(),
                    said,
                })
            }
            Ok(None) if started.elapsed() >= limit => {
                end(&mut child, job);
                return Err(gave_up(command, limit));
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(err) => return Err(format!("could not wait for {}: {err}", named(command))),
        }
    }
}

/// Kills the child and everything it started.
///
/// Both halves matter. `kill` ends the one process Sill spawned, and dropping
/// the job closes the last handle to it, which is what the kernel takes as the
/// signal to terminate every other process inside.
fn end(child: &mut std::process::Child, job: Option<crate::job::Job>) {
    let _ = child.kill();
    drop(job);
    let _ = child.wait();
}

/// What is said when the deadline fires.
///
/// Names the program and the limit, because "the install failed" about a
/// subprocess nobody knew was involved is not something anybody can act on,
/// and a stalled network is the likeliest cause by a distance.
fn gave_up(command: &Command, limit: Duration) -> String {
    format!(
        "{} was still running after {} seconds, so Sill stopped waiting for it. \
         A slow or unreachable network is the usual reason.",
        named(command),
        limit.as_secs()
    )
}

/// The program's own name, without the path it was found at.
fn named(command: &Command) -> String {
    std::path::Path::new(command.get_program())
        .file_name()
        .unwrap_or(command.get_program())
        .to_string_lossy()
        .into_owned()
}

/// Adds a line to the tail that is kept, dropping the front when it is full.
fn keep(said: &mut String, line: &str) {
    said.push_str(line);
    said.push('\n');

    if said.len() <= KEPT {
        return;
    }

    // On a character boundary, because the excess is measured in bytes and
    // the string is UTF-8.
    let from = said
        .char_indices()
        .find(|(at, _)| *at >= said.len() - KEPT)
        .map(|(at, _)| at)
        .unwrap_or(0);
    said.drain(..from);
}

/// Reads one stream to EOF, a line at a time.
fn pump(reader: impl BufRead, tx: mpsc::Sender<String>) {
    for line in reader.lines() {
        let Ok(mut line) = line else { return };
        line.truncate(
            line.char_indices()
                .take_while(|(at, _)| *at < LINE)
                .last()
                .map(|(at, c)| at + c.len_utf8())
                .unwrap_or(0),
        );

        // The receiver has gone, which means the deadline fired.
        if tx.send(line).is_err() {
            return;
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// A child that will not finish, the way `job.rs` writes one.
    fn sleeps() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/c", "ping -n 60 127.0.0.1"]);
        command
    }

    /// The whole point. A deadline that does not fire is not a deadline.
    #[test]
    fn a_child_that_never_finishes_is_given_up_on() {
        let started = Instant::now();
        let answer = run(&mut sleeps(), Duration::from_millis(300), &mut |_| {});

        let said = answer.expect_err("a child sleeping for a minute must not be waited for");

        assert!(
            said.contains("stopped waiting"),
            "the message has to say what happened: {said}"
        );
        assert!(said.contains("cmd"), "and name the program: {said}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "it waited {:?}, which is not a deadline",
            started.elapsed()
        );
    }

    /// Giving up has to end the process, not stop looking at it.
    ///
    /// A run that was abandoned rather than killed leaves a program running
    /// for the rest of the session, which for npm is a dependency tree still
    /// being resolved for a build nobody wants any more.
    ///
    /// Measured by what the child is still doing rather than by counting
    /// processes: `ping` writes a line a second, so if it is alive its output
    /// grows and if it is not it does not. Counting `ping.exe` on the machine
    /// was the first version and it is a test that fails when another test
    /// happens to run one at the same moment.
    #[test]
    fn giving_up_kills_the_child_rather_than_leaving_it() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let writing = scratch.path().join("still-going.txt");

        // `raw_arg`, because `args` quotes what it is given and cmd then reads
        // the redirect as part of a quoted string. The same bargain
        // `shell::cmd_line` makes, and for the same reason.
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("cmd");
        command
            .raw_arg("/d /s /c ")
            .raw_arg(format!("ping -n 60 127.0.0.1 > \"{}\"", writing.display()));

        run(&mut command, Duration::from_millis(1_500), &mut |_| {})
            .expect_err("a child pinging for a minute must not be waited for");

        // The kill is asynchronous at the kernel level, so this is not the
        // measurement, it is waiting for the measurement to be fair.
        std::thread::sleep(Duration::from_secs(3));
        let settled = std::fs::metadata(&writing).map(|it| it.len()).unwrap_or(0);

        // Two more lines' worth. A ping that is still alive writes in here.
        std::thread::sleep(Duration::from_secs(3));
        let later = std::fs::metadata(&writing).map(|it| it.len()).unwrap_or(0);

        assert_eq!(
            later, settled,
            "the child is still writing, so it outlived the deadline that gave up on it"
        );
    }

    /// A program that finishes inside its deadline is not disturbed by it.
    #[test]
    fn a_program_that_answers_is_left_alone() {
        let mut command = Command::new("cmd");
        command.args(["/c", "echo hello"]);

        let ran = run(&mut command, Duration::from_secs(30), &mut |_| {}).expect("it finished");

        assert!(ran.ok);
        assert!(ran.said.contains("hello"), "got {:?}", ran.said);
    }

    /// A refusal is an answer. Only a silence is a deadline.
    #[test]
    fn a_program_that_fails_is_a_result_rather_than_an_error() {
        let mut command = Command::new("cmd");
        command.args(["/c", "echo bad 1>&2 & exit /b 3"]);

        let ran = run(&mut command, Duration::from_secs(30), &mut |_| {}).expect("it finished");

        assert!(!ran.ok, "exit code 3 is a failure");
        assert!(
            ran.said.contains("bad"),
            "what it said on stderr is what names the problem: {:?}",
            ran.said
        );
    }

    /// Lines reach the caller while the program is still running.
    #[test]
    fn every_line_is_handed_over_as_it_arrives() {
        let mut command = Command::new("cmd");
        command.args(["/c", "echo one & echo two & echo three"]);

        let mut seen: Vec<String> = Vec::new();
        run(&mut command, Duration::from_secs(30), &mut |line| {
            seen.push(line.to_string())
        })
        .expect("it finished");

        assert_eq!(seen.len(), 3, "got {seen:?}");
        assert_eq!(seen[0].trim(), "one");
        assert_eq!(seen[2].trim(), "three");
    }

    /// A program that prints without stopping must not be held in full.
    #[test]
    fn what_is_kept_is_bounded() {
        let mut said = String::new();
        let line = "x".repeat(1_000);

        for _ in 0..200 {
            keep(&mut said, &line);
        }

        assert!(
            said.len() <= KEPT,
            "kept {} bytes of what a program said",
            said.len()
        );
        assert!(
            said.ends_with(&format!("{line}\n")),
            "the tail is what names the failure, so it is the end that is kept"
        );
    }
}
