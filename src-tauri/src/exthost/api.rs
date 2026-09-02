//! The API layer: what an extension can ask of Sill.
//!
//! Signatures follow the MIT `@raycast/api` type declarations, which are the
//! spec. Coverage is the M1 subset and grows as extensions demand it.
//!
//! Anything not implemented returns a JSON-RPC error naming the method rather
//! than a silent default, so gaps show up in the log as gaps.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::bridge::{Alert, Bridge, Clip};
use super::permission::{self, Permits};
use super::rpc::RpcError;
use super::storage::Storage;

/// Something the extension wants the UI to do.
///
/// The API layer never touches the window directly. It emits these and the
/// Tauri layer forwards them, which keeps the protocol testable without a UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiEvent {
    /// A batch of tree patches from the reconciler.
    Render {
        session: String,
        ops: Value,
    },
    ShowToast {
        session: String,
        id: String,
        title: String,
        message: String,
        style: String,
    },
    UpdateToast {
        session: String,
        id: String,
        title: String,
        message: String,
        style: String,
    },
    HideToast {
        session: String,
        id: String,
    },
    ShowHud {
        session: String,
        text: String,
    },
    SetSearchText {
        session: String,
        text: String,
    },
    PopToRoot {
        session: String,
    },
    CloseMainWindow {
        session: String,
    },
    /// A command died on its own.
    ///
    /// Was logged and dropped, which left the window waiting for a first
    /// render that was never coming. An extension that crashes on load is
    /// indistinguishable from one that is slow, and the user gets an empty
    /// screen with no way to tell which.
    Crashed {
        session: String,
        reason: String,
    },
}

pub struct ApiLayer {
    events: mpsc::UnboundedSender<UiEvent>,
    /// Everything an extension can reach outside its own process.
    bridge: Arc<dyn Bridge>,
    storage: Arc<Storage>,
    /// Whether it is allowed to. Separate from `bridge`, which is how a thing
    /// gets done rather than whether it may be: one of them would otherwise
    /// have to refuse in the middle of doing.
    permits: Arc<dyn Permits>,
}

impl ApiLayer {
    pub fn new(
        events: mpsc::UnboundedSender<UiEvent>,
        bridge: Arc<dyn Bridge>,
        storage: Arc<Storage>,
        permits: Arc<dyn Permits>,
    ) -> Self {
        Self {
            events,
            bridge,
            storage,
            permits,
        }
    }

    /// Who decides whether a capability may be used.
    ///
    /// Exposed so the built-in actions the window performs on an extension's
    /// behalf ask the same question the API layer does. They reach the same
    /// capabilities by another route, and for a while they did it without
    /// asking anybody.
    pub fn permits(&self) -> &Arc<dyn Permits> {
        &self.permits
    }

    /// Handles one API call from a session. `extension` scopes storage.
    ///
    /// Async because two of these wait on the world: a confirmation waits for
    /// the person, and the clipboard waits for whichever application currently
    /// holds it. Both run off the runtime's worker threads rather than on one.
    pub async fn dispatch(
        &self,
        session: &str,
        extension: &str,
        method: &str,
        params: &Value,
    ) -> Result<Value, RpcError> {
        /*
         * Nothing runs before it is allowed to.
         *
         * At the top rather than inside each arm, because a check written
         * twenty-two times is a check that will be missed once, and the one it
         * is missed on is a new method somebody added in a hurry.
         *
         * A method with no row in `permission::NEEDED` is refused rather than
         * run. It is the same answer an unknown method already got, and it
         * means the failure of forgetting a row is an extension that cannot
         * call the thing, never an extension that calls it unpermitted.
         */
        let Some(needs) = permission::needed(method) else {
            return Err(RpcError::method_not_found(method));
        };

        if let Err(why) = self.permits.allow(extension, needs).await {
            return Err(RpcError::internal(why));
        }

        let string = |key: &str| -> String {
            params
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };

        match method {
            "UI/render" => {
                let ops = params.get("ops").cloned().unwrap_or_else(|| json!([]));
                self.emit(UiEvent::Render {
                    session: session.to_string(),
                    ops,
                });
                Ok(Value::Null)
            }

            "UI/showToast" => {
                self.emit(UiEvent::ShowToast {
                    session: session.to_string(),
                    id: string("id"),
                    title: string("title"),
                    message: string("message"),
                    style: string("style"),
                });
                Ok(Value::Null)
            }

            "UI/updateToast" => {
                self.emit(UiEvent::UpdateToast {
                    session: session.to_string(),
                    id: string("id"),
                    title: string("title"),
                    message: string("message"),
                    style: string("style"),
                });
                Ok(Value::Null)
            }

            "UI/hideToast" => {
                self.emit(UiEvent::HideToast {
                    session: session.to_string(),
                    id: string("id"),
                });
                Ok(Value::Null)
            }

            "UI/showHud" => {
                self.emit(UiEvent::ShowHud {
                    session: session.to_string(),
                    text: string("text"),
                });
                Ok(Value::Null)
            }

            "UI/setSearchText" => {
                self.emit(UiEvent::SetSearchText {
                    session: session.to_string(),
                    text: string("text"),
                });
                Ok(Value::Null)
            }

            "UI/popToRoot" => {
                self.emit(UiEvent::PopToRoot {
                    session: session.to_string(),
                });
                Ok(Value::Null)
            }

            "UI/closeMainWindow" => {
                self.emit(UiEvent::CloseMainWindow {
                    session: session.to_string(),
                });
                Ok(Value::Null)
            }

            "Storage/get" => Ok(self.storage.get(extension, &string("key"))),

            "Storage/set" => {
                let value = params.get("value").cloned().unwrap_or(Value::Null);
                self.storage
                    .set(extension, &string("key"), &value)
                    .map_err(|err| RpcError::internal(format!("could not save: {err}")))?;
                Ok(Value::Null)
            }

            "Storage/remove" => {
                self.storage
                    .remove(extension, &string("key"))
                    .map_err(|err| RpcError::internal(format!("could not remove: {err}")))?;
                Ok(Value::Null)
            }

            "Storage/clear" => {
                self.storage
                    .clear(extension)
                    .map_err(|err| RpcError::internal(format!("could not clear: {err}")))?;
                Ok(Value::Null)
            }

            "Storage/list" => Ok(Value::Object(self.storage.list(extension))),

            // ------------------------------------------------------ clipboard
            //
            // These were the gap that mattered. The host has always called
            // them and nothing answered, so `Clipboard.copy` (which is close
            // to the most-used call in the whole Raycast ecosystem) failed
            // with "method not found" in every extension that used it.
            "Clipboard/copy" => {
                let clip = clip_from(params.get("content"), params.get("options"));
                self.blocking(move |bridge| bridge.clipboard_write(&clip))
                    .await?;
                Ok(Value::Null)
            }

            "Clipboard/paste" => {
                let clip = clip_from(params.get("content"), params.get("options"));
                self.blocking(move |bridge| bridge.clipboard_paste(&clip))
                    .await?;
                Ok(Value::Null)
            }

            "Clipboard/clear" => {
                self.blocking(|bridge| bridge.clipboard_clear()).await?;
                Ok(Value::Null)
            }

            "Clipboard/readContent" => {
                let clip = self.blocking(|bridge| bridge.clipboard_read()).await?;
                // Shaped as Raycast's `Clipboard.ReadContent`: the keys are
                // present and null rather than absent, because extensions
                // destructure this rather than probing it.
                Ok(json!({
                    "text": clip.text,
                    "html": clip.html,
                    "file": clip.file,
                }))
            }

            // ---------------------------------------------------- applications
            "Application/open" => {
                let target = string("target");
                if target.is_empty() {
                    return Err(RpcError::internal("open was given nothing to open"));
                }
                let with = params
                    .get("appId")
                    .and_then(Value::as_str)
                    .filter(|id| !id.is_empty())
                    .map(str::to_string);

                self.blocking(move |bridge| bridge.open(&target, with.as_deref()))
                    .await?;
                Ok(Value::Null)
            }

            "Application/getDefault" => {
                let target = string("target");
                if target.is_empty() {
                    return Err(RpcError::internal("getDefault was given nothing"));
                }

                let found = self
                    .blocking(move |bridge| bridge.default_application(&target))
                    .await?;

                // Null rather than an error when nothing is registered. An
                // address with no handler is an ordinary state of a machine,
                // not a fault in the extension that asked about it.
                match found {
                    Some(app) => serde_json::to_value(app)
                        .map_err(|err| RpcError::internal(format!("could not describe it: {err}"))),
                    None => Ok(Value::Null),
                }
            }

            "Application/list" => {
                let apps = self.blocking(|bridge| bridge.applications()).await?;
                serde_json::to_value(apps).map_err(|err| {
                    RpcError::internal(format!("could not list applications: {err}"))
                })
            }

            // ------------------------------------------------------------- ui
            "UI/getSelectedText" => {
                let selected = self.blocking(|bridge| bridge.selected_text()).await?;

                // An empty string rather than null when nothing is selected,
                // because the call is typed as returning a string and every
                // extension using it goes straight to `.trim()`.
                Ok(Value::String(selected.unwrap_or_default()))
            }

            "UI/confirmAlert" => {
                let alert = alert_from(params.get("payload").unwrap_or(params));
                let answered = self.blocking(move |bridge| bridge.confirm(&alert)).await?;
                Ok(Value::Bool(answered))
            }

            _ => Err(RpcError::method_not_found(method)),
        }
    }

    /// Runs one bridge call off the runtime's worker threads.
    ///
    /// Every capability here can block for an unbounded time: the clipboard is
    /// a single system-wide resource held under a lock by whoever last wrote
    /// to it, and a confirmation waits for a person. Doing either inline would
    /// stall a Tokio worker, and with enough sessions that starves the runtime
    /// that is also driving search.
    async fn blocking<T, F>(&self, work: F) -> Result<T, RpcError>
    where
        T: Send + 'static,
        F: FnOnce(&dyn Bridge) -> Result<T, String> + Send + 'static,
    {
        let bridge = self.bridge.clone();

        tokio::task::spawn_blocking(move || work(bridge.as_ref()))
            .await
            .map_err(|err| RpcError::internal(format!("that call did not finish: {err}")))?
            .map_err(RpcError::internal)
    }

    /// Tells the window a command died.
    ///
    /// Not part of `dispatch`, because a crash is the one thing an extension
    /// cannot report about itself.
    pub fn report_crash(&self, session: &str, reason: &str) {
        self.emit(UiEvent::Crashed {
            session: session.to_string(),
            reason: reason.to_string(),
        });
    }

    fn emit(&self, event: UiEvent) {
        // A closed receiver means the UI is gone, which is not the extension's
        // problem and not worth failing its call over.
        let _ = self.events.send(event);
    }
}

/// Reads Raycast's `Clipboard.Content`, which is deliberately loose.
///
/// It may be a bare string, a number, or an object carrying any of `text`,
/// `html` and `file`. Extensions in the wild use all four forms, so accepting
/// only the documented object would break most of them.
fn clip_from(content: Option<&Value>, options: Option<&Value>) -> Clip {
    let concealed = options
        .and_then(|o| o.get("concealed"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let field = |value: &Value, key: &str| -> Option<String> {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    match content {
        Some(Value::String(text)) => Clip {
            text: Some(text.clone()),
            concealed,
            ..Clip::default()
        },
        // A number is not a string but is plainly meant as one. Refusing it
        // would fail on `Clipboard.copy(count)`, which people write.
        Some(Value::Number(number)) => Clip {
            text: Some(number.to_string()),
            concealed,
            ..Clip::default()
        },
        Some(value @ Value::Object(_)) => Clip {
            text: field(value, "text"),
            html: field(value, "html"),
            file: field(value, "file"),
            concealed,
        },
        _ => Clip {
            concealed,
            ..Clip::default()
        },
    }
}

/// Reads Raycast's `Alert.Options`.
///
/// The button labels are nested under `primaryAction` and `dismissAction`, and
/// a destructive action is marked on the primary one rather than on the alert.
fn alert_from(options: &Value) -> Alert {
    let text = |value: &Value, key: &str| -> Option<String> {
        value
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    let primary = options.get("primaryAction");
    let dismiss = options.get("dismissAction");

    Alert {
        // Never empty: a dialog with no text is a dialog nobody can answer.
        title: text(options, "title").unwrap_or_else(|| "Are you sure?".to_string()),
        message: text(options, "message"),
        primary: primary.and_then(|action| text(action, "title")),
        dismiss: dismiss.and_then(|action| text(action, "title")),
        destructive: primary
            .and_then(|action| action.get("style"))
            .and_then(Value::as_str)
            .map(|style| style.eq_ignore_ascii_case("destructive"))
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_content_is_accepted_in_every_shape_extensions_use() {
        assert_eq!(
            clip_from(Some(&json!("plain")), None).text.as_deref(),
            Some("plain")
        );
        assert_eq!(
            clip_from(Some(&json!(42)), None).text.as_deref(),
            Some("42"),
            "a number is meant as text, not as a reason to fail"
        );

        let rich = clip_from(Some(&json!({"text": "hi", "html": "<b>hi</b>"})), None);
        assert_eq!(rich.text.as_deref(), Some("hi"));
        assert_eq!(rich.html.as_deref(), Some("<b>hi</b>"));

        assert_eq!(
            clip_from(Some(&json!({"file": "C:\\a.txt"})), None)
                .file
                .as_deref(),
            Some("C:\\a.txt")
        );
    }

    #[test]
    fn an_empty_field_is_treated_as_absent() {
        // `{text: ""}` is what a template literal produces when its value was
        // undefined, and writing an empty string over the clipboard is not
        // what the extension meant.
        let clip = clip_from(Some(&json!({"text": "", "html": "<b>x</b>"})), None);
        assert!(clip.text.is_none());
        assert_eq!(clip.html.as_deref(), Some("<b>x</b>"));
    }

    #[test]
    fn concealed_survives_from_the_options_object() {
        // The flag that decides whether a secret reaches clipboard history.
        let clip = clip_from(Some(&json!("hunter2")), Some(&json!({"concealed": true})));
        assert!(clip.concealed);

        assert!(!clip_from(Some(&json!("public")), None).concealed);
    }

    #[test]
    fn an_alert_reads_its_labels_out_of_the_nested_actions() {
        let alert = alert_from(&json!({
            "title": "Delete it?",
            "message": "This cannot be undone.",
            "primaryAction": { "title": "Delete", "style": "destructive" },
            "dismissAction": { "title": "Keep" },
        }));

        assert_eq!(alert.title, "Delete it?");
        assert_eq!(alert.message.as_deref(), Some("This cannot be undone."));
        assert_eq!(alert.primary.as_deref(), Some("Delete"));
        assert_eq!(alert.dismiss.as_deref(), Some("Keep"));
        assert!(alert.destructive);
    }

    #[test]
    fn an_alert_with_nothing_in_it_still_asks_a_question() {
        // A dialog whose title is the empty string is one nobody can answer.
        let alert = alert_from(&json!({}));
        assert!(!alert.title.is_empty());
        assert!(!alert.destructive);
    }
}
