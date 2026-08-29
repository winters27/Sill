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
    System::Threading::{AttachThreadInput, GetCurrentThreadId},
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

/// Records whatever currently has focus, so it can be restored on dismissal.
pub fn remember_foreground() {
    #[cfg(windows)]
    {
        // SAFETY: GetForegroundWindow takes nothing and returns a handle or
        // null. Nothing is dereferenced.
        let hwnd = unsafe { GetForegroundWindow() };
        let mut slot = PREVIOUS_FOREGROUND
            .lock()
            .expect("foreground slot poisoned");
        *slot = if hwnd.0.is_null() {
            None
        } else {
            Some(hwnd.0 as isize)
        };
    }
}

/// Hands focus back to whatever had it before the launcher appeared.
pub fn restore_foreground() {
    #[cfg(windows)]
    {
        let previous = PREVIOUS_FOREGROUND
            .lock()
            .expect("foreground slot poisoned")
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
fn force_foreground(hwnd: HWND) {
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
    remember_foreground();

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
}

/// Hides the launcher and returns focus.
pub fn hide(window: &WebviewWindow) {
    let _ = window.hide();
    restore_foreground();
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

/// Convenience for handlers that only have the app handle.
pub fn toggle_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        toggle(&window);
    }
}
