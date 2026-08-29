//! Sill's side of [`exthost::Bridge`].
//!
//! Every capability an extension has reaches the system through here, and
//! nowhere else. That is the point of the seam: it is one file to read to know
//! what an extension can do, one place to add a permission check, and one
//! place to log from when auditing arrives.
//!
//! Nothing here is clever. It is the launcher's existing behaviour, made
//! reachable by an extension instead of only by the person at the keyboard.

use std::sync::Arc;

use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::exthost::{Alert, AppInfo, Bridge, Clip};

pub struct SillBridge {
    app: AppHandle,
}

impl SillBridge {
    pub fn new(app: AppHandle) -> Arc<Self> {
        Arc::new(Self { app })
    }

    /// Puts the launcher away so an action lands where the user was.
    ///
    /// The same courtesy `launch_command` does for snippets: Sill is
    /// frontmost when an extension runs, so anything typed or pasted from one
    /// would otherwise arrive in the launcher rather than in the work.
    fn step_aside(&self) {
        if let Some(window) = self.app.get_webview_window("main") {
            crate::summon::hide(&window);
        }
    }

    fn write(&self, clip: &Clip) -> Result<(), String> {
        // Told before the write, not after. The clipboard listener fires on a
        // thread of its own the moment the contents change, so a flag set
        // afterwards is set too late and the secret is already recorded.
        if clip.concealed {
            if let Some(history) = self.app.try_state::<crate::clipboard::monitor::Clipboard>() {
                history.ignore_next();
            }
        }

        // HTML first when both are present, because it carries the plain text
        // alongside it and writing them separately would leave whichever went
        // last on the clipboard alone.
        if let Some(html) = clip.html.as_deref() {
            let plain = clip.text.as_deref().unwrap_or(html);
            return self
                .app
                .clipboard()
                .write_html(html, Some(plain))
                .map_err(|err| format!("could not copy: {err}"));
        }

        let text = clip
            .as_text()
            .ok_or_else(|| "that copy carried nothing to put on the clipboard".to_string())?;

        self.app
            .clipboard()
            .write_text(text)
            .map_err(|err| format!("could not copy: {err}"))
    }
}

impl Bridge for SillBridge {
    fn clipboard_write(&self, clip: &Clip) -> Result<(), String> {
        self.write(clip)
    }

    fn clipboard_read(&self) -> Result<Clip, String> {
        // A clipboard holding an image, or nothing, is not an error. It is an
        // ordinary state, and an extension asking what is there should be told
        // "no text" rather than handed a failure to explain.
        let text = self.app.clipboard().read_text().ok();

        Ok(Clip {
            text,
            ..Clip::default()
        })
    }

    fn clipboard_clear(&self) -> Result<(), String> {
        self.app
            .clipboard()
            .clear()
            .map_err(|err| format!("could not clear the clipboard: {err}"))
    }

    fn clipboard_paste(&self, clip: &Clip) -> Result<(), String> {
        self.write(clip)?;
        self.step_aside();

        // The same settle every paste in Sill needs. Writing and immediately
        // pasting races the target application's read of the clipboard, and
        // the symptom is the previous contents arriving instead of the new.
        std::thread::sleep(std::time::Duration::from_millis(60));
        crate::dictation::paste::chord();
        Ok(())
    }

    fn open(&self, target: &str, with: Option<&str>) -> Result<(), String> {
        self.step_aside();

        // `open_path` handles both: given a URL it hands off to the shell the
        // same way, and unlike `open_url` it accepts the application argument.
        tauri_plugin_opener::open_path(target, with)
            .map_err(|err| format!("could not open {target}: {err}"))
    }

    fn applications(&self) -> Result<Vec<AppInfo>, String> {
        // Read out of the index the launcher already holds rather than
        // scanned. A scan is a PowerShell round trip and a few thousand
        // filesystem calls, and an extension asking what is installed must not
        // be able to cost that whenever it likes.
        let state = self
            .app
            .try_state::<crate::RegistryState>()
            .ok_or_else(|| "the application index is not ready yet".to_string())?;

        let registry = state
            .inner
            .try_lock()
            .map_err(|_| "the application index is busy being rebuilt".to_string())?;

        Ok(registry
            .commands
            .iter()
            .filter(|command| command.mode == "app")
            .map(|command| AppInfo {
                name: command.title.clone(),
                path: command.entrypoint.clone(),
                bundle_id: None,
            })
            .collect())
    }

    fn confirm(&self, alert: &Alert) -> Result<bool, String> {
        let buttons = match (alert.primary.as_deref(), alert.dismiss.as_deref()) {
            (Some(yes), Some(no)) => {
                MessageDialogButtons::OkCancelCustom(yes.to_string(), no.to_string())
            }
            // Only one label given, so the other keeps the system's word for
            // it. Inventing "Cancel" in English would be wrong on a machine
            // that is not running in English.
            (Some(yes), None) => MessageDialogButtons::OkCancelCustom(
                yes.to_string(),
                "Cancel".to_string(),
            ),
            _ => MessageDialogButtons::OkCancel,
        };

        let kind = if alert.destructive {
            MessageDialogKind::Warning
        } else {
            MessageDialogKind::Info
        };

        // Blocking is correct here and is why the API layer runs this off a
        // worker thread: the extension is awaiting an answer and there is
        // nothing to do until a person gives one.
        Ok(self
            .app
            .dialog()
            .message(alert.message.clone().unwrap_or_default())
            .title(&alert.title)
            .buttons(buttons)
            .kind(kind)
            .blocking_show())
    }
}
