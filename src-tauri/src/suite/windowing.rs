//! Window enumeration against the real desktop.
//!
//! The layout arithmetic is unit-tested in the module itself, without a
//! desktop. What cannot be tested that way is whether the *filter* is right:
//! whether the list is the windows a person would say are open, or whether it
//! is full of ghosts. That only shows up against a running session, and it is
//! the part of window management that is usually wrong.
//!
//! Every check here is an invariant rather than an exact count, because the
//! desktop these run on is whatever happens to be open at the time.

#![cfg(windows)]

use crate::windowing::{self, Slot};

/// A desktop with nothing on it at all is not a case worth handling.
///
/// If this fails the enumeration is broken, not the machine: something is
/// always open, including whatever is running the test.
fn windows() -> Vec<windowing::Window> {
    let found = windowing::list();
    assert!(
        !found.is_empty(),
        "no windows at all, which means the enumeration found nothing rather than that nothing is open"
    );
    found
}

#[test]
fn every_listed_window_has_a_title() {
    // A window with no caption is not something anyone searches for by name,
    // and a list full of blank rows is the first symptom of a filter that only
    // checks visibility.
    for window in windows() {
        assert!(
            !window.title.trim().is_empty(),
            "a window with no title got through: {window:?}"
        );
    }
}

#[test]
fn every_listed_window_belongs_to_a_real_process() {
    for window in windows() {
        assert!(window.pid != 0, "{} has no process", window.title);
        assert!(
            !window.app.is_empty(),
            "{} has no application name",
            window.title
        );
    }
}

#[test]
fn no_window_claims_a_display_that_does_not_exist() {
    // The monitor index is what every layout call resolves against. An index
    // past the end sends the window nowhere and reports success.
    let displays = windowing::monitors();
    assert!(!displays.is_empty(), "no displays");

    for window in windows() {
        assert!(
            window.monitor < displays.len(),
            "{} claims display {} of {}",
            window.title,
            window.monitor,
            displays.len()
        );
    }
}

#[test]
fn a_window_is_on_the_display_it_overlaps_most() {
    // The assignment rule, checked against the geometry it claims to follow.
    // Minimized windows are exempt: their rectangle is off-screen by design.
    let displays = windowing::monitors();

    for window in windows() {
        if window.minimized {
            continue;
        }

        let claimed = window.rect.overlap(&displays[window.monitor].full);
        for display in &displays {
            assert!(
                window.rect.overlap(&display.full) <= claimed,
                "{} is on display {} but overlaps {} more",
                window.title,
                window.monitor,
                display.index
            );
        }
    }
}

#[test]
fn exactly_one_display_is_primary() {
    let displays = windowing::monitors();
    let primary = displays.iter().filter(|d| d.primary).count();
    assert_eq!(primary, 1, "found {primary} primary displays");
}

#[test]
fn every_work_area_sits_inside_its_display() {
    // The work area is the display minus the taskbar. One that escapes its
    // own display would put windows off-screen and is a sign the two rects
    // were read from different structures.
    for display in windowing::monitors() {
        assert!(
            display.work.x >= display.full.x
                && display.work.y >= display.full.y
                && display.work.right() <= display.full.right()
                && display.work.bottom() <= display.full.bottom(),
            "display {} work {:?} escapes {:?}",
            display.index,
            display.work,
            display.full
        );
        assert!(display.work.width > 0 && display.work.height > 0);
    }
}

#[test]
fn no_two_windows_share_a_handle() {
    // Handles are the identity every action resolves against. A duplicate
    // means the enumeration is visiting something twice and the switcher
    // would show it twice.
    let mut seen = std::collections::HashSet::new();
    for window in windows() {
        assert!(seen.insert(window.id), "{} appears twice", window.title);
    }
}

#[test]
fn a_handle_that_is_not_a_window_is_refused_rather_than_acted_on() {
    // Handles go stale the moment a window closes, and a stale one can be
    // reused by a different window. Every operation has to revalidate, or a
    // "close" arrives at a stranger.
    let nonsense = 0x7fff_0000isize;

    assert!(windowing::find(nonsense).is_none());
    assert!(windowing::focus(nonsense).is_err());
    assert!(windowing::close(nonsense).is_err());
    assert!(windowing::minimize(nonsense).is_err());
    assert!(windowing::maximize(nonsense).is_err());
    assert!(windowing::restore(nonsense).is_err());
    assert!(windowing::place(nonsense, windowing::Rect::new(0, 0, 100, 100)).is_err());
}

#[test]
fn a_slot_lands_inside_the_display_the_window_is_actually_on() {
    // The one that catches a second monitor being ignored: the arithmetic is
    // resolved against a real work area with a real offset rather than an
    // assumed one starting at the origin.
    for display in windowing::monitors() {
        for slot in Slot::ALL {
            let rect = windowing::slot_rect(slot, display.work);
            assert!(
                rect.x >= display.work.x && rect.right() <= display.work.right(),
                "{slot:?} on display {} gave {rect:?} for work {:?}",
                display.index,
                display.work
            );
            assert!(
                rect.y >= display.work.y && rect.bottom() <= display.work.bottom(),
                "{slot:?} on display {} gave {rect:?} for work {:?}",
                display.index,
                display.work
            );
        }
    }
}

#[test]
fn the_visible_frame_is_not_wider_than_the_window() {
    // DWM's extended frame bounds are what the user sees; GetWindowRect
    // includes an invisible resize border around it. The visible frame can
    // never be the larger of the two, and if it is, the two were read from
    // different windows.
    for window in windows().into_iter().filter(|w| !w.minimized).take(12) {
        let Some(frame) = windowing::visible_frame(window.id) else {
            continue;
        };
        assert_eq!(
            frame, window.rect,
            "{} reports two different frames",
            window.title
        );
    }
}
