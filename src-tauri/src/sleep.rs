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
use webview2_com::{Microsoft::Web::WebView2::Win32::ICoreWebView2_3, TrySuspendCompletedHandler};
#[cfg(windows)]
use webview2_core::Interface;

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

/// Arms the sleep. Called on dismissal, returns immediately.
pub fn sleep_soon(window: &WebviewWindow) {
    let label = window.label().to_string();
    let armed = generation(&label);
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

        suspend(&label, &window);
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
            // Resume first. Made visible while still suspended, the renderer
            // would resume on its own, but explicitly is one less thing that
            // depends on a documented side effect.
            if let Ok(core) = controller.CoreWebView2() {
                if let Ok(suspendable) = core.cast::<ICoreWebView2_3>() {
                    let _ = suspendable.Resume();
                }
            }

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
fn suspend(label: &str, window: &WebviewWindow) {
    let label = label.to_string();

    let _ = window.with_webview(|webview| {
        let controller = webview.controller();

        unsafe {
            // Required: `TrySuspend` refuses a visible controller outright.
            if controller.SetIsVisible(false).is_err() {
                return;
            }

            let Ok(core) = controller.CoreWebView2() else {
                return;
            };
            let Ok(suspendable) = core.cast::<ICoreWebView2_3>() else {
                return;
            };

            // Best effort by design. A page still running script suspends when
            // that script finishes, and one holding something Edge will not
            // sleep through never suspends at all. Both report through this
            // handler and neither is worth acting on: the window is hidden and
            // invisible either way, which is most of the saving.
            let handler =
                TrySuspendCompletedHandler::create(Box::new(move |_result, suspended| {
                    if suspended {
                        crate::say!("{label} suspended");
                    } else {
                        // Not a failure. The page was busy, and it will be asked
                        // again the next time it is put away.
                        crate::say!("{label} would not suspend, the page is busy");
                    }
                    Ok(())
                }));
            let _ = suspendable.TrySuspend(&handler);
        }
    });
}

#[cfg(not(windows))]
fn suspend(_label: &str, _window: &WebviewWindow) {}

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
