//! JSON-RPC 2.0 peer for the extension host channel.
//!
//! Mirrors `host/src/proto/rpc.ts`. A message carrying an `id` is a request or
//! its response; a message carrying a `method` and no `id` is an event.
//!
//! Outbound requests resolve through a pending map. Inbound requests and
//! events are handed to the owner over a channel rather than dispatched here,
//! which keeps this type free of trait objects and lets the owner decide what
//! to do without holding any lock.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

/// An error returned by the peer, in JSON-RPC's shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl RpcError {
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: message.into(),
            data: None,
        }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.data {
            Some(data) => write!(f, "{} ({}): {}", self.message, self.code, data),
            None => write!(f, "{} ({})", self.message, self.code),
        }
    }
}

impl std::error::Error for RpcError {}

/// Something the peer received that the owner has to act on.
#[derive(Debug)]
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Event {
        method: String,
        params: Value,
    },
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>>;

/// One end of a JSON-RPC conversation.
#[derive(Clone)]
pub struct RpcPeer {
    outbound: mpsc::UnboundedSender<String>,
    incoming: mpsc::UnboundedSender<Incoming>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
}

impl RpcPeer {
    /// Returns the peer plus the streams the caller must drive: outbound text
    /// to be framed and written, and inbound work to be dispatched.
    pub fn new() -> (
        Self,
        mpsc::UnboundedReceiver<String>,
        mpsc::UnboundedReceiver<Incoming>,
    ) {
        let (out_tx, out_rx) = mpsc::unbounded_channel();
        let (in_tx, in_rx) = mpsc::unbounded_channel();

        let peer = Self {
            outbound: out_tx,
            incoming: in_tx,
            pending: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        };

        (peer, out_rx, in_rx)
    }

    /// Calls a method on the peer and waits for its result.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();

        self.pending
            .lock()
            .expect("rpc pending map poisoned")
            .insert(id, tx);

        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        if self.send(message).is_err() {
            self.pending
                .lock()
                .expect("rpc pending map poisoned")
                .remove(&id);
            return Err(RpcError::internal("extension host channel is closed"));
        }

        rx.await
            .unwrap_or_else(|_| Err(RpcError::internal("extension host closed before replying")))
    }

    /// Sends a notification. Nothing is awaited and nothing is answered.
    ///
    /// Only for methods the other side registers as event listeners. Sending a
    /// notification for a request method is silently dropped over there, so
    /// `Manager/*` calls must go through [`request`](Self::request).
    pub fn emit(&self, method: &str, params: Value) {
        let _ = self.send(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    /// Answers an inbound request.
    pub fn respond(&self, id: Value, result: Result<Value, RpcError>) {
        let message = match result {
            Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        let _ = self.send(message);
    }

    /// Feeds one decoded frame in.
    pub fn receive(&self, text: &str) {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            crate::say!("dropping unparseable frame from the extension host");
            return;
        };

        let has_result = message.get("result").is_some();
        let has_error = message.get("error").is_some();
        let method = message.get("method").and_then(Value::as_str);

        // A response carries an id and no method.
        if method.is_none() && (has_result || has_error) {
            let Some(id) = message.get("id").and_then(Value::as_u64) else {
                return;
            };

            let sender = self
                .pending
                .lock()
                .expect("rpc pending map poisoned")
                .remove(&id);

            if let Some(sender) = sender {
                let outcome = if has_error {
                    Err(serde_json::from_value(message["error"].clone())
                        .unwrap_or_else(|_| RpcError::internal(message["error"].to_string())))
                } else {
                    Ok(message["result"].clone())
                };
                let _ = sender.send(outcome);
            }
            return;
        }

        let Some(method) = method else { return };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        let work = match message.get("id") {
            Some(id) if !id.is_null() => Incoming::Request {
                id: id.clone(),
                method: method.to_string(),
                params,
            },
            _ => Incoming::Event {
                method: method.to_string(),
                params,
            },
        };

        let _ = self.incoming.send(work);
    }

    fn send(&self, message: Value) -> Result<(), ()> {
        self.outbound.send(message.to_string()).map_err(|_| ())
    }
}
