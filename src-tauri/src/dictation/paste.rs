//! Synthesising the paste chord.
//!
//! Dictation ends by putting the transcript on the clipboard and pressing
//! Ctrl+V for the user, in whatever application they were already typing in.
//! That application is recorded when dictation starts (`Target`) and put back
//! in front before the chord, rather than trusted to still be there.

/// Presses Ctrl+V in whatever has focus.
///
/// The machinery moved to `crate::input` once replacing a selection needed
/// Ctrl+C as well; this is the name dictation has always called it by.
///
/// Whether Windows took the keystrokes. It refuses synthetic input to an
/// elevated window when Sill is not elevated, and says nothing else.
pub fn chord() -> bool {
    #[cfg(windows)]
    return crate::input::ctrl(crate::input::VK_V);

    #[cfg(not(windows))]
    false
}

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

/// The window a dictation pastes into: whatever was in front when it began.
///
/// The paste used to go to whatever was in front when the transcript came
/// back, on the word of the module header that nothing moves focus. Something
/// can: the panel is shown with a plain `show()` on every dictation but the
/// first (tao clears its don't-focus marker after one use), and a long
/// transcription is plenty of time for the person to click somewhere. When
/// the paste landed elsewhere the transcript was still on the clipboard, which
/// is why a Ctrl+V by hand always worked.
///
/// Empty when the front window was one of Sill's own (the Dictate row in the
/// launcher, which is hiding as dictation starts), or there was none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Target(isize);

impl Target {
    #[cfg(windows)]
    pub fn now() -> Self {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

        // SAFETY: no arguments, and a null answer is handled below.
        let front = unsafe { GetForegroundWindow() };
        if front.is_invalid() {
            return Self::default();
        }

        let mut owner = 0u32;
        // SAFETY: `front` was just returned by Windows and `owner` is a live u32.
        unsafe { GetWindowThreadProcessId(front, Some(&mut owner)) };
        if owner == std::process::id() {
            return Self::default();
        }

        Self(front.0 as isize)
    }

    #[cfg(not(windows))]
    pub fn now() -> Self {
        Self::default()
    }

    /// Puts the target back in front if something else is. Says what it found
    /// and did, for the log.
    #[cfg(windows)]
    pub fn bring_back(self) -> &'static str {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow};

        if self.0 == 0 {
            return "no target recorded";
        }

        let target = HWND(self.0 as *mut core::ffi::c_void);
        // SAFETY: both only read; a stale handle answers false.
        let (front, alive) = unsafe { (GetForegroundWindow(), IsWindow(Some(target)).as_bool()) };
        if front == target {
            return "target still in front";
        }
        if !alive {
            return "target window has closed";
        }

        if crate::summon::force_foreground(target) {
            std::thread::sleep(SETTLE);
            "target was not in front, brought it back"
        } else {
            "target was not in front, and Windows refused to bring it back"
        }
    }

    #[cfg(not(windows))]
    pub fn bring_back(self) -> &'static str {
        "no target recorded"
    }
}
