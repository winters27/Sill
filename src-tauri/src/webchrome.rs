//! The browser's own chrome, switched off in every Sill window.
//!
//! Sill draws in WebView2, and WebView2 arrives with a browser's habits: a
//! context menu on right click and on the Menu key, developer tools on F12,
//! reload on F5, print on Ctrl+P, find on Ctrl+F, zoom on Ctrl and the wheel,
//! a status bar for hovered links, pinch zoom, swipe to go back, SmartScreen
//! asking about every address, and the rest of Edge. None of those is Sill. A
//! launcher that opens Edge's context menu when the Menu key is pressed reads
//! as broken, and a key the browser takes for itself is a key Sill's own
//! chords never see.
//!
//! ## Why this is done on the WebView2 settings and not only in JavaScript
//!
//! A `preventDefault` in the page only reaches what the browser forwards, and
//! it has to survive every page. The settings object on the WebView2
//! controller is where these behaviours are decided, once, before the page has
//! a say. The page keeps a second, smaller layer (`src/lib/quiet.ts`) for what
//! the settings leave through and for honouring Sill's own hotkeys while a
//! Sill window has the keyboard.
//!
//! ## What is read back
//!
//! The context-menu switch is read back after it is set and the answer goes
//! in the log. Setting it and trusting it is how a window ends up with the
//! browser's menu and a log line saying it has none.
//!
//! ## Cost
//!
//! A handful of calls on a settings object, once per window, when it is built.

use tauri::WebviewWindow;

/// Turns off the browser's chrome in one window. Called once, when the
/// window is built.
pub fn quiet(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        let label = window.label().to_string();
        let _ = window.with_webview(move |webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::{
                ICoreWebView2Settings3, ICoreWebView2Settings5, ICoreWebView2Settings6,
                ICoreWebView2Settings8,
            };
            use webview2_core::Interface;

            // SAFETY: COM calls on interfaces the controller hands out, made
            // on the thread Tauri runs this closure on, which is the one that
            // owns them.
            unsafe {
                let Ok(core) = webview.controller().CoreWebView2() else {
                    crate::say!("{label}: no WebView2 core to quiet");
                    return;
                };
                let Ok(settings) = core.Settings() else {
                    return;
                };

                // The first settings interface: every WebView2 has it.
                let _ = settings.SetAreDefaultContextMenusEnabled(false);
                let _ = settings.SetAreDevToolsEnabled(false);
                let _ = settings.SetIsZoomControlEnabled(false);
                let _ = settings.SetIsStatusBarEnabled(false);
                let _ = settings.SetIsBuiltInErrorPageEnabled(false);

                // The later ones arrived with later runtimes, so each is asked
                // for rather than assumed, and a runtime without one simply
                // keeps that habit.
                let accelerators = settings
                    .cast::<ICoreWebView2Settings3>()
                    .map(|more| more.SetAreBrowserAcceleratorKeysEnabled(false).is_ok())
                    .unwrap_or(false);
                let pinch = settings
                    .cast::<ICoreWebView2Settings5>()
                    .map(|more| more.SetIsPinchZoomEnabled(false).is_ok())
                    .unwrap_or(false);
                let swipe = settings
                    .cast::<ICoreWebView2Settings6>()
                    .map(|more| more.SetIsSwipeNavigationEnabled(false).is_ok())
                    .unwrap_or(false);
                let reputation = settings
                    .cast::<ICoreWebView2Settings8>()
                    .map(|more| more.SetIsReputationCheckingRequired(false).is_ok())
                    .unwrap_or(false);

                // Read back, not trusted.
                let mut menus = webview2_core::BOOL(1);
                let _ = settings.AreDefaultContextMenusEnabled(&mut menus);
                let mut tools = webview2_core::BOOL(1);
                let _ = settings.AreDevToolsEnabled(&mut tools);

                let kept: Vec<&str> = [
                    (menus.as_bool(), "context menus"),
                    (tools.as_bool(), "devtools"),
                    (!accelerators, "accelerator keys"),
                    (!pinch, "pinch zoom"),
                    (!swipe, "swipe navigation"),
                    (!reputation, "reputation checks"),
                ]
                .into_iter()
                .filter_map(|(still_on, name)| still_on.then_some(name))
                .collect();

                if kept.is_empty() {
                    crate::say!("{label}: browser chrome off");
                } else {
                    crate::say!("{label}: browser chrome off, except {}", kept.join(", "));
                }
            }
        });
    }

    #[cfg(not(windows))]
    let _ = window;
}
