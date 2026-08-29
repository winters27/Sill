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

/// Window label, matching `tauri.conf.json`.
pub const PANEL_WINDOW_LABEL: &str = "dictation";

/// Declared size, repeated here because `outer_size()` reports 0x0 for a
/// window that has never been shown, and this one is declared `visible:
/// false`. The HUD hardcodes its own dimensions for exactly this reason.
const PANEL_WIDTH: f64 = 240.0;
const PANEL_HEIGHT: f64 = 84.0;

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
    let window = app.get_webview_window(PANEL_WINDOW_LABEL).ok_or_else(|| {
        // A missing window here means tauri.conf.json and this constant have
        // drifted apart, which is silent otherwise.
        crate::say!("no window labelled '{PANEL_WINDOW_LABEL}'");
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

    window
        .show()
        .map_err(|e| DictationError::Platform(format!("show dictation panel: {e}")))?;

    // Emitted AFTER showing: a hidden window's webview may not be running,
    // and an event delivered to a window that is not listening yet is lost
    // with no error anywhere.
    app.emit("dictation:status", status)
        .map_err(|e| DictationError::Platform(format!("emit dictation:status: {e}")))?;
    crate::say!("panel shown ({status:?})");

    Ok(())
}

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
    }
    Ok(())
}

fn position_at_bottom_center<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Result<()> {
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
