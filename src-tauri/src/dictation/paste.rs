//! Synthesising the paste chord.
//!
//! Dictation ends by putting the transcript on the clipboard and pressing
//! Ctrl+V for the user, in whatever application they were already typing in.
//! Nothing here moves focus: the panel window is declared `focus: false` and
//! `skipTaskbar`, so the target application is still frontmost by the time
//! this runs.

/// Presses Ctrl+V in whatever has focus.
///
/// The machinery moved to `crate::input` once replacing a selection needed
/// Ctrl+C as well; this is the name dictation has always called it by.
pub fn chord() {
    #[cfg(windows)]
    crate::input::ctrl(crate::input::VK_V);
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
