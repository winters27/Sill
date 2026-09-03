//! Ending a program, against programs this test started itself.
//!
//! Ignored, and it has to be. It starts real programs and two of them open a
//! window that takes the focus for a moment, which is not a thing to do to
//! somebody in the middle of a `cargo test`. Run it deliberately:
//!
//! ```text
//! cargo test --test probe_processes -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Everything here is aimed at something started from this file and chosen
//! because it can be lost: Character Map holds no document, and `ping` counts
//! to sixty. Nothing here goes near a program somebody else is using, and no
//! text editor is opened, because an editor is the one program on the machine
//! that might be holding work. The decisions the rows depend on are covered by
//! ordinary unit tests in `processes.rs` that need no machine at all.
//!
//! ## Why nothing is spawned as a child
//!
//! `may_end` refuses anything descended from the process asking, which is what
//! stops the launcher quitting its own renderer, and a `Command::spawn` from a
//! test is exactly that: the first draft of this file spawned `cmd.exe` and
//! got "Sill will not end itself" back, which is the check working. So each of
//! these is handed to a `cmd` that then exits, leaving a program whose parent
//! is not here.

#![cfg(windows)]

use std::time::{Duration, Instant};

/// Starts a program that is not descended from this test, and finds its id.
///
/// `start` hands the program over and the `cmd` carrying it exits, so the
/// parent recorded against it is a process id that is already gone. Found
/// afterwards by name, comparing against what was running before, because the
/// handle belonged to the `cmd` rather than to what it started.
fn detached(program: &str, extra: &[&str]) -> u32 {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let name = program.to_ascii_lowercase();
    let before = ids_named(&name);

    let mut arguments = vec!["/c", "start", "", program];
    arguments.extend_from_slice(extra);

    let mut carrier = std::process::Command::new("cmd.exe")
        .args(&arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .unwrap_or_else(|err| panic!("cmd.exe starts: {err}"));

    let _ = carrier.wait();

    let waited = Instant::now();

    while waited.elapsed() < Duration::from_secs(10) {
        if let Some(pid) = ids_named(&name).into_iter().find(|id| !before.contains(id)) {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    panic!("{program} never appeared in the process list");
}

fn ids_named(lower: &str) -> Vec<u32> {
    sill_lib::processes::running()
        .into_iter()
        .filter(|process| process.name.eq_ignore_ascii_case(lower))
        .map(|process| process.pid)
        .collect()
}

/// Whether a process has gone, given a moment to go.
fn went(pid: u32, name: &str) -> bool {
    let waited = Instant::now();

    while waited.elapsed() < Duration::from_secs(5) {
        if !ids_named(name).contains(&pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    false
}

/// Left running whatever else happened, so a failure does not leak a window.
fn tidy(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

/// The identity check, against a real process id.
///
/// A reused id cannot be produced to order, so this asks the same question
/// from the other side: the id is live and the name is wrong. Nothing may
/// happen to it, and the message has to name what it really is.
#[test]
#[ignore]
fn an_id_acted_on_under_the_wrong_name_is_left_alone() {
    let pid = detached("charmap.exe", &[]);

    for wrong in ["chrome.exe", "explorer.exe"] {
        let refused = sill_lib::processes::force_quit(pid, wrong)
            .expect_err("the id does not name that program");

        println!("  force_quit as {wrong} said: {refused}");
        assert!(
            refused.to_ascii_lowercase().contains("charmap.exe"),
            "{refused}"
        );

        if !ids_named("charmap.exe").contains(&pid) {
            panic!("{wrong} ended a process that is not it");
        }
    }

    tidy(pid);
}

/// Quit closes a window rather than ending the process behind it.
///
/// Character Map, which is a plain Win32 dialog: it owns its own window, holds
/// no document, and has nothing to ask about on the way out. Notepad was tried
/// first and is the wrong choice twice over. Windows 11 ships it as a packaged
/// app, so the window on screen belongs to `ApplicationFrameHost` and this
/// reported "no window to close" for ten seconds at a Notepad that was plainly
/// there; and a text editor is the one program on the machine that might be
/// holding somebody's unsaved work.
#[test]
#[ignore]
fn quit_closes_a_window_the_way_its_own_close_button_does() {
    let pid = detached("charmap.exe", &[]);

    // Its window does not exist the instant it starts, and quitting is about
    // the window rather than about the process.
    let waited = Instant::now();
    let mut said = None;
    let mut complained = false;

    while waited.elapsed() < Duration::from_secs(10) {
        match sill_lib::processes::quit(pid, "charmap.exe") {
            Ok(message) => {
                said = Some(message);
                break;
            }
            Err(why) => {
                // Once. A window that never arrives is one line of output
                // rather than thirty of the same line.
                if !complained {
                    println!("  waiting for a window: {why}");
                    complained = true;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }

    let Some(said) = said else {
        tidy(pid);
        panic!("Character Map never had a window to close");
    };

    println!("  quit said: {said}");

    if !went(pid, "charmap.exe") {
        tidy(pid);
        panic!("Character Map ignored WM_CLOSE");
    }
}

/// Force Quit really does end a process, when it is the right one.
#[test]
#[ignore]
fn force_quit_ends_the_process_it_names() {
    let pid = detached("charmap.exe", &[]);

    let said =
        sill_lib::processes::force_quit(pid, "charmap.exe").expect("this process is endable");

    println!("  force_quit said: {said}");

    if !went(pid, "charmap.exe") {
        tidy(pid);
        panic!("the process is still running");
    }
}

/// A program with no window is told so rather than killed.
///
/// The branch that must never fall through to a terminate: most of a process
/// list has no window, and if Quit quietly ended those then the safe action
/// and the dangerous one would be the same key exactly where it matters most.
#[test]
#[ignore]
fn a_program_with_no_window_cannot_be_asked_to_close() {
    // A console program handed over the same way. `start` gives it a console
    // of its own, which is a window, so it is asked for minimised and headless
    // and then checked: if it turns out to own one, this test says so rather
    // than passing for the wrong reason.
    let pid = detached("ping.exe", &["-n", "60", "127.0.0.1"]);

    let windowed = sill_lib::processes::running()
        .into_iter()
        .find(|process| process.pid == pid)
        .map(|process| process.visible)
        .unwrap_or(false);

    if windowed {
        tidy(pid);
        println!("  skipped: ping opened a console window here, so it is not the case wanted");
        return;
    }

    let refused = sill_lib::processes::quit(pid, "ping.exe")
        .expect_err("a windowless process has nothing to send WM_CLOSE to");

    println!("  quit said: {refused}");
    assert!(refused.contains("Force Quit"), "{refused}");

    if !ids_named("ping.exe").contains(&pid) {
        panic!("Quit ended a process it had just said it could not close");
    }

    tidy(pid);
}
