//! The Manager layer: spawning the extension host and managing sessions.
//!
//! Rust is the client here. It asks the host to load and unload commands, and
//! receives each extension's API traffic as an opaque payload to be handed to
//! the API layer.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::rpc::{RpcError, RpcPeer};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommandMode {
    View,
    NoView,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommandEnv {
    Development,
    Production,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LaunchType {
    User,
    Background,
    CommandLine,
}

/// What the host is allowed to ask of this build.
///
/// An extension that needs a capability we do not have should be told so up
/// front rather than failing at the call site.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub browser_extension: bool,
    pub window_management: bool,
    pub file_search: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadOptions {
    pub mode: CommandMode,
    pub env: CommandEnv,
    pub entrypoint: String,
    pub extension_id: String,
    pub extension_name: String,
    pub command_name: String,
    pub owner_or_author_name: String,
    pub is_raycast: bool,
    pub assets_path: String,
    pub support_path: String,
    pub preferences: Value,
    pub arguments: Value,
    pub launch_type: LaunchType,
    pub capabilities: Capabilities,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

impl LoadOptions {
    /// A view command from a local extension directory.
    pub fn view(entrypoint: impl Into<String>, extension: &str, command: &str) -> Self {
        Self::for_command(entrypoint, extension, command, CommandMode::View)
    }

    /// A command whose mode comes from its manifest.
    ///
    /// Forcing every command to View is wrong: a `no-view` command never
    /// renders, and running one as a view leaves the UI waiting for a tree
    /// that will never arrive.
    pub fn for_command(
        entrypoint: impl Into<String>,
        extension: &str,
        command: &str,
        mode: CommandMode,
    ) -> Self {
        Self {
            mode,
            env: CommandEnv::Development,
            entrypoint: entrypoint.into(),
            extension_id: extension.to_string(),
            extension_name: extension.to_string(),
            command_name: command.to_string(),
            owner_or_author_name: String::new(),
            is_raycast: true,
            assets_path: String::new(),
            support_path: String::new(),
            preferences: json!({}),
            arguments: json!({}),
            launch_type: LaunchType::User,
            capabilities: Capabilities::default(),
            cwd: None,
        }
    }
}

/// Client half of the Manager layer.
#[derive(Clone)]
pub struct ManagerClient {
    peer: RpcPeer,
}

impl ManagerClient {
    pub fn new(peer: RpcPeer) -> Self {
        Self { peer }
    }

    /// Loads a command and returns its session id.
    ///
    /// The host starts the extension immediately, so its first messages can
    /// arrive before this returns. They are buffered on the host side until
    /// [`ready`](Self::ready) is called.
    pub async fn load(&self, opts: &LoadOptions) -> Result<String, RpcError> {
        let result = self
            .peer
            .request("Manager/load", json!({ "opts": opts }))
            .await?;

        result
            .get("session_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| RpcError::internal("load returned no session_id"))
    }

    /// Releases the host's buffered messages for a session.
    ///
    /// Call this only once the session id is stored, otherwise the first
    /// render can arrive with nowhere to go.
    pub async fn ready(&self, session_id: &str) -> Result<bool, RpcError> {
        let result = self
            .peer
            .request("Manager/ready", json!({ "session_id": session_id }))
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    pub async fn unload(&self, session_id: &str) -> Result<bool, RpcError> {
        let result = self
            .peer
            .request("Manager/unload", json!({ "session_id": session_id }))
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    /// Sends one API-layer message to a running extension.
    ///
    /// This is a request, not a notification. The host routes request methods
    /// and event methods through different tables, so a notification here is
    /// dropped without an error.
    pub async fn message_extension(
        &self,
        session_id: &str,
        payload: &str,
    ) -> Result<bool, RpcError> {
        let result = self
            .peer
            .request(
                "Manager/messageExtension",
                json!({ "session_id": session_id, "payload": payload }),
            )
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }
}
