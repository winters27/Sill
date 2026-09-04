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
///
/// Says whether the window really ended up in front. It used to throw that
/// answer away, and a launcher that is up but has not got the keyboard looks
/// exactly like a launcher that is broken: the user types and the characters go
/// to whatever they were doing before.
#[cfg(windows)]
pub(crate) fn force_foreground(hwnd: HWND) -> bool {
    // SAFETY: all handles and thread ids come from Win32 itself, and the
    // attach is unwound before returning in every path.
    unsafe {
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let this_thread = GetCurrentThreadId();

        let taken = if foreground_thread != 0 && foreground_thread != this_thread {
            let _ = AttachThreadInput(foreground_thread, this_thread, true);
            let taken = SetForegroundWindow(hwnd).as_bool();
            let _ = AttachThreadInput(foreground_thread, this_thread, false);
            taken
        } else {
            SetForegroundWindow(hwnd).as_bool()
        };

        // Asked again rather than trusted, because the call returns false in
        // cases where the change went through anyway. Reporting a problem the
        // user does not have is worse than reporting nothing.
        taken || GetForegroundWindow() == hwnd
    }
}

/// Why the launcher is on screen without the keyboard.
///
/// Only reasons Sill can name. There is no `Unknown`, deliberately: a trouble
/// the reader cannot act on teaches them the surface is noise, and a failed
/// `SetForegroundWindow` on its own is not evidence of anything they could do
/// something about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// The window in front belongs to a program running with more privilege
    /// than Sill, and Windows will not let a lower one take the keyboard from
    /// it. This is also why the summon key itself does nothing while such a
    /// window has focus.
    Elevated,
    /// Something is running full screen or presenting, so it owns the display
    /// and the launcher is behind it.
    FullScreen,
}

/// What to tell the user about a summon that did not get the keyboard.
///
/// Its own function so the rule can be read and tested without a screen, a
/// keyboard or an administrator. The order matters: an elevated window in front
/// is the case the user can do something about, and a full-screen game running
/// as administrator is both.
///
/// **The two readings are asked for rather than handed over.** Each is a call
/// into Windows, and this runs between a key being pressed and the launcher
/// being on screen, which is the one path in this application that may not
/// spend anything it does not have to. On the summon that worked, which is
/// every summon, neither is asked.
pub fn blocked_by(
    took_focus: bool,
    elevated: impl FnOnce() -> bool,
    full_screen: impl FnOnce() -> bool,
) -> Option<Blocked> {
    if took_focus {
        return None;
    }

    if elevated() {
        return Some(Blocked::Elevated);
    }

    if full_screen() {
        return Some(Blocked::FullScreen);
    }

    None
}

/// The one focus trouble, named once so the report and the withdrawal cannot
/// disagree about which failure they mean.
const FOREGROUND_TROUBLE: &str = "summon-foreground";

/// Says out loud that the launcher is up but has not got the keyboard.
#[cfg(windows)]
fn report_focus(window: &WebviewWindow, took_focus: bool) {
    let app = window.app_handle();

    // Cheap, and it is the whole happy path: nothing was wrong, so nothing is
    // said, and anything said last time stops being said.
    let Some(blocked) = blocked_by(took_focus, foreground_is_elevated, presenting) else {
        // A refusal with no cause to name still belongs in the log, which is
        // the only place with a timestamp and an ordering and is what a bug
        // report needs. It does not belong on the surface, which is for things
        // the reader can act on.
        if !took_focus {
            crate::say!("the launcher did not get the foreground, and nothing says why");
        }

        crate::status::resolved(app, FOREGROUND_TROUBLE);
        return;
    };

    let message = match blocked {
        Blocked::Elevated => {
            "The window in front is running as administrator, so Windows will not let Sill \
             take the keyboard from it. Sill has to be started as administrator too to work \
             over that program."
        }
        Blocked::FullScreen => {
            "A program is running full screen, so the launcher opened behind it and did not \
             get the keyboard."
        }
    };

    crate::status::report(app, FOREGROUND_TROUBLE, message, None);
}

/// Whether the window in front belongs to a process this one may not touch.
///
/// Asked by trying to open it for the least a process can be opened for. A
/// refusal is what a higher integrity level looks like from below, and it is
/// the same refusal that stops the summon key reaching Sill at all while such a
/// window has focus.
#[cfg(windows)]
fn foreground_is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, E_ACCESSDENIED};
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    // SAFETY: the window handle comes from Windows and the process id is a
    // local the call only writes into.
    unsafe {
        let foreground = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(foreground, Some(&mut pid));

        if pid == 0 || pid == GetCurrentProcessId() {
            return false;
        }

        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let _ = CloseHandle(handle);
                false
            }
            // Only a refusal. A process that has since exited fails differently
            // and is not something to tell anybody about.
            Err(err) => err.code() == E_ACCESSDENIED,
        }
    }
}

/// Whether something owns the display.
///
/// The shell's own answer, which is what decides whether Windows itself will
/// show a notification, so it already covers exclusive full-screen Direct3D,
/// a full-screen store app and presentation mode.
#[cfg(windows)]
fn presenting() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE,
        QUNS_RUNNING_D3D_FULL_SCREEN,
    };

    // SAFETY: takes nothing and returns a value.
    let state = unsafe { SHQueryUserNotificationState() };

    let Ok(state) = state else {
        return false;
    };

    state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE
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
        let took_focus = force_foreground(HWND(handle.0 as *mut core::ffi::c_void));

        // A launcher that is on screen without the keyboard is the failure this
        // whole file exists to prevent, and until now it happened in silence.
        report_focus(window, took_focus);
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
    // Carrying the label, for the reason `sleep::sleep_soon` explains about
    // the other half of this pair: an emit reaches every window, so the page
    // has to be able to tell whether it is the one being talked about.
    let _ = window.emit("sill://shown", window.label());

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

/// Shows the launcher on the welcome, the first time Sill runs here.
///
/// The same shape as `show_switcher`: Rust puts the window up and says what it
/// was opened for, and the page decides what that looks like.
///
/// The event is not the only way the page finds out, and it must not be. Tauri
/// creates this window before the `setup` hook runs, so the page may still be
/// loading and have no listener yet when this fires. It asks on mount as well,
/// and `FirstRun` hands the welcome to whichever of the two arrives after the
/// summon key has been registered.
pub fn show_welcome(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    show(&window);
    let _ = window.emit("sill://welcome", ());
}

/// Hides the launcher and returns focus.
pub fn hide(window: &WebviewWindow) {
    let _ = window.hide();
    restore_foreground();
    went_away(window);
}

/// Everything that has to happen however the launcher went away.
///
/// Two routes reach it and they differ in what they may do first. Dismissing
/// puts back whatever was in front; losing focus must not, because focus has
/// already gone somewhere the user picked. What the two have in common is
/// here, so a third route cannot quietly do half of it.
pub fn went_away(window: &WebviewWindow) {
    // A question the launcher asked belongs to whoever was looking at the row.
    // The window has gone, so nobody is answering it: leaving it open would
    // mean coming back and pressing Enter once was enough to restart a machine.
    if let Some(asked) = window.app_handle().try_state::<crate::system::Asked>() {
        asked.forget();
    }

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

    use super::{blocked_by, Blocked};
    use std::cell::Cell;

    /// A summon that got the keyboard says nothing, whatever else is true.
    ///
    /// A full-screen video plays while somebody uses the launcher over it every
    /// day of the week. Reporting that as a problem, on the summon that plainly
    /// worked, is how a status surface becomes something people stop reading.
    #[test]
    fn a_summon_that_worked_is_never_reported() {
        assert_eq!(blocked_by(true, || true, || true), None);
    }

    /// The summon that worked asks Windows nothing.
    ///
    /// Both readings are calls into the operating system, and this runs between
    /// a key being pressed and the launcher being on screen, which is the path
    /// this whole application is judged on. Every summon takes it, and the one
    /// that has something to report is the rare one.
    #[test]
    fn a_summon_that_worked_costs_no_calls_into_windows() {
        let asked = Cell::new(0);
        let count = || {
            asked.set(asked.get() + 1);
            true
        };

        blocked_by(true, count, count);

        assert_eq!(asked.get(), 0, "the summon path asked Windows something");
    }

    /// The reason the user can act on is the one they are told.
    ///
    /// A game running as administrator is both, and "start Sill as
    /// administrator" is a thing somebody can go and do. "Something is full
    /// screen" is not. Nothing is asked about the display once the answer is
    /// settled, which is the same rule as above rather than a second one.
    #[test]
    fn an_elevated_window_is_named_ahead_of_a_full_screen_one() {
        let looked_at_the_display = Cell::new(false);

        let blocked = blocked_by(
            false,
            || true,
            || {
                looked_at_the_display.set(true);
                true
            },
        );

        assert_eq!(blocked, Some(Blocked::Elevated));
        assert!(
            !looked_at_the_display.get(),
            "the display was asked about after the answer was already known"
        );

        assert_eq!(
            blocked_by(false, || true, || false),
            Some(Blocked::Elevated)
        );
    }

    #[test]
    fn a_full_screen_program_is_named_when_nothing_is_elevated() {
        assert_eq!(
            blocked_by(false, || false, || true),
            Some(Blocked::FullScreen)
        );
    }

    /// A failure with no cause Sill can name is not reported at all.
    ///
    /// `SetForegroundWindow` is refused for reasons that are none of the
    /// user's business and several that are transient. A trouble saying only
    /// that something went wrong is one nobody can act on, and the surface is
    /// worth having exactly because it is empty almost always.
    #[test]
    fn a_failure_with_no_nameable_cause_stays_out_of_the_surface() {
        assert_eq!(blocked_by(false, || false, || false), None);
    }
}
