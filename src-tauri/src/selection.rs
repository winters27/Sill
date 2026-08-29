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
//! - The clipboard history must not record any of it, or every transformation
//!   would leave entries behind.
//! - The original contents go back afterwards, so the thing the user copied
//!   ten minutes ago is still there.
//! - The keystroke is marked as ours, so the dictation hook does not read
//!   Sill's own Ctrl+C as a trigger.
//!
//! **The borrow is held for the whole operation, by one owner.** It used to be
//! two nested save-and-restore pairs, one in the capture and one in the
//! replace, each reserving the changes it expected to make. That is wrong for
//! a reason no amount of reading the code showed: the action running in
//! between writes its own result to the clipboard, so by the time the replace
//! read "the previous contents" it was reading the action's output and
//! faithfully restored that. Measured on a real desktop: a 5,011 character
//! clipboard came back as 14 characters. See [`Held`].
//!
//! Sill must not be the foreground window when any of this runs. Everything
//! here is reached from a global shortcut with the launcher hidden, which is
//! the only arrangement where "the selection" means anything.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::clipboard::monitor::{Clipboard, CLIPBOARD_ATTEMPTS, RETRY_DELAY};

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

/// The user's clipboard, borrowed for the length of one operation.
///
/// Taken before anything runs and given back after everything has, so the
/// value that goes back is the one the user actually had rather than whatever
/// the last step happened to leave behind. Nothing in between is recorded in
/// the history, however many writes it takes, because the history is suspended
/// rather than told to expect a number.
///
/// A bound transform's result therefore does not appear in clipboard history.
/// That is deliberate: pressing a transform key twenty times should not leave
/// twenty entries nobody asked for.
pub struct Held {
    app: AppHandle,
    /// What was on the clipboard before Sill touched it.
    previous: Option<String>,
}

impl Held {
    /// Takes the clipboard, stopping the history until it is given back.
    pub fn take(app: &AppHandle) -> Self {
        // Before the read, because the listener fires on its own thread the
        // moment the contents change and a flag set afterwards is set too late.
        if let Some(history) = app.try_state::<Clipboard>() {
            history.suspend();
        }

        Self {
            app: app.clone(),
            previous: app.clipboard().read_text().ok(),
        }
    }

    /// Puts the original contents back and starts recording again.
    pub fn give_back(self) {
        put(&self.app, self.previous.as_deref());
        self.finish();
    }

    /// Starts recording again, leaving on the clipboard whatever is there.
    ///
    /// For an operation whose whole point was to put its result there.
    pub fn keep_result(self) {
        self.finish();
    }

    fn finish(self) {
        // The listener reports a change slightly after it happens, so resuming
        // the instant the last write returns lets that write be recorded as
        // though the user had copied it.
        std::thread::sleep(SETTLE);

        if let Some(history) = self.app.try_state::<Clipboard>() {
            history.resume();
        }
    }
}

/// Writes to the clipboard, retrying the lock the way every read here does.
///
/// A single attempt is what a borrowed clipboard cannot afford: the
/// application that was just pasted into is frequently still holding the
/// clipboard open. Returns whether the write landed.
fn put(app: &AppHandle, text: Option<&str>) -> bool {
    for attempt in 0..CLIPBOARD_ATTEMPTS {
        let result = match text {
            Some(text) => app.clipboard().write_text(text.to_string()),
            // There was no text before, so leaving ours behind would be adding
            // something rather than restoring anything.
            None => app.clipboard().clear(),
        };

        match result {
            Ok(()) => return true,
            Err(err) if attempt + 1 == CLIPBOARD_ATTEMPTS => {
                crate::say!(
                    "could not write to the clipboard after {CLIPBOARD_ATTEMPTS} tries: {err}"
                );
                return false;
            }
            Err(_) => std::thread::sleep(RETRY_DELAY),
        }
    }

    false
}

/// Whatever is selected in the foreground application.
///
/// `None` when nothing is selected, when the application does not answer
/// Ctrl+C, or when it answers with something that is not text.
///
/// Leaves its copy on the clipboard. The caller holds the borrow and is what
/// puts things back; see [`Held`].
pub fn capture(app: &AppHandle) -> Option<String> {
    let before = sequence();

    if !crate::input::ctrl(crate::input::VK_C) {
        return None;
    }

    let deadline = Instant::now() + ANSWER_TIMEOUT;
    while sequence() == before {
        if Instant::now() >= deadline {
            // Nothing was selected, or the application ignores Ctrl+C. Either
            // way there is nothing to act on, and the clipboard is untouched.
            return None;
        }
        std::thread::sleep(POLL);
    }

    app.clipboard().read_text().ok().filter(|s| !s.is_empty())
}

/// Replaces the selection with `text`.
///
/// Leaves the result on the clipboard. The caller holds the borrow and is what
/// puts the original back; see [`Held`].
pub fn replace(app: &AppHandle, text: &str) -> Result<(), String> {
    if !put(app, Some(text)) {
        return Err("could not put the result on the clipboard".to_string());
    }

    std::thread::sleep(SETTLE);

    if !crate::input::ctrl(crate::input::VK_V) {
        return Err("that application would not accept the paste".to_string());
    }

    std::thread::sleep(SETTLE);
    Ok(())
}

/// Text a keyboard-driven action is about to run against.
///
/// Captured when a shortcut fires rather than held continuously, because the
/// only cheap way to read a selection is to press Ctrl+C in somebody else's
/// window, and doing that on a timer would be indefensible.
pub struct Captured {
    pub text: String,
    /// Where the text came from, which is not the same as where it was asked
    /// for. This is the only thing that distinguishes "the selection said
    /// HELLO" from "there was no selection so here is the clipboard", and the
    /// caller has to know which, because only one of them can be pasted back.
    pub from: Origin,
}

/// Where captured text actually came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Selection,
    Clipboard,
}

impl Captured {
    /// The selection, or the clipboard when there is no selection.
    ///
    /// The fallback is what makes a shortcut usable rather than fussy: a
    /// transform bound to a key should do something sensible when you press it
    /// with nothing highlighted, and the thing you last copied is the obvious
    /// candidate.
    ///
    /// **The fallback is only safe because [`Origin`] comes back with it.**
    /// Falling back and then pasting the result over the selection would take
    /// text the user never chose and write it into their document, destroying
    /// what was highlighted. That is not hypothetical: it is what happened the
    /// first time this ran against a real editor, before the capture worked.
    ///
    /// The fallback reads `held` rather than the clipboard, and has to: by
    /// this point Sill's own copy is sitting on the clipboard, so reading it
    /// again would return the selection rather than what the user last copied.
    pub fn selection_or_clipboard(app: &AppHandle, held: &Held) -> Option<Self> {
        if let Some(text) = capture(app).filter(|text| !text.is_empty()) {
            return Some(Self {
                text,
                from: Origin::Selection,
            });
        }

        Self::clipboard(held)
    }

    /// What is on the clipboard, whatever is selected.
    pub fn clipboard(held: &Held) -> Option<Self> {
        let text = held.previous.clone().filter(|text| !text.is_empty())?;

        Some(Self {
            text,
            from: Origin::Clipboard,
        })
    }
}
