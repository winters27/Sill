//! Reading and replacing whatever text is selected in another application.
//!
//! Windows offers no way to ask "what is selected?" that works everywhere. UI
//! Automation answers for applications that implement `TextPattern` and stays
//! silent for the rest, which in practice means it fails in a browser text
//! box, in a terminal, and in half of Electron. So this does what every tool
//! in this space does: it presses Ctrl+C for you, reads what landed, and puts
//! the clipboard back the way it was.
//!
//! That is a borrowed clipboard, not a clobbered one, and the borrowing has to
//! be invisible:
//!
//! - The clipboard history must not record either change, or every
//!   transformation would leave two entries behind.
//! - The original contents go back afterwards, so the thing the user copied
//!   ten minutes ago is still there.
//! - The keystroke is marked as ours, so the dictation hook does not read
//!   Sill's own Ctrl+C as a trigger.
//!
//! Sill must not be the foreground window when this runs. Everything here is
//! reached from a global shortcut with the launcher hidden, which is the only
//! arrangement where "the selection" means anything.

use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

/// How long to wait for the target application to answer Ctrl+C.
///
/// Generous, because the wait ends the moment the clipboard actually changes
/// and only runs out when there was no selection to copy. A tighter bound
/// makes an ordinary slow application look like an empty selection.
const ANSWER_TIMEOUT: Duration = Duration::from_millis(400);

/// How often the clipboard is asked whether it has changed yet.
const POLL: Duration = Duration::from_millis(10);

/// Long enough for the target to have read the clipboard before it is put back.
///
/// The same race every paste in Sill has to lose deliberately: writing and
/// immediately restoring means the application pastes the restored value.
const SETTLE: Duration = Duration::from_millis(80);

/// A count of clipboard changes, which Windows increments for every one.
///
/// Comparing contents instead cannot tell "the copy produced the same text
/// that was already there" from "nothing was copied", and the first is
/// perfectly ordinary: selecting a word you just copied and running a
/// transform on it.
#[cfg(windows)]
fn sequence() -> u32 {
    // SAFETY: takes nothing, returns a counter, dereferences nothing.
    unsafe { windows::Win32::System::DataExchange::GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
fn sequence() -> u32 {
    0
}

/// Whatever is selected in the foreground application.
///
/// `None` when nothing is selected, when the application does not answer
/// Ctrl+C, or when it answers with something that is not text.
pub fn capture(app: &AppHandle) -> Option<String> {
    let clipboard = app.try_state::<crate::clipboard::monitor::Clipboard>();

    // Both changes are ours: the copy, and putting the original back. Told
    // before either happens, because the listener fires on its own thread the
    // moment the contents change and a flag set afterwards is set too late.
    if let Some(history) = &clipboard {
        history.ignore_next_changes(2);
    }

    let before = sequence();
    let previous = app.clipboard().read_text().ok();

    if !crate::input::ctrl(crate::input::VK_C) {
        restore(app, previous);
        return None;
    }

    let deadline = Instant::now() + ANSWER_TIMEOUT;
    while sequence() == before {
        if Instant::now() >= deadline {
            // Nothing was selected, or the application ignores Ctrl+C. Either
            // way there is nothing to act on, and the clipboard is untouched.
            if let Some(history) = &clipboard {
                history.forget_ignored();
            }
            return None;
        }
        std::thread::sleep(POLL);
    }

    let selected = app.clipboard().read_text().ok().filter(|s| !s.is_empty());
    restore(app, previous);
    selected
}

/// Replaces the selection with `text`.
///
/// Leaves the clipboard as it found it. A transform is meant to feel like the
/// text changed in place, and quietly keeping the result on the clipboard is a
/// side effect nobody asked for.
pub fn replace(app: &AppHandle, text: &str) -> Result<(), String> {
    let clipboard = app.try_state::<crate::clipboard::monitor::Clipboard>();

    // The paste, and putting the original back afterwards.
    if let Some(history) = &clipboard {
        history.ignore_next_changes(2);
    }

    let previous = app.clipboard().read_text().ok();

    app.clipboard()
        .write_text(text.to_string())
        .map_err(|err| format!("could not put the result on the clipboard: {err}"))?;

    std::thread::sleep(SETTLE);

    if !crate::input::ctrl(crate::input::VK_V) {
        restore(app, previous);
        return Err("that application would not accept the paste".to_string());
    }

    std::thread::sleep(SETTLE);
    restore(app, previous);
    Ok(())
}

/// Puts back what was on the clipboard, or clears it if there was nothing.
fn restore(app: &AppHandle, previous: Option<String>) {
    match previous {
        Some(text) => {
            let _ = app.clipboard().write_text(text);
        }
        // There was no text before, so leaving ours behind would be adding
        // something rather than restoring anything.
        None => {
            let _ = app.clipboard().clear();
        }
    }
}

/// Everything a keyboard-driven action needs about the world.
///
/// Captured when a shortcut fires rather than held continuously, because the
/// only cheap way to read a selection is to press Ctrl+C in somebody else's
/// window, and doing that on a timer would be indefensible.
pub struct Captured {
    pub text: String,
}

impl Captured {
    /// The selection, or the clipboard when there is no selection.
    ///
    /// The fallback is what makes a shortcut usable rather than fussy: a
    /// transform bound to a key should do something sensible when you press it
    /// with nothing highlighted, and the thing you last copied is the obvious
    /// candidate.
    pub fn selection_or_clipboard(app: &AppHandle) -> Option<Self> {
        let text = capture(app)
            .or_else(|| app.clipboard().read_text().ok())
            .filter(|text| !text.is_empty())?;

        Some(Self { text })
    }

    pub fn clipboard(app: &AppHandle) -> Option<Self> {
        let text = app
            .clipboard()
            .read_text()
            .ok()
            .filter(|text| !text.is_empty())?;

        Some(Self { text })
    }
}

use tauri::Manager;
