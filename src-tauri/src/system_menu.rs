//! Alt+Space in a window that records keys.
//!
//! Windows reads Alt+Space in any window as "open the system menu" (Restore,
//! Move, Size, Minimize, Close). The webview's child window passes the key to
//! `DefWindowProc`, which sends the top-level window `WM_SYSCOMMAND` with
//! `SC_KEYMENU` and a space, and the menu that opens takes focus. The settings
//! window's key recorder stops recording when it loses focus, so Alt+Space,
//! Sill's own default summon key, could not be recorded at all.
//!
//! The settings window is frameless and draws its own title bar, so the system
//! menu is not something anybody there reaches for. This declines that one
//! request and hands every other message on, including `SC_KEYMENU` from the
//! bare Alt key and from Alt with any other letter.

/// Stops Alt+Space from opening the system menu of `window`.
///
/// Callable from any thread: the subclass is installed on the window's own
/// thread, which is what `SetWindowSubclass` requires.
#[cfg(windows)]
pub fn decline_alt_space(window: &tauri::WebviewWindow) {
    let Ok(handle) = window.hwnd() else {
        crate::say!("{}: no window to keep Alt+Space on", window.label());
        return;
    };

    // Tauri hands back an HWND from its own pinned `windows` version, which is
    // a different type to ours with an identical value. The raw pointer, as a
    // number so the closure is `Send`, is the common ground.
    let raw = handle.0 as isize;
    let label = window.label().to_string();

    let _ = window.run_on_main_thread(move || {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::Shell::SetWindowSubclass;

        let hwnd = HWND(raw as *mut core::ffi::c_void);

        // SAFETY: the handle belongs to a window of this process, this runs on
        // the thread that owns it, and the procedure defers everything it does
        // not recognise. Installing twice under the same id replaces rather
        // than stacks, so reopening the window is safe.
        let installed = unsafe { SetWindowSubclass(hwnd, Some(procedure), ID, 0) };
        if !installed.as_bool() {
            crate::say!("{label}: could not keep Alt+Space from the system menu");
        }
    });
}

#[cfg(not(windows))]
pub fn decline_alt_space(_window: &tauri::WebviewWindow) {}

/// Tells this subclass apart from anybody else's on the same window.
#[cfg(windows)]
const ID: usize = 0x5112;

/// Whether a `WM_SYSCOMMAND` is the system menu being opened by Alt+Space.
///
/// The low four bits of the command are Windows' own, so they are masked off
/// before comparing, as the documentation for `WM_SYSCOMMAND` requires. For
/// `SC_KEYMENU` the `lParam` is the character pressed with Alt: a space for
/// Alt+Space, zero for Alt on its own.
#[cfg(windows)]
fn is_alt_space(command: usize, character: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::SC_KEYMENU;

    (command & 0xFFF0) == SC_KEYMENU as usize && character == b' ' as isize
}

/// Runs before the window's own procedure, for every message it gets.
///
/// One comparison for anything that is not `WM_SYSCOMMAND`, because this sits
/// in front of every paint and every keystroke the window receives.
#[cfg(windows)]
unsafe extern "system" fn procedure(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    _id: usize,
    _data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::Foundation::LRESULT;
    use windows::Win32::UI::Shell::DefSubclassProc;
    use windows::Win32::UI::WindowsAndMessaging::WM_SYSCOMMAND;

    if message == WM_SYSCOMMAND && is_alt_space(wparam.0, lparam.0) {
        // Handled, which for WM_SYSCOMMAND means returning zero.
        return LRESULT(0);
    }

    // SAFETY: the arguments are the ones Windows handed in, unchanged.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{SC_CLOSE, SC_KEYMENU};

    #[test]
    fn alt_space_is_declined() {
        assert!(is_alt_space(SC_KEYMENU as usize, b' ' as isize));
    }

    /// Windows sets the low four bits for its own use, and the documentation
    /// says to mask them. An unmasked comparison would let this through.
    #[test]
    fn the_low_bits_windows_keeps_do_not_hide_it() {
        assert!(is_alt_space(SC_KEYMENU as usize | 0x0003, b' ' as isize));
    }

    /// Alt on its own still reaches the window, and so does Alt with a letter.
    #[test]
    fn every_other_key_menu_is_left_alone() {
        assert!(!is_alt_space(SC_KEYMENU as usize, 0));
        assert!(!is_alt_space(SC_KEYMENU as usize, b'f' as isize));
    }

    #[test]
    fn another_system_command_with_a_space_is_left_alone() {
        assert!(!is_alt_space(SC_CLOSE as usize, b' ' as isize));
    }
}
