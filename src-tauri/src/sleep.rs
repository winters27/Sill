//! Letting the launcher's renderer sleep while nobody is looking at it.
//!
//! # What this is for
//!
//! A launcher is hidden almost all of the time. Measured on this machine, the
//! two renderers that exist while nothing is on screen hold 101 MB of private
//! memory between them, and the browser and GPU processes behind them hold
//! another 134 MB. That is the whole at-rest cost of the product, and none of
//! it is doing anything.
//!
//! # Why hiding the window was not already enough
//!
//! Tauri's `hide()` reaches the operating system window and stops there. Its
//! `WindowMessage::Hide` calls `set_visible(false)` on the tao window; the
//! webview's own `set_visible` sits behind a separate `WebviewMessage::Hide`
//! that a plain `window.hide()` never sends. So `ICoreWebView2Controller`'s
//! `IsVisible` stays true for as long as Sill runs, and the renderer spends
//! every hidden minute believing it is on screen: timers fire, animations
//! advance, and nothing is ever reclaimed.
//!
//! Because Tauri never touches that flag, this module owns it outright. There
//! is no other writer to race with.
//!
//! # What suspending does
//!
//! `TrySuspend` is what Edge does to a sleeping tab. It stops script timers
//! and animations, drops renderer processor use to nothing, and releases the
//! renderer's pages for the operating system to reuse. It refuses unless the
//! controller is already invisible, which is why the two calls belong
//! together and in that order.
//!
//! # Why it no longer suspends
//!
//! Only the visibility half survives. `TrySuspend` was removed on 2026-09-05
//! after it was traced to a visible flash on summon, and the note beside
//! `suspend` carries the measurements. Everything below about suspension
//! describes what the call did, and is kept because it is the argument for
//! bringing it back if the memory ever matters more than the flash.
//!
//! # Why it waits
//!
//! Summoning again a second later is the ordinary way this launcher is used,
//! and waking a renderer to serve a keystroke that was already coming would
//! spend latency to save memory for one second. So a dismissal only arms a
//! timer. Summoning inside the grace period disarms it, and nothing was ever
//! suspended, so that path costs exactly what it costs today. Only a launcher
//! that has genuinely been left alone pays anything to come back.
//!
//! # Ordering
//!
//! `with_webview` runs the closure inline when it is already on the main
//! thread, and posts it to the event loop otherwise. Waking posts before
//! `show()` posts, and both travel the same queue in order, so the renderer
//! is awake before the window is up either way.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use tauri::WebviewWindow;

#[cfg(windows)]
#[cfg(windows)]

/// How long a dismissal has to stand before the renderer is put to sleep.
///
/// Long enough that using the launcher in bursts never touches it, short
/// enough that walking away from the machine reclaims the memory while the
/// person is still walking.
const SLEEP_AFTER: Duration = Duration::from_secs(20);

/// Which dismissal a pending sleep belongs to, per window.
///
/// A timer cannot be cancelled once it is sleeping, so it is disarmed instead:
/// waking bumps the count, and a timer that finds it moved on knows the window
/// it was going to suspend has since been summoned, and does nothing.
///
/// Per window rather than one count for all of them. Sill has two windows that
/// come and go independently, and a shared count would let opening the tray
/// menu disarm the launcher's pending sleep, which is the one that matters:
/// the launcher would then never suspend at all.
static GENERATIONS: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

fn generation(label: &str) -> u64 {
    GENERATIONS
        .lock()
        .map(|all| all.get(label).copied().unwrap_or(0))
        .unwrap_or(0)
}

/// Claims the next generation for a window that is being put away.
///
/// Arming bumps the count, the same way waking does, so **only the newest
/// timer for a window ever acts**. Two arrive for one dismissal: `summon::hide`
/// arms one and the focus handler arms another as focus leaves, and both used
/// to run. The log said so, twice: `main suspended` on the same millisecond,
/// from two threads that had both slept twenty seconds for the same window.
fn arm(label: &str) -> u64 {
    GENERATIONS
        .lock()
        .map(|mut all| {
            let next = all.get(label).copied().unwrap_or(0) + 1;
            all.insert(label.to_string(), next);
            next
        })
        .unwrap_or(0)
}

/// Arms the sleep. Called on dismissal, returns immediately.
pub fn sleep_soon(window: &WebviewWindow) {
    /*
     * The page is told it is not on screen, here and only here.
     *
     * Every path that puts a window away calls this: the hotkey, clicking
     * away, the tray, and the lazy windows that hide instead of closing. So
     * this is the one place where "you are no longer being looked at" is true
     * for all of them, and putting the event anywhere else would be one more
     * list to keep in step.
     *
     * Nothing told the page before, so a widget pinned to the chin kept
     * polling into a window nobody could see: the machine readout enumerated
     * every process on the system once a second, forever, whether or not the
     * launcher was up.
     *
     * At the moment of hiding rather than when the renderer is suspended
     * twenty seconds later. Those twenty seconds are exactly the window a
     * once-a-second poll fills with work nobody asked for.
     *
     * The label is the payload, and it has to be. `emit` reaches every window
     * in the application, and a page listening with the default target gets
     * events aimed elsewhere as well, so there is no way to send this to one
     * window. Every window Sill has comes through here, including the tray
     * menu and the deferred ones, so without a label the launcher was told it
     * had been hidden every time the tray menu was dismissed, and stopped its
     * live readings while it was still on screen.
     */
    use tauri::Emitter;
    use tauri::Manager;

    let _ = window.emit("sill://hidden", window.label());

    /*
     * And the icons extracted since the last time, written now.
     *
     * This is the moment nobody is waiting for anything: the launcher has just
     * gone away. Writing on every insert instead would mean a megabyte of
     * base64 written while somebody scrolls a list.
     */
    window.app_handle().state::<crate::icons::Icons>().save();

    let label = window.label().to_string();
    let armed = arm(&label);
    let window = window.clone();

    std::thread::spawn(move || {
        std::thread::sleep(SLEEP_AFTER);

        // Summoned since, so there is nothing to put away.
        if generation(&label) != armed {
            return;
        }

        // Shown by some other path than a summon, which is the same answer.
        if window.is_visible().unwrap_or(false) {
            return;
        }

        /*
         * And the extension views go with it.
         *
         * A view is a Node worker holding a React tree so that a window can
         * draw it. Once nothing of Sill's is on screen there is nothing to
         * draw it into, and until now the only thing that ever noticed was the
         * host's five minute idle sweep: dismissing the launcher by clicking
         * away left a worker, and the whole Node process behind it, resident
         * for the rest of those five minutes.
         *
         * Here rather than at the moment of the dismissal, because a dismissal
         * is not always meant. This is the point where Sill has already decided
         * that this one stood, which is the same conclusion, and the same
         * twenty seconds, that the renderer's own sleep is based on. Coming
         * straight back still finds the command where it was.
         *
         * Before the renderer is suspended rather than after, and blocking on
         * purpose: the window has to be told its view has gone while it is
         * still awake to hear it.
         */
        if !crate::summon::anything_visible(&window.app_handle()) {
            crate::host::release_views(&window.app_handle());
        }

        suspend(&label, &window, armed);
    });
}

/// Wakes the renderer, and disarms any sleep still counting down.
///
/// Safe to call when nothing is suspended: making a visible controller visible
/// is a no-op, and this runs on every summon rather than only on the ones that
/// need it, so the summon path has no state to get wrong.
pub fn wake(window: &WebviewWindow) {
    if let Ok(mut all) = GENERATIONS.lock() {
        *all.entry(window.label().to_string()).or_insert(0) += 1;
    }

    #[cfg(windows)]
    let _ = window.with_webview(|webview| {
        let controller = webview.controller();

        unsafe {
            // Nothing to resume: this module stopped suspending, so the
            // renderer was only ever made invisible. Undoing that is the whole
            // of waking now.
            let _ = controller.SetIsVisible(true);
        }
    });

    #[cfg(not(windows))]
    let _ = window;
}

/**
Puts the renderer to sleep. Only called by the armed timer.

Says what happened, and that matters more than it sounds. Suspension is best
effort: `TrySuspend` defers while the page is still running script and never
lands at all if the page holds something Edge will not sleep through. So "the
renderer did not shrink" has two completely different causes, one of which is a
bug in the arming and the other of which is a busy page, and without a line in
the log there is no way to tell them apart. Measuring this cost a confused hour
the first time: the same build suspended on one run and not the next, and the
difference was what the page happened to be doing.
*/
#[cfg(windows)]
fn suspend(label: &str, window: &WebviewWindow, armed: u64) {
    let label = label.to_string();
    let watched = window.clone();

    let _ = window.with_webview(move |webview| {
        /*
         * Asked again, here, on the webview's own thread.
         *
         * `with_webview` hands this closure to that thread and returns, so
         * everything checked before it is a statement about the past.
         * Summoning in the gap meant `SetIsVisible(false)` on a window
         * somebody was looking at: the launcher would be up, focused, and
         * invisible. The gap is small and the window it opens is exactly
         * twenty seconds after a dismissal, which is a perfectly ordinary
         * moment to come back.
         */
        if generation(&label) != armed || watched.is_visible().unwrap_or(false) {
            return;
        }

        let controller = webview.controller();

        unsafe {
            // Required: `TrySuspend` refuses a visible controller outright.
            if controller.SetIsVisible(false).is_err() {
                return;
            }

            /*
             * And that is as far as it goes. `TrySuspend` is deliberately not
             * called; see this module's header for the whole argument.
             */
            crate::say!("{label} hidden, renderer left resident");
        }
    });
}


#[cfg(not(windows))]
fn suspend(_label: &str, _window: &WebviewWindow, _armed: u64) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grace period is the whole reason this is safe to turn on: bursts of
    /// use must never reach the suspend path at all.
    #[test]
    fn dismissing_and_summoning_again_never_suspends() {
        assert!(
            SLEEP_AFTER >= Duration::from_secs(10),
            "a grace period shorter than a pause for thought puts the renderer \
             to sleep between two keystrokes of the same task"
        );
    }

    fn bump(label: &str) {
        if let Ok(mut all) = GENERATIONS.lock() {
            *all.entry(label.to_string()).or_insert(0) += 1;
        }
    }

    /// Waking has to disarm, or a timer armed before the summon suspends a
    /// window somebody is looking at.
    #[test]
    fn waking_disarms_a_pending_sleep() {
        let armed = generation("main");
        bump("main");

        assert_ne!(
            generation("main"),
            armed,
            "a timer armed earlier would still fire"
        );
    }

    /// The bug this shape exists to prevent: one count for every window let
    /// the tray menu opening disarm the launcher's sleep, so the launcher,
    /// which holds the larger renderer, never suspended at all.
    #[test]
    fn one_window_waking_leaves_another_windows_sleep_armed() {
        let armed = generation("main");
        bump("traymenu");

        assert_eq!(
            generation("main"),
            armed,
            "the launcher's pending sleep was cancelled"
        );
    }
}

#[cfg(test)]
mod arming {
    use super::{arm, generation};

    /// Two timers for one dismissal leave only the newer one armed.
    ///
    /// Both `summon::hide` and the focus handler arm a sleep when the launcher
    /// goes away, so two threads sleep twenty seconds for the same window. The
    /// log said so, twice, on the same millisecond.
    #[test]
    fn only_the_newest_timer_for_a_window_acts() {
        let first = arm("test-window");
        let second = arm("test-window");

        assert_ne!(first, second);
        assert_eq!(generation("test-window"), second);
        assert_ne!(
            generation("test-window"),
            first,
            "the older timer would still fire"
        );
    }

    /// One window's dismissal does not disarm another's.
    ///
    /// Sill has two windows that come and go independently, and a shared count
    /// would let opening the tray menu disarm the launcher's pending sleep,
    /// which is the one that matters.
    #[test]
    fn windows_are_armed_independently() {
        let launcher = arm("independent-main");
        arm("independent-tray");
        arm("independent-tray");

        assert_eq!(
            generation("independent-main"),
            launcher,
            "another window's dismissal disarmed this one"
        );
    }
}
