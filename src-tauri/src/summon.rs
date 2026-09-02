//! Showing and hiding the launcher, and giving focus back when it goes away.
//!
//! A launcher is judged on this more than on anything else it does. It has to
//! appear instantly, take the keyboard, and on dismissal leave the focus
//! exactly where it was, so using it never costs the user their place.

use std::sync::Mutex;

use tauri::{Emitter, Manager, WebviewWindow};

use crate::preferences::Backdrop;

#[cfg(windows)]
use windows::Win32::{
    Foundation::HWND,
    // AttachThreadInput lives under System::Threading in the windows crate,
    // not under UI::Input where the Win32 docs group it.
    System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId},
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow},
};

/// The window that had focus before we took it.
///
/// Stored as a raw handle rather than anything richer because it is only ever
/// handed straight back to Win32, and the window may well be gone by then,
/// which `SetForegroundWindow` reports rather than crashing on.
#[cfg(windows)]
static PREVIOUS_FOREGROUND: Mutex<Option<isize>> = Mutex::new(None);

#[cfg(not(windows))]
static PREVIOUS_FOREGROUND: Mutex<Option<isize>> = Mutex::new(None);

/// Puts real desktop blur behind the launcher.
///
/// This has to be the OS compositor. Nothing inside the page can blur what is
/// behind the window, because the page cannot see it: `backdrop-filter` only
/// reaches other page content, and WebGPU glass libraries only composite their
/// own scene. Acrylic asks Windows to sample the desktop itself.
///
/// Acrylic rather than Mica: Mica tints from the wallpaper and is meant for
/// long-lived app windows, while acrylic blurs whatever is actually behind it,
/// which is the point for something that appears over your work.
///
/// Failure is not fatal. On a machine or build without it, the window simply
/// renders on its solid background.
pub fn apply_backdrop(window: &WebviewWindow, backdrop: Backdrop, tint_alpha: u8) {
    #[cfg(windows)]
    {
        // Rounding comes first. The backdrop sheet and the drop shadow are
        // drawn by DWM on the window, and a window is a rectangle: without
        // this they render square corners behind the page's rounded card,
        // which reads as a hazy box sticking out past the radius.
        round_corners(window);

        // Clear whatever was applied before, so switching modes at runtime
        // does not stack two materials on one window.
        let _ = window_vibrancy::clear_acrylic(window);
        let _ = window_vibrancy::clear_blur(window);

        let tint = (10, 10, 11, tint_alpha);

        let result = match backdrop {
            Backdrop::Acrylic => window_vibrancy::apply_acrylic(window, Some(tint)),
            Backdrop::Blur => window_vibrancy::apply_blur(window, Some(tint)),
            Backdrop::None => {
                println!("[sill] no backdrop: the page paints its own surface");
                return;
            }
        };

        match result {
            Ok(()) => println!("[sill] {backdrop:?} backdrop applied, tint alpha {tint_alpha}"),
            Err(err) => crate::say!("no {backdrop:?} backdrop: {err}"),
        }
    }
}

/// Asks DWM to clip the window to rounded corners.
///
/// Everything the OS draws for a window (the backdrop sheet, the shadow) is
/// clipped to this shape, so it is what makes those agree with the page's own
/// border radius instead of squaring off behind it.
///
/// `DWMWCP_ROUND` is the system radius, which the page matches rather than
/// picking its own. Two different radii is the very mismatch being fixed.
#[cfg(windows)]
fn round_corners(window: &WebviewWindow) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let Ok(handle) = window.hwnd() else { return };
    let hwnd = HWND(handle.0 as *mut core::ffi::c_void);
    let preference = DWMWCP_ROUND;

    // SAFETY: the attribute id and the size match the DWM contract, and the
    // value outlives the call. Older Windows returns an error rather than
    // faulting, which is why the result is only logged.
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&preference) as u32,
        )
    };

    match result {
        Ok(()) => println!("[sill] window corners rounded by DWM"),
        Err(err) => crate::say!("could not round window corners: {err}"),
    }
}

/// Whether a window is worth handing focus back to on dismissal.
///
/// One of Sill's own never is. Windows that spend their lives hidden still hold
/// the foreground if they are ever given it, and handing focus to a window
/// nobody can see is a dead end: the application the user actually came from
/// never gets it back, and their next keystroke goes nowhere.
///
/// That is not hypothetical. The tray menu is created hidden, and until its
/// config said otherwise it took the foreground at startup and kept it. So
/// every dismissal handed focus to an invisible window, and the launcher had to
/// be clicked before it would take a keystroke or a scroll.
///
/// The config change stops that one window. This stops all of them, including
/// whichever hidden window gets added next.
#[cfg(windows)]
fn worth_returning_to(window_pid: u32, our_pid: u32) -> bool {
    window_pid != 0 && window_pid != our_pid
}

/// Records whatever currently has focus, so it can be restored on dismissal.
/// The window that had focus before the launcher took it.
///
/// Read by placement, which needs the screen that window was on. Only ever
/// handed straight back to Win32, so a handle that has since closed is the
/// caller's problem to survive rather than something to check here.
#[cfg(windows)]
pub(crate) fn previous_foreground() -> Option<isize> {
    *PREVIOUS_FOREGROUND
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn remember_foreground() {
    #[cfg(windows)]
    {
        // SAFETY: GetForegroundWindow takes nothing and returns a handle or
        // null. Nothing is dereferenced.
        let hwnd = unsafe { GetForegroundWindow() };

        let worth_keeping = if hwnd.0.is_null() {
            None
        } else {
            let mut pid = 0u32;
            // SAFETY: the handle came from GetForegroundWindow, and `pid` is a
            // local the call only writes into.
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            // SAFETY: takes nothing, returns this process's id.
            let ours = unsafe { GetCurrentProcessId() };

            worth_returning_to(pid, ours).then_some(hwnd.0 as isize)
        };

        let mut slot = PREVIOUS_FOREGROUND
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *slot = worth_keeping;
    }
}

/// Drops the remembered window, so dismissal leaves focus where it is.
///
/// **Required after anything that deliberately focuses another window.**
/// Dismissal restores whatever was in front before the launcher appeared, so
/// without this, switching to a window works and is then immediately undone by
/// the launcher getting out of the way. Measured: the switcher restored a
/// minimized window, focused it, and control landed back on the window the
/// user had summoned Sill from.
///
/// Launching an application usually survives this by accident, because the new
/// process takes the foreground after the restore has already run. Focusing a
/// window that already exists loses that race every time.
pub fn forget_foreground() {
    #[cfg(windows)]
    {
        let mut slot = PREVIOUS_FOREGROUND
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *slot = None;
    }
}

/// Hands focus back to whatever had it before the launcher appeared.
pub fn restore_foreground() {
    #[cfg(windows)]
    {
        let previous = PREVIOUS_FOREGROUND
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();

        if let Some(raw) = previous {
            let hwnd = HWND(raw as *mut core::ffi::c_void);
            // SAFETY: the handle came from GetForegroundWindow. If the window
            // has since closed, SetForegroundWindow returns false rather than
            // faulting, which is why the result is ignored.
            unsafe {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

/// Brings a window to the front, working around Windows' focus-stealing rules.
///
/// `SetForegroundWindow` is refused for a process that does not own the
/// current foreground window. Attaching to that window's input queue for the
/// duration of the call makes Windows treat the change as user-driven, which
/// is the standard approach and what every launcher on this platform does.
#[cfg(windows)]
pub(crate) fn force_foreground(hwnd: HWND) {
    // SAFETY: all handles and thread ids come from Win32 itself, and the
    // attach is unwound before returning in every path.
    unsafe {
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let this_thread = GetCurrentThreadId();

        if foreground_thread != 0 && foreground_thread != this_thread {
            let _ = AttachThreadInput(foreground_thread, this_thread, true);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(foreground_thread, this_thread, false);
        } else {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Shows the launcher and takes the keyboard.
pub fn show(window: &WebviewWindow) {
    // Before anything, so what is measured is what somebody waited for rather
    // than what was left after the parts that were easy to instrument.
    let timings = window.app_handle().try_state::<crate::timing::Timings>();
    if let Some(timings) = timings.as_deref() {
        timings.summon_began();
    }

    remember_foreground();

    /*
     * Placed before it is shown, so it appears where it belongs rather than
     * appearing and then moving. After `remember_foreground`, because placing
     * by the active window needs the window that was active a moment ago.
     */
    if let Some(placement) = window
        .app_handle()
        .try_state::<crate::placement::Placement>()
    {
        crate::placement::centre_for_summon(window, placement.get());
    }

    // Before the window goes up, so the renderer is awake by the time there is
    // something to paint. Ordinarily there is nothing to wake and this costs a
    // disarmed timer.
    crate::sleep::wake(window);

    let _ = window.show();
    let _ = window.set_focus();

    // Tauri exposes an HWND from its own pinned `windows` version, which is a
    // different type to ours even though the value is identical. The raw
    // pointer is the common ground.
    #[cfg(windows)]
    if let Ok(handle) = window.hwnd() {
        force_foreground(HWND(handle.0 as *mut core::ffi::c_void));
    }

    /*
     * Window focus is not the same as focus inside the page.
     *
     * The window can be foreground and still have nothing in the document
     * focused, which leaves the user typing into nowhere and unable to move
     * the selection until they click. The page has to put focus back in the
     * search field on every summon, not just once at startup while the window
     * is still hidden.
     */
    let _ = window.emit("sill://shown", ());

    // The window is up and the page has been told. The other half of the
    // number comes back from the page, because only it knows when it painted.
    if let Some(timings) = timings.as_deref() {
        timings.summon_shown();
    }
}

/// Shows the launcher already in the window switcher.
///
/// Always shows rather than toggling. A switcher key is pressed to get
/// somewhere, and a second press meaning "put it away" would make holding the
/// key to look through the list impossible.
///
/// The event goes out after the window is up, and the page decides what the
/// switcher looks like. Rust knows which windows exist; it has no business
/// knowing which screen the launcher is showing.
pub fn show_switcher(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    show(&window);
    let _ = window.emit("sill://switcher", ());
}

/// Shows the launcher, optionally asking it to run something on arrival.
///
/// The same shape as `show_switcher`: Rust puts the window up and says what
/// was asked for, and the page decides what that looks like. A tray entry that
/// summoned the launcher and then left the user to type the command name again
/// would not be worth having.
///
/// Always shows rather than toggling. Choosing "Clipboard History" from a menu
/// means "put that on screen", never "put the launcher away".
pub fn show_with(app: &tauri::AppHandle, command: Option<String>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    show(&window);

    if let Some(id) = command {
        let _ = window.emit("sill://run", id);
    }
}

/// Hides the launcher and returns focus.
pub fn hide(window: &WebviewWindow) {
    let _ = window.hide();
    restore_foreground();

    // Arms a timer rather than suspending now: coming straight back is the
    // ordinary way this is used, and that path must stay free.
    crate::sleep::sleep_soon(window);
}

/// Summons or dismisses, depending on what the window is doing now.
///
/// Visibility alone is not enough: the window can be visible but behind
/// something else, and in that case the user pressing the hotkey means "bring
/// it here", not "put it away".
pub fn toggle(window: &WebviewWindow) {
    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);

    if visible && focused {
        hide(window);
    } else {
        show(window);
    }
}

/// Whether any of Sill's windows is on screen.
///
/// Asked by the readings a widget polls for, so a poll that outlives the
/// window it was drawn in costs a visibility check rather than a walk of every
/// process on the machine. The window layer already stops polling when it is
/// told it was hidden; this is the same rule stated where it cannot be
/// forgotten by a second caller, which for these two readings includes an
/// extension.
///
/// Every window rather than the launcher alone: the readout can be pinned in
/// the launcher's chin and it can also be a view of its own, and a reading is
/// worth taking if anything is showing it.
pub fn anything_visible(app: &tauri::AppHandle) -> bool {
    app.webview_windows()
        .values()
        .any(|window| window.is_visible().unwrap_or(false))
}

/// Convenience for handlers that only have the app handle.
pub fn toggle_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        toggle(&window);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::worth_returning_to;

    /// Dismissing the launcher has to give focus back to the application the
    /// user came from, and one of Sill's own windows is never that.
    #[test]
    fn sills_own_windows_are_not_somewhere_to_return_to() {
        assert!(!worth_returning_to(4321, 4321));
    }

    #[test]
    fn another_application_is() {
        assert!(worth_returning_to(1234, 4321));
    }

    /// A window that reports no owning process is gone or going.
    #[test]
    fn a_window_with_no_process_is_not() {
        assert!(!worth_returning_to(0, 4321));
    }
}
