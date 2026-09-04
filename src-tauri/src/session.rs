//! When the machine comes back.
//!
//! Windows announces a resume from sleep and an unlocked session by
//! broadcasting a message to every top-level window. Sill already owns one that
//! lives for as long as the process does, so this listens on that rather than
//! creating a window and a thread of its own: `SetWindowSubclass` is the
//! documented way to add a handler to somebody else's window procedure, and it
//! costs nothing when no message arrives, which is nearly always.
//!
//! There is exactly one reason to listen. **A low-level keyboard hook does not
//! survive being taken away**, and Windows takes them away silently around
//! sleep. [`crate::hooks`] is what decides and what puts them back; this is
//! only the part that knows when to ask.
//!
//! ## Two messages that are deliberately not handled here
//!
//! **`WM_DISPLAYCHANGE`.** `P1-10` decided against a handler and that decision
//! still holds, now that having one would be four lines. The launcher is placed
//! on every summon, so a window stranded off-screen by a display change is
//! already recovered by the next summon, and recovered for every cause rather
//! than the one Windows announces. Handling it would also be wrong in the one
//! case it covers, a display change while the launcher is on screen: where the
//! launcher belongs depends on where the cursor was **when it was summoned**,
//! so re-centring on a display change would move it, under the user's hands
//! mid-keyword, to a place they never asked for.
//!
//! **`WM_DPICHANGED`.** The window layer already handles it, and correctly: the
//! launcher is sized in logical pixels, so Windows resizes it to keep that size
//! when it meets a screen at a different scale. The consequence, that the size
//! read before a move is the size on the old screen, is absorbed by
//! [`crate::placement::centre_for_summon`], which places twice for exactly this
//! reason. A handler here would be a second opinion about a resize already in
//! progress, which is how a window ends up flickering between two positions.

/// Starts listening for the machine coming back.
#[cfg(windows)]
pub fn watch(window: &tauri::WebviewWindow) {
    windows_impl::watch(window);
}

#[cfg(not(windows))]
pub fn watch(_window: &tauri::WebviewWindow) {}

#[cfg(windows)]
mod windows_impl {
    use std::sync::OnceLock;

    use tauri::{AppHandle, Manager, WebviewWindow};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::RemoteDesktop::{
        WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
    };
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
        WTS_SESSION_UNLOCK,
    };

    /// The app the handler acts on.
    ///
    /// A window procedure is a bare function pointer, so what it needs has to
    /// be reachable without a closure environment. The same shape both keyboard
    /// hooks already use, and for the same reason.
    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// Tells this subclass apart from anybody else's on the same window.
    ///
    /// The pair of the procedure address and this number is the identity
    /// Windows keys a subclass by, so a fixed value here also means installing
    /// twice replaces rather than stacks.
    const ID: usize = 0x5111;

    pub(super) fn watch(window: &WebviewWindow) {
        let Ok(handle) = window.hwnd() else {
            crate::say!("no window to watch for sleep and resume on");
            return;
        };

        // Tauri hands back an HWND from its own pinned `windows` version, which
        // is a different type to ours with an identical value. The raw pointer
        // is the common ground, as it is everywhere else that crosses this line.
        let hwnd = HWND(handle.0 as *mut core::ffi::c_void);

        let _ = APP.set(window.app_handle().clone());

        // SAFETY: the handle belongs to a window of this process on the thread
        // this runs on, which is what `SetWindowSubclass` requires, and the
        // procedure defers everything it does not recognise.
        let installed = unsafe { SetWindowSubclass(hwnd, Some(watcher), ID, 0) };

        if !installed.as_bool() {
            crate::say!("could not watch for sleep and resume");
            return;
        }

        /*
         * Unlocking, as well as waking.
         *
         * Power messages cover a machine that really suspended. A laptop on
         * modern standby often does not send them, and coming back to it is a
         * session unlock instead. Both are "the user went away and came back",
         * which is the moment hooks are found missing, so both ask.
         *
         * Nothing unregisters this: the window lives as long as the process and
         * Windows drops the registration with it.
         */
        // SAFETY: registers this process's own window for notifications about
        // its own session.
        if let Err(err) = unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) } {
            crate::say!("could not watch for the session being unlocked: {err}");
        }
    }

    /// Runs before the window's own procedure, for every message it gets.
    ///
    /// **Two messages are looked at and everything else is handed straight on.**
    /// This sits in front of the launcher's whole message stream, including
    /// every paint and every keystroke, so anything it did per message it would
    /// do thousands of times a second.
    unsafe extern "system" fn watcher(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        // SAFETY: the arguments are the ones Windows handed in, unchanged.
        let next = || unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };

        let woke = match message {
            WM_POWERBROADCAST => {
                let event = wparam.0 as u32;

                /*
                 * Both resume events, because which one arrives depends on how
                 * the machine went to sleep.
                 *
                 * `PBT_APMRESUMESUSPEND` says a person is here; the automatic
                 * one also fires when the machine woke itself for a scheduled
                 * task. Acting on both is right: a hook Windows removed is gone
                 * whether or not anybody is at the keyboard, and putting it back
                 * before they sit down is the whole point.
                 */
                event == PBT_APMRESUMEAUTOMATIC || event == PBT_APMRESUMESUSPEND
            }
            WM_WTSSESSION_CHANGE => wparam.0 as u32 == WTS_SESSION_UNLOCK,
            _ => return next(),
        };

        if !woke {
            return next();
        }

        if let Some(app) = APP.get() {
            crate::say!("the machine came back, checking the keyboard hooks");
            crate::hooks::check(app, crate::hooks::Cause::Woke);
        }

        next()
    }
}
