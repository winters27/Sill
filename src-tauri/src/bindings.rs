//! Global shortcuts that run an action without the launcher appearing.
//!
//! This is the point of having built an action registry. A shortcut does not
//! get its own implementation of anything: it names an action and says what to
//! run it against, and the same code that runs behind Enter and behind the
//! action panel runs here too. Adding a transform makes it bindable for free,
//! and fixing one fixes it everywhere.
//!
//! The launcher is never shown. Highlight some text, press the key, and the
//! text changes where it sits. Summoning a window to do that would defeat the
//! point, and it would also make the selection unreadable: reading a selection
//! means pressing Ctrl+C in the foreground application, and Sill must not be
//! the foreground application when that happens.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::action::{ActionCtx, ActionRegistry};
use crate::object::{Object, ObjectKind};

/// Where a bound action gets the thing it acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "camelCase")]
pub enum Source {
    /// Whatever is highlighted, falling back to the clipboard.
    ///
    /// The fallback is what makes a bound transform usable rather than fussy.
    /// Pressing the key with nothing highlighted should do the obvious thing
    /// rather than nothing, and the last thing copied is the obvious thing.
    Selection,
    /// What is on the clipboard, whatever is highlighted.
    Clipboard,
    /// One particular thing from the index, named once when the binding is
    /// made. This is how a key opens a specific application.
    Command { id: String },
}

/// One key, one action, one thing to run it against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    /// An accelerator like `Ctrl+Alt+U`.
    pub accelerator: String,
    /// The action's stable id, which is why action ids are stable.
    pub action: String,
    pub source: Source,
    /// Put the result back where the text came from.
    ///
    /// Only meaningful for a selection: replacing means pressing Ctrl+V, and
    /// there is nothing to paste over when the source was the clipboard.
    #[serde(default = "yes")]
    pub replace: bool,
}

fn yes() -> bool {
    true
}

/// Registers every binding, releasing whatever was registered before.
///
/// Called on startup and whenever the bindings change. Releasing first matters:
/// leaving a stale accelerator registered means the old key keeps working,
/// which looks exactly like the setting having been ignored.
pub fn apply(app: &AppHandle, previous: &[Binding], current: &[Binding]) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    for binding in previous {
        // Only the ones that are actually going away. Unregistering and
        // immediately re-registering an unchanged accelerator leaves a window
        // in which the key does nothing.
        if !current.iter().any(|b| b.accelerator == binding.accelerator) {
            let _ = app.global_shortcut().unregister(binding.accelerator.as_str());
        }
    }

    for binding in current {
        if previous.iter().any(|b| b == binding) {
            continue;
        }

        // Changed rather than new: release the old meaning before binding it.
        if previous
            .iter()
            .any(|b| b.accelerator == binding.accelerator)
        {
            let _ = app.global_shortcut().unregister(binding.accelerator.as_str());
        }

        let handle = app.clone();
        let bound = binding.clone();

        let result = app.global_shortcut().on_shortcut(
            binding.accelerator.as_str(),
            move |_, _, event| {
                // Fires on press and release; acting on both runs everything
                // twice, which for a transform means transforming the result.
                if event.state() != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                    return;
                }

                let handle = handle.clone();
                let bound = bound.clone();
                tauri::async_runtime::spawn(async move { fire(&handle, &bound).await });
            },
        );

        match result {
            Ok(()) => println!(
                "[sill] {} runs {} on {:?}",
                binding.accelerator, binding.action, binding.source
            ),
            // Another application already holds the combination. Reported
            // rather than fatal: one unusable key is not a reason to refuse
            // to start.
            Err(err) => crate::say!("could not bind {}: {err}", binding.accelerator),
        }
    }
}

/// Runs one binding.
async fn fire(app: &AppHandle, binding: &Binding) {
    let registry = app.state::<ActionRegistry>();
    let Some(action) = registry.get(&binding.action) else {
        crate::say!("{} is bound to {}, which does not exist", binding.accelerator, binding.action);
        return;
    };

    let object = match resolve(app, binding).await {
        Ok(object) => object,
        Err(reason) => {
            crate::say!("{}: {reason}", binding.accelerator);
            return;
        }
    };

    if !action.accepts(object.kind) {
        crate::say!(
            "{} is bound to {}, which cannot be done to {:?}",
            binding.accelerator,
            binding.action,
            object.kind
        );
        return;
    }

    let outcome = match action.run(&ActionCtx { app: app.clone() }, &object).await {
        Ok(outcome) => outcome,
        Err(reason) => {
            crate::say!("{} failed: {reason}", binding.accelerator);
            return;
        }
    };

    // Put the result back where the text came from. Only for a selection: a
    // binding that reads the clipboard has nothing to paste over, and pressing
    // Ctrl+V into whatever happens to be in front would be destructive.
    if binding.replace && matches!(binding.source, Source::Selection) {
        if let Some(text) = outcome.text {
            if let Err(reason) = crate::selection::replace(app, &text) {
                crate::say!("{}: {reason}", binding.accelerator);
            }
        }
    }
}

/// What the binding acts on.
async fn resolve(app: &AppHandle, binding: &Binding) -> Result<Object, String> {
    match &binding.source {
        Source::Selection | Source::Clipboard => {
            let captured = if matches!(binding.source, Source::Selection) {
                // Blocking: reading a selection presses Ctrl+C and waits for
                // the other application to answer, which is not something to
                // do on a runtime worker.
                let handle = app.clone();
                tokio::task::spawn_blocking(move || {
                    crate::selection::Captured::selection_or_clipboard(&handle)
                })
                .await
                .map_err(|err| format!("the capture did not finish: {err}"))?
            } else {
                crate::selection::Captured::clipboard(app)
            };

            let captured = captured.ok_or_else(|| "nothing selected and nothing copied".to_string())?;

            Ok(Object {
                kind: ObjectKind::Text,
                id: "selection".to_string(),
                title: preview(&captured.text),
                target: captured.text,
                mode: "text".to_string(),
            })
        }

        Source::Command { id } => {
            let registry = app.state::<crate::state::RegistryState>();
            let record = registry
                .inner
                .lock()
                .await
                .commands
                .iter()
                .find(|c| &c.id == id)
                .cloned()
                .ok_or_else(|| format!("nothing in the index is called {id}"))?;

            Object::from_record(&record)
                .ok_or_else(|| format!("{} is a kind of thing Sill cannot act on", record.title))
        }
    }
}

/// A one-line stand-in for a block of text, for logs and messages.
fn preview(text: &str) -> String {
    let first = text.lines().next().unwrap_or_default().trim();
    let mut out: String = first.chars().take(40).collect();
    if first.chars().count() > 40 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(accelerator: &str, action: &str) -> Binding {
        Binding {
            accelerator: accelerator.into(),
            action: action.into(),
            source: Source::Selection,
            replace: true,
        }
    }

    #[test]
    fn a_binding_round_trips_through_its_stored_form() {
        for source in [
            Source::Selection,
            Source::Clipboard,
            Source::Command {
                id: "app:code".into(),
            },
        ] {
            let original = Binding {
                source,
                ..binding("Ctrl+Alt+U", "sill.text.upper")
            };

            let text = serde_json::to_string(&original).expect("serialises");
            let back: Binding = serde_json::from_str(&text).expect("parses");
            assert_eq!(back, original);
        }
    }

    #[test]
    fn a_binding_written_before_replace_existed_still_replaces() {
        // The upgrade case. Defaulting this to false would silently turn every
        // existing transform binding into one that copies and does nothing
        // visible, which reads as the shortcut having broken.
        let older = r#"{"accelerator":"Ctrl+Alt+U","action":"sill.text.upper","source":{"from":"selection"}}"#;
        let parsed: Binding = serde_json::from_str(older).expect("parses");
        assert!(parsed.replace);
    }

    #[test]
    fn a_preview_is_one_line_and_short() {
        // It goes in a log line and a status message; a paragraph in either is
        // unreadable and a multi-line one breaks the format.
        let long = "first line that runs on and on and on and on and on\nsecond line";
        let shown = preview(long);

        assert!(!shown.contains('\n'));
        assert!(shown.chars().count() <= 41, "{shown:?}");
        assert!(shown.ends_with('…'));

        assert_eq!(preview("  short  "), "short");
        assert_eq!(preview(""), "");
    }
}
