//! Sending keystrokes to whatever has focus.
//!
//! Extracted from `dictation::paste`, which had the only copy of it and is no
//! longer the only caller: replacing a selection needs Ctrl+C as well as
//! Ctrl+V, and putting a copy chord in a module called `paste` would be a
//! confusing place for the next person to look.
//!
//! Everything here is marked with [`crate::synthetic::SILL_SYNTHETIC`], which
//! is what lets Sill's own keyboard hook tell our keystrokes from the user's.
//! Filtering on `LLKHF_INJECTED` instead would also ignore every remapper,
//! macro key, on-screen keyboard and remote session, which is most of the
//! reason that flag is the wrong tool.

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};

#[cfg(windows)]
pub use windows::Win32::UI::Input::KeyboardAndMouse::{VK_C, VK_V};

/// Holds Ctrl, taps `key`, releases Ctrl.
///
/// All four events go in one `SendInput` call. Sending them separately lets
/// another thread's input interleave between them, which arrives at the target
/// as a lone letter: a stray "v" in the middle of somebody's document.
/// Modifiers that change what a chord means if they are still held.
#[cfg(windows)]
const MODIFIERS: [VIRTUAL_KEY; 5] = [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN];

/// Lets go of any modifier the user has not released yet.
///
/// **Found by testing on a real desktop, and it is not an edge case.** These
/// chords are sent from a global shortcut, which fires the instant the key
/// goes down while the user is still holding Ctrl and Alt. Sending Ctrl+C on
/// top of that arrives at the target as Ctrl+Alt+C, which is not copy, so the
/// selection is never read and the whole feature silently does nothing.
///
/// Only keys actually down are released. Sending a key-up for a modifier
/// nobody is holding is not free: it can cancel a chord in the middle of being
/// typed in some applications.
#[cfg(windows)]
fn release_held_modifiers() {
    let held: Vec<INPUT> = MODIFIERS
        .iter()
        // SAFETY: takes a virtual key, returns a bitfield, dereferences
        // nothing. The high bit means the key is down right now.
        .filter(|&&vk| unsafe { GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000 != 0 })
        .map(|&vk| stroke(vk, true))
        .collect();

    if held.is_empty() {
        return;
    }

    // SAFETY: a live slice of correctly sized INPUT records, size from the
    // type rather than assumed.
    unsafe { SendInput(&held, std::mem::size_of::<INPUT>() as i32) };
}

/// Taps `key` with every modifier a hyper key stands for.
///
/// One `SendInput` for all ten events, so nothing can interleave and nothing
/// is left held. **Every release is in the same batch as its press**, which is
/// the property that makes a hyper key safe: there is no window in which the
/// process could end and leave four modifiers down, because holding them is
/// never a state this is in.
///
/// Sent straight from the hook thread rather than handed to another one. A
/// thread per chord would be free to run after the next keystroke had already
/// gone through, so Hyper+T followed by a letter could arrive as the letter
/// followed by Hyper+T. Ten events is microseconds, and order is the whole
/// point of the feature.
///
/// No `release_held_modifiers` here, unlike `ctrl`. That exists because a
/// global shortcut fires while its own keys are still down; a hyper chord is
/// sent because a key went down, and whatever else is held is what the person
/// is deliberately holding.
#[cfg(windows)]
pub fn hyper(key: u16) -> bool {
    let vk = VIRTUAL_KEY(key);

    let events = [
        stroke(VK_CONTROL, false),
        stroke(VK_MENU, false),
        stroke(VK_SHIFT, false),
        stroke(VK_LWIN, false),
        stroke(vk, false),
        stroke(vk, true),
        stroke(VK_LWIN, true),
        stroke(VK_SHIFT, true),
        stroke(VK_MENU, true),
        stroke(VK_CONTROL, true),
    ];

    // SAFETY: a live array of correctly sized INPUT records, size taken from
    // the type rather than assumed.
    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };

    sent as usize == events.len()
}

#[cfg(not(windows))]
pub fn hyper(_key: u16) -> bool {
    false
}

#[cfg(windows)]
pub fn ctrl(key: VIRTUAL_KEY) -> bool {
    // The shortcut that got us here is probably still held down.
    release_held_modifiers();

    let events = [
        stroke(VK_CONTROL, false),
        stroke(key, false),
        stroke(key, true),
        stroke(VK_CONTROL, true),
    ];

    // SAFETY: `events` is a live array of correctly sized INPUT records and
    // the size argument is taken from the type rather than assumed.
    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };

    if sent as usize != events.len() {
        // The usual cause is a target running elevated while Sill is not:
        // Windows refuses synthetic input across that boundary and gives no
        // other signal.
        crate::say!(
            "a keystroke was blocked, {sent} of {} events delivered. \
             The target is probably running elevated",
            events.len()
        );
        return false;
    }

    true
}

#[cfg(windows)]
fn stroke(vk: VIRTUAL_KEY, up: bool) -> INPUT {
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
pub fn ctrl(_key: u16) -> bool {
    false
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_chord_presses_and_releases_both_keys_in_order() {
        // Modifier down, key down, key up, modifier up. Releasing the
        // modifier first leaves the target seeing a bare letter.
        let events = [
            stroke(VK_CONTROL, false),
            stroke(VK_V, false),
            stroke(VK_V, true),
            stroke(VK_CONTROL, true),
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
        // A record whose type does not match the union member that was filled
        // in is read as mouse input, which moves the pointer.
        for event in [stroke(VK_V, false), stroke(VK_CONTROL, true)] {
            assert_eq!(event.r#type, INPUT_KEYBOARD);
        }
    }

    #[test]
    fn our_own_keystrokes_carry_the_mark() {
        // Without this the dictation hook reads Sill's paste as user input and
        // the trigger fires on its own keystroke.
        for event in [stroke(VK_C, false), stroke(VK_V, true)] {
            // SAFETY: built as INPUT_KEYBOARD directly above.
            let extra = unsafe { event.Anonymous.ki.dwExtraInfo };
            assert_eq!(extra, crate::synthetic::SILL_SYNTHETIC);
        }
    }
}
