//! The API layer: what an extension can ask of Sill.
//!
//! Signatures follow the MIT `@raycast/api` type declarations, which are the
//! spec. Coverage is the M1 subset and grows as extensions demand it.
//!
//! Anything not implemented returns a JSON-RPC error naming the method rather
//! than a silent default, so gaps show up in the log as gaps.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::rpc::RpcError;

/// Something the extension wants the UI to do.
///
/// The API layer never touches the window directly. It emits these and the
/// Tauri layer forwards them, which keeps the protocol testable without a UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UiEvent {
    /// A batch of tree patches from the reconciler.
    Render { session: String, ops: Value },
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
    HideToast { session: String, id: String },
    ShowHud { session: String, text: String },
    SetSearchText { session: String, text: String },
    PopToRoot { session: String },
    CloseMainWindow { session: String },
}

/// Per-extension key/value store backing `LocalStorage`.
///
/// In memory for M1. Persisting it is M4 work, and doing it now would bake in
/// a layout before the extension identity model is settled.
#[derive(Default)]
struct Storage {
    by_extension: HashMap<String, HashMap<String, Value>>,
}

pub struct ApiLayer {
    events: mpsc::UnboundedSender<UiEvent>,
    storage: Arc<Mutex<Storage>>,
}

impl ApiLayer {
    pub fn new(events: mpsc::UnboundedSender<UiEvent>) -> Self {
        Self {
            events,
            storage: Arc::new(Mutex::new(Storage::default())),
        }
    }

    /// Handles one API call from a session. `extension` scopes storage.
    pub fn dispatch(
        &self,
        session: &str,
        extension: &str,
        method: &str,
        params: &Value,
    ) -> Result<Value, RpcError> {
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

            "Storage/get" => {
                let store = self.storage.lock().expect("storage poisoned");
                Ok(store
                    .by_extension
                    .get(extension)
                    .and_then(|kv| kv.get(&string("key")))
                    .cloned()
                    .unwrap_or(Value::Null))
            }

            "Storage/set" => {
                let mut store = self.storage.lock().expect("storage poisoned");
                store
                    .by_extension
                    .entry(extension.to_string())
                    .or_default()
                    .insert(string("key"), params.get("value").cloned().unwrap_or(Value::Null));
                Ok(Value::Null)
            }

            "Storage/remove" => {
                let mut store = self.storage.lock().expect("storage poisoned");
                if let Some(kv) = store.by_extension.get_mut(extension) {
                    kv.remove(&string("key"));
                }
                Ok(Value::Null)
            }

            "Storage/clear" => {
                let mut store = self.storage.lock().expect("storage poisoned");
                store.by_extension.remove(extension);
                Ok(Value::Null)
            }

            "Storage/list" => {
                let store = self.storage.lock().expect("storage poisoned");
                let map = store
                    .by_extension
                    .get(extension)
                    .cloned()
                    .unwrap_or_default();
                Ok(Value::Object(map.into_iter().collect()))
            }

            _ => Err(RpcError::method_not_found(method)),
        }
    }

    fn emit(&self, event: UiEvent) {
        // A closed receiver means the UI is gone, which is not the extension's
        // problem and not worth failing its call over.
        let _ = self.events.send(event);
    }
}
