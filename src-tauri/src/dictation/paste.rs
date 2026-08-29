//! Synthesising the paste chord.
//!
//! Dictation ends by putting the transcript on the clipboard and pressing
//! Ctrl+V for the user, in whatever application they were already typing in.
//! Nothing here moves focus: the panel window is declared `focus: false` and
//! `skipTaskbar`, so the target application is still frontmost by the time
//! this runs.

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_V,
};

/// Presses Ctrl+V.
///
/// Both keys go down and both come up in one `SendInput` call. Sending them
/// as separate calls lets another thread's input interleave between them,
/// which arrives at the target as a lone V.
#[cfg(windows)]
pub fn chord() {
    let events = [
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];

    // SAFETY: `events` is a live array of correctly sized INPUT records and
    // the size argument is taken from the type, not assumed.
    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };

    if sent as usize != events.len() {
        // The usual cause is a target running elevated while Sill is not:
        // Windows refuses synthetic input across that boundary and gives no
        // other signal. The transcript is still on the clipboard.
        eprintln!(
            "[sill] the paste chord was blocked, {sent} of {} events delivered. \
             The transcript is on the clipboard",
            events.len()
        );
    }
}

#[cfg(windows)]
fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: crate::synthetic::SILL_SYNTHETIC,
            },
        },
    }
}

#[cfg(not(windows))]
pub fn chord() {}

/// How long to wait between writing the clipboard and pressing Ctrl+V.
///
/// Writing and immediately pasting races the target application's read of the
/// clipboard, and the symptom is the *previous* contents arriving instead of
/// what was just put there. Long enough to lose that race reliably, short
/// enough that nobody notices it happening.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(60);

/// Puts the launcher away and pastes into whatever was in front of it.
///
/// Call this once the clipboard already holds what should land. It is the
/// second half of every paste in Sill: a snippet expanding from the root list,
/// an extension calling `Clipboard.paste`, and `Action.Paste`. All three used
/// to spell it out for themselves, and one of them got it wrong by not doing
/// it at all.
///
/// The launcher has to go first. Sill is frontmost while any of those run, so
/// pasting without stepping aside delivers the text into the search field.
pub fn deliver(app: &tauri::AppHandle) {
    use tauri::Manager;

    if let Some(window) = app.get_webview_window("main") {
        crate::summon::hide(&window);
    }

    std::thread::sleep(SETTLE);
    chord();
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn the_chord_presses_and_releases_both_keys_in_order() {
        // Modifier down, key down, key up, modifier up. Releasing the
        // modifier first leaves the target seeing a bare V.
        let events = [
            key(VK_CONTROL, false),
            key(VK_V, false),
            key(VK_V, true),
            key(VK_CONTROL, true),
        ];

        let flags: Vec<_> = events
            .iter()
            // SAFETY: every record was built as INPUT_KEYBOARD above.
            .map(|input| unsafe { (input.Anonymous.ki.wVk, input.Anonymous.ki.dwFlags) })
            .collect();

        assert_eq!(
            flags,
            vec![
                (VK_CONTROL, Default::default()),
                (VK_V, Default::default()),
                (VK_V, KEYEVENTF_KEYUP),
                (VK_CONTROL, KEYEVENTF_KEYUP),
            ]
        );
    }

    #[test]
    fn every_event_is_a_keyboard_event() {
        // A record whose type does not match the union member that was
        // filled in is read as mouse input, which moves the pointer.
        for event in [key(VK_V, false), key(VK_CONTROL, true)] {
            assert_eq!(event.r#type, INPUT_KEYBOARD);
        }
    }
}
