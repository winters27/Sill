//! The floating dictation panel.
//!
//! Its own pre-declared window (label `"dictation"`), not a mode of the
//! launcher: the launcher takes focus when it appears, and dictation has to
//! leave focus exactly where it found it or the paste lands in the wrong
//! application.
//!
//! It carries three things the interaction has nowhere else to put: that a
//! dictation is live, what the microphone is actually hearing, and which key
//! ends it. The middle one matters because a microphone blocked by Windows
//! privacy settings fails by returning digital silence rather than an error,
//! and a flat waveform is the only sign.

use crate::dictation::error::{DictationError, Result};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager};

/// Window label. Built on demand by `lazy_windows`, not declared.
pub const PANEL_WINDOW_LABEL: &str = "dictation";

/// Its size, repeated here because `outer_size()` reports 0x0 for a window
/// that has never been shown, and this one is built hidden. The HUD hardcodes
/// its own dimensions for exactly this reason.
const PANEL_WIDTH: f64 = 240.0;
/// Mirrors the `dictation` window in lazy_windows.rs, which explains the
/// number. Change both or the panel resizes itself on first show.
const PANEL_HEIGHT: f64 = 96.0;

/// Gap from the bottom of the monitor. Clear of the taskbar, and low enough
/// that it never covers what is being dictated into.
const PANEL_BOTTOM_MARGIN: f64 = 80.0;

/// What the panel is currently doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelStatus {
    Listening,
    Transcribing,
    /// The transcript went to the clipboard rather than being pasted.
    Copied,
    /// Cancel was pressed once and is waiting to be confirmed.
    Confirming,
}

/// Latest panel status, so the route can recover it on mount.
///
/// A window declared `visible: false` may not have its webview running when
/// the first `show` lands, and an event emitted to a window that is not
/// listening yet is simply gone. `routes/hud` hit exactly this and solved it
/// with a state getter; the same belt is needed here or the very first
/// dictation renders an empty, transparent pill.
#[derive(Default)]
pub struct PanelState(pub Mutex<Option<PanelStatus>>);

/// Shows the panel, or updates it if it is already up.
pub fn show(app: &AppHandle, status: PanelStatus) -> Result<()> {
    // Built on the first dictation rather than declared. A window costs a
    // renderer whether or not it is ever shown, and most sessions never
    // dictate.
    let window = crate::lazy_windows::ensure(app, PANEL_WINDOW_LABEL).map_err(|err| {
        crate::say!("{err}");
        DictationError::NotFound("dictation panel window".to_string())
    })?;

    position_at_bottom_center(&window)?;

    // Recorded before anything is emitted so a route mounting late can ask
    // for the current status instead of waiting for the next event.
    if let Some(state) = app.try_state::<PanelState>() {
        if let Ok(mut current) = state.0.lock() {
            *current = Some(status);
        }
    }

    crate::sleep::wake(&window);
    window
        .show()
        .map_err(|e| DictationError::Platform(format!("show dictation panel: {e}")))?;
    keep_on_top(&window);

    // Emitted AFTER showing: a hidden window's webview may not be running,
    // and an event delivered to a window that is not listening yet is lost
    // with no error anywhere.
    app.emit("dictation:status", status)
        .map_err(|e| DictationError::Platform(format!("emit dictation:status: {e}")))?;
    report_shown(&window, status);

    Ok(())
}

/// Says what the show achieved, rather than that it was attempted.
///
/// `show()` returning `Ok` means the message was accepted and nothing more.
/// This panel is transparent, undecorated and never focused, so three separate
/// things have to be true before any of it is on screen: the window up, the
/// renderer painting, and the window above what it covers. A panel that is
/// missing for any of those reasons looks the same as one that is missing for
/// the others, and this line used to read `panel shown` for all of them.
///
/// The renderer's own visibility is not here. It lives behind `with_webview`,
/// which hands its closure to another thread and returns nothing, so
/// `sleep::wake` reports that half itself on the line above this one.
fn report_shown(window: &tauri::WebviewWindow, status: PanelStatus) {
    let up = window
        .is_visible()
        .map_or_else(|e| format!("unreadable ({e})"), |on| on.to_string());

    let on_top = topmost(window).map_or_else(|| "unreadable".to_string(), |on| on.to_string());

    // Read back rather than recomputed. `position_at_bottom_center` says where
    // the panel was asked to go; this says where it is, which is the number
    // that settles whether it landed on a screen somebody is looking at.
    let at = match (window.outer_position(), window.outer_size()) {
        (Ok(at), Ok(size)) => format!("{},{} {}x{}", at.x, at.y, size.width, size.height),
        _ => "unreadable".to_string(),
    };

    crate::say!("panel shown ({status:?}) window={up} topmost={on_top} at {at}");
}

/// Whether the window carries `WS_EX_TOPMOST` at this moment.
///
/// `always_on_top(true)` is set once when the window is built and nothing in
/// the tree ever asserts it again, so this is read rather than assumed.
/// `windowing::is_on_top` asks the same question of another application's
/// window and has to look the handle up by id first; this one is handed a
/// window Sill owns.
#[cfg(windows)]
fn topmost(window: &tauri::WebviewWindow) -> Option<bool> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOPMOST};

    let handle = window.hwnd().ok()?;

    // SAFETY: the handle comes from the window this is called on, which is
    // alive for the length of the call.
    let style =
        unsafe { GetWindowLongPtrW(HWND(handle.0 as *mut core::ffi::c_void), GWL_EXSTYLE) } as u32;
    Some(style & WS_EX_TOPMOST.0 != 0)
}

#[cfg(not(windows))]
fn topmost(_window: &tauri::WebviewWindow) -> Option<bool> {
    None
}

/// Puts the panel back on top of the stack without activating it.
///
/// This is the one Sill window with no other way to the front. Every other
/// show calls `set_focus`, and three of them call `summon::force_foreground`,
/// either of which reorders the stack as a side effect. This one deliberately
/// does neither, so nothing has re-asserted its position since the window was
/// built.
///
/// `SWP_NOACTIVATE` is the whole reason this is safe here: without it the call
/// takes the foreground, and the transcript then pastes into the panel rather
/// than into whatever was being dictated into.
#[cfg(windows)]
fn keep_on_top(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    let Ok(handle) = window.hwnd() else {
        return;
    };

    // Position and size are left alone, so the zero rectangle is never read.
    //
    // SAFETY: the handle comes from the window this is called on, and the
    // flags are valid for this call.
    let placed = unsafe {
        SetWindowPos(
            HWND(handle.0 as *mut core::ffi::c_void),
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
        )
    };

    if let Err(err) = placed {
        crate::say!("dictation panel would not go on top: {err}");
    }
}

#[cfg(not(windows))]
fn keep_on_top(_window: &tauri::WebviewWindow) {}

/// Pushes one frame of band energies, each 0.0 to 1.0.
///
/// Fire and forget: a dropped frame of a waveform is not worth interrupting
/// a recording over, and the next one is 33 ms away.
pub fn emit_bands(app: &AppHandle, bands: &[f32]) {
    let _ = app.emit("dictation:bands", bands);
}

/// Hides the panel and clears its waveform.
pub fn hide(app: &AppHandle) -> Result<()> {
    if let Some(state) = app.try_state::<PanelState>() {
        if let Ok(mut current) = state.0.lock() {
            *current = None;
        }
    }
    let _ = app.emit("dictation:hide", ());

    if let Some(window) = app.get_webview_window(PANEL_WINDOW_LABEL) {
        window
            .hide()
            .map_err(|e| DictationError::Platform(format!("hide dictation panel: {e}")))?;

        // And let the renderer go to sleep behind it. The panel is built on
        // demand and never closed, so without this it stays awake for the rest
        // of the session after one dictation. See `lazy_windows::hide`, which
        // is the same thing where the failure does not need reporting.
        crate::sleep::sleep_soon(&window);
    }
    Ok(())
}

fn position_at_bottom_center<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Result<()> {
    let monitor = window
        .primary_monitor()
        .map_err(|e| DictationError::Platform(format!("primary_monitor: {e}")))?
        .ok_or_else(|| DictationError::NotFound("primary monitor".to_string()))?;

    let scale = monitor.scale_factor();
    let monitor_size = monitor.size().to_logical::<f64>(scale);
    let monitor_position = monitor.position().to_logical::<f64>(scale);

    // Force the declared size before positioning so the centring maths has
    // real dimensions to work from.
    window
        .set_size(tauri::Size::Logical(LogicalSize {
            width: PANEL_WIDTH,
            height: PANEL_HEIGHT,
        }))
        .map_err(|e| DictationError::Platform(format!("dictation panel set_size: {e}")))?;

    let x = monitor_position.x + (monitor_size.width - PANEL_WIDTH) / 2.0;
    let y = monitor_position.y + monitor_size.height - PANEL_HEIGHT - PANEL_BOTTOM_MARGIN;

    window
        .set_position(tauri::Position::Logical(LogicalPosition { x, y }))
        .map_err(|e| DictationError::Platform(format!("dictation panel set_position: {e}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_serializes_to_what_the_route_matches_on() {
        // The Svelte side compares against these exact strings; a rename on
        // either side silently leaves the panel stuck on "Listening".
        assert_eq!(
            serde_json::to_string(&PanelStatus::Listening).unwrap(),
            "\"listening\""
        );
        assert_eq!(
            serde_json::to_string(&PanelStatus::Transcribing).unwrap(),
            "\"transcribing\""
        );
        assert_eq!(
            serde_json::to_string(&PanelStatus::Copied).unwrap(),
            "\"copied\""
        );
        assert_eq!(
            serde_json::to_string(&PanelStatus::Confirming).unwrap(),
            "\"confirming\""
        );
    }
}
