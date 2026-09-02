//! Windows that are not made until they are wanted.
//!
//! ## What this is worth
//!
//! Measured on a fresh start with nothing opened by hand: **five declared
//! windows, five renderer processes, 412 MB between them.** One renderer per
//! window, and four of the five were hidden and doing nothing.
//!
//! `markup`, `capture` and `dictation` are built here instead, when something
//! first asks for them, which is worth roughly 246 MB at rest. `traymenu`
//! stays declared: it is opened by a click on the tray icon, where the wait
//! for a window to be built is a wait somebody is watching, and it is the one
//! of the four that gets used every session.
//!
//! ## Why this is not just an optimisation
//!
//! Rule 23 of the constitution asks for lazy initialisation and for near
//! nothing at rest, and the audit's headline claim is that Sill's brain idles
//! at eleven megabytes. Four invisible windows costing eighty megabytes each
//! is not a footnote against that, it is most of the answer.
//!
//! ## The correction this rests on
//!
//! The 2026-08-29 audit measured two windows sharing one renderer and
//! concluded a window costs about 34 MB. That was true of two windows.
//! `markup` and `capture` were added afterwards and the renderer count has
//! tracked the window count exactly ever since, at about 82 MB each.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Builds one of the deferred windows, or hands back the one already there.
///
/// Every property here is the one the window carried in `tauri.conf.json`
/// before it was deferred. They are not defaults and they are not taste: a
/// capture overlay that is not always on top is behind the thing it is
/// photographing, and one that is not transparent is a grey sheet over the
/// screen.
pub fn ensure(app: &AppHandle, label: &str) -> Result<WebviewWindow, String> {
    if let Some(existing) = app.get_webview_window(label) {
        return Ok(existing);
    }

    let builder = |url: &str| {
        WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            // Never focused on creation. Tauri's `focus` option defaults to
            // true, and a window created focused and invisible takes the
            // foreground from whatever the user was in, which is the bug that
            // cost a session in August.
            .focused(false)
            .visible(false)
            .decorations(false)
            .closable(false)
    };

    let window = match label {
        "markup" => builder("markup")
            .title("Sill markup")
            .inner_size(1100.0, 800.0)
            .min_inner_size(720.0, 520.0)
            .resizable(true)
            .transparent(false)
            .shadow(true)
            .build(),

        "capture" => builder("capture")
            .title("Sill capture")
            .inner_size(800.0, 600.0)
            .resizable(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .maximizable(false)
            .minimizable(false)
            .build(),

        "dictation" => builder("dictation")
            .title("Dictation")
            .inner_size(240.0, 84.0)
            .resizable(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .maximizable(false)
            .minimizable(false)
            .build(),

        other => return Err(format!("{other} is not a window Sill builds on demand")),
    };

    let window = window.map_err(|err| format!("could not make the {label} window: {err}"))?;

    crate::say!("built the {label} window");
    Ok(window)
}

/// The labels this module knows how to build.
///
/// Public so a test can assert that every one of them is buildable and that
/// none of them is still declared in the configuration, which is the mistake
/// that would give a window a renderer again without anybody noticing.
pub const DEFERRED: &[&str] = &["markup", "capture", "dictation"];

/**
Puts one of these away, and lets its renderer go to sleep.

**Building a window on demand only defers the cost; it does not end it.** These
are all `closable(false)` and are only ever hidden, so once a screenshot has
been taken the capture overlay's renderer is resident for the rest of the
session at about the 82 MB the module note measures. The saving was real until
the first use of the feature and then quietly gone, which is the worst shape
for a performance fix to have: it measures well on a fresh start and not at all
on a machine somebody has used.

`summon::hide` has always armed the sleep for the launcher. Nothing armed it
for these, so `sleep.rs` never saw them. Hiding through here rather than
calling `window.hide()` directly is what keeps that true as more windows are
deferred.

Silent when the window does not exist: not having been built is the cheapest
possible state and there is nothing to put away.
*/
pub fn hide(app: &AppHandle, label: &str) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };

    let _ = window.hide();
    crate::sleep::sleep_soon(&window);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// None of the deferred windows may also be declared.
    ///
    /// Declaring one is what costs the renderer, so a window that is both
    /// deferred and declared has all of the complexity and none of the saving,
    /// and nothing about the running app would look wrong.
    #[test]
    fn a_deferred_window_is_not_also_declared() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("the config parses");

        let declared: Vec<String> = config["app"]["windows"]
            .as_array()
            .expect("windows is a list")
            .iter()
            .filter_map(|w| w["label"].as_str().map(str::to_string))
            .collect();

        for label in DEFERRED {
            assert!(
                !declared.contains(&label.to_string()),
                "{label} is built on demand and still declared, so it costs a \
                 renderer at startup anyway"
            );
        }
    }

    /// The tray menu is deliberately not deferred.
    #[test]
    fn the_tray_menu_stays_declared() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("the config parses");

        let declared: Vec<String> = config["app"]["windows"]
            .as_array()
            .expect("windows is a list")
            .iter()
            .filter_map(|w| w["label"].as_str().map(str::to_string))
            .collect();

        assert!(
            declared.contains(&"traymenu".to_string()),
            "the tray menu is opened by a click, where building it is a wait \
             somebody is watching"
        );
        assert!(!DEFERRED.contains(&"traymenu"));
    }
}
