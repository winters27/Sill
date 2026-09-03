//! The snippet hook's lifetime.
//!
//! This installs a real low-level keyboard hook for a moment. That is the
//! thing under test and there is no way to check it without one; the hook
//! passes every key straight through while it is up, because expansion is
//! never switched on here.
//!
//! Which is why it is still a binary of its own, now that the rest of the
//! suite has moved into the library. The header used to give the dialog-plugin
//! manifest as the reason and that is not the reason: `WH_KEYBOARD_LL` is
//! system-wide while it is installed, and **Windows silently removes a hook
//! that takes too long to answer**. Arming one inside a process that is running
//! fifteen hundred other tests in parallel makes every keystroke on the machine
//! wait behind them, and makes the thing under test depend on how busy the
//! machine is. A process that does nothing else is worth one link.

#![cfg(windows)]

use std::time::{Duration, Instant};

use sill_lib::snippets::expander::{arm, armed, facts, stop, Expander};

/// Waits for the hook's thread to reach a state, rather than sleeping and
/// hoping. Starting and stopping a thread are not instant, and a fixed sleep
/// is either flaky or slow.
fn settle(expander: &Expander, want: bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if armed(expander) == want {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    armed(expander) == want
}

#[test]
fn switching_expansion_off_takes_the_hook_out_rather_than_muting_it() {
    // The whole point of the change. A low-level keyboard hook is called for
    // every keystroke on the machine, in every application. Leaving it
    // installed and teaching it to ignore everything means Sill is still in
    // the path of every key the user presses in order to do nothing.
    let expander = Expander::new();
    assert!(!armed(&expander), "armed before anything started it");

    arm(&expander);
    assert!(settle(&expander, true), "the hook never went in");

    stop(&expander);
    assert!(settle(&expander, false), "the hook is still installed");
}

#[test]
fn arming_twice_does_not_start_two_hooks() {
    // Every settings save calls this, so it is reached far more often than it
    // is meant to do anything. Two hooks would mean every keystroke handled
    // twice, and only one of them ever removed.
    let expander = Expander::new();

    arm(&expander);
    assert!(settle(&expander, true));

    arm(&expander);
    arm(&expander);

    stop(&expander);
    assert!(
        settle(&expander, false),
        "one stop did not undo it, so more than one was started"
    );
}

#[test]
fn stopping_something_that_was_never_started_is_harmless() {
    // Reached on every save while expansion has always been off, which is the
    // ordinary case for anyone not using snippets.
    let expander = Expander::new();

    stop(&expander);
    stop(&expander);

    assert!(!armed(&expander));
}

#[test]
fn it_can_be_started_again_after_being_stopped() {
    // Switching a setting off and back on is a thing people do, and a stop
    // that left the running flag set would make the second start silently do
    // nothing.
    let expander = Expander::new();

    arm(&expander);
    assert!(settle(&expander, true));
    stop(&expander);
    assert!(settle(&expander, false));

    arm(&expander);
    assert!(settle(&expander, true), "it would not start a second time");

    stop(&expander);
    assert!(settle(&expander, false));
}

/// The hook reports what it believes and what actually happened, separately.
///
/// Windows removes a low-level hook whose callback runs long and tells nobody:
/// the thread stays parked, the handle stays valid, and `armed` keeps
/// answering true while every keyword quietly stops firing. A count beside the
/// flag is the only thing that can tell that apart from "the keyword is
/// wrong", which is why the dictation hook has had one since the day its
/// trigger died for two silent reasons at once.
///
/// Nothing here types, so the count stays where it starts. What is asserted is
/// that the two facts are reported independently, which is the property the
/// diagnosis needs.
#[test]
fn the_hook_reports_installation_and_keystrokes_separately() {
    let expander = Expander::new();

    let (installed, before) = facts(&expander);
    assert!(!installed, "nothing is installed yet");

    arm(&expander);
    assert!(settle(&expander, true));

    let (installed, after) = facts(&expander);
    assert!(installed, "it says it is installed");
    assert!(
        after >= before,
        "the count never goes backwards, so a reading can be compared with an \
         earlier one to see whether keys are still arriving"
    );

    stop(&expander);
    assert!(settle(&expander, false));

    let (installed, _) = facts(&expander);
    assert!(!installed, "and it stops claiming to be installed");
}
