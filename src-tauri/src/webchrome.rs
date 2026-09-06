//! The browser's own chrome, switched off in every Sill window.
//!
//! Sill draws in WebView2, and WebView2 arrives with a browser's habits: a
//! context menu on right click and on the Menu key, developer tools on F12,
//! reload on F5, print on Ctrl+P, find on Ctrl+F, and the rest of Edge's
//! accelerator keys. None of those is Sill. A launcher that opens Edge's
//! context menu when the Menu key is pressed reads as broken, and a key the
//! browser takes for itself is a key Sill's own chords never see.
//!
//! ## Why this is done on the WebView2 settings and not in JavaScript
//!
//! A `preventDefault` in the page only reaches what the browser forwards, and
//! it has to be repeated in every window and survive every page. The
//! settings object on the WebView2 controller is where these behaviours are
//! decided, once, before the page has a say: default context menus off,
//! developer tools off, browser accelerator keys off. Sill's own keys are
//! unaffected, because those are read from the keydown events the page still
//! receives; only the browser's default handling of them is gone.
//!
//! ## Cost
//!
//! Three calls on a settings object, once per window, when it is built.

use tauri::WebviewWindow;

/// Turns off the context menu, developer tools and browser accelerator keys
/// in one window. Called once, when the window is built.
pub fn quiet(window: &WebviewWindow) {
    #[cfg(windows)]
    {
        let label = window.label().to_string();
        let _ = window.with_webview(move |webview| {
            use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
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
                let _ = settings.SetAreDefaultContextMenusEnabled(false);
                let _ = settings.SetAreDevToolsEnabled(false);
                // Accelerator keys arrived in a later settings interface than
                // the two above, so it is asked for rather than assumed.
                match settings.cast::<ICoreWebView2Settings3>() {
                    Ok(more) => {
                        let _ = more.SetAreBrowserAcceleratorKeysEnabled(false);
                    }
                    Err(_) => crate::say!("{label}: this WebView2 cannot switch off its accelerator keys"),
                }
            }
        });
    }

    #[cfg(not(windows))]
    let _ = window;
}
