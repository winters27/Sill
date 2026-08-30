//! The snippet hook's lifetime.
//!
//! An integration test for the same reason the others are: a lib unit-test
//! binary that retains these code paths also retains the dialog plugin, which
//! needs a manifest only test targets can be given. See `build.rs`.
//!
//! This installs a real low-level keyboard hook for a moment. That is the
//! thing under test and there is no way to check it without one; the hook
//! passes every key straight through while it is up, because expansion is
//! never switched on here.

#![cfg(windows)]

use std::time::{Duration, Instant};

use sill_lib::snippets::expander::{arm, armed, stop, Expander};

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
