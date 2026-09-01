//! The Manager layer: spawning the extension host and managing sessions.
//!
//! Rust is the client here. It asks the host to load and unload commands, and
//! receives each extension's API traffic as an opaque payload to be handed to
//! the API layer.

use serde::{Deserialize, Serialize};

use crate::action::Capability;
use serde_json::{json, Value};

use super::rpc::{RpcError, RpcPeer};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommandMode {
    View,
    NoView,
}

impl CommandMode {
    /// The mode a manifest's `mode` string means, or nothing when Sill has no
    /// way to run it.
    ///
    /// **This type is the answer to "can Sill run this command", and this is
    /// the only place that question is decided.** Raycast also has `menu-bar`,
    /// which is a status item next to the clock and has nowhere to live in a
    /// launcher. The store asks this before offering an extension, so a mode
    /// Raycast invents later is reported as unrunnable in the store instead of
    /// installing and then quietly doing nothing.
    ///
    /// `None` rather than a default, deliberately. A match over modes that
    /// falls through to `View` for anything it does not recognise is the shape
    /// that has silently swallowed a new case five times in this codebase.
    pub fn from_manifest(mode: &str) -> Option<Self> {
        match mode {
            "view" => Some(Self::View),
            "no-view" => Some(Self::NoView),
            _ => None,
        }
    }
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
    /// What this extension has been allowed to reach.
    ///
    /// Was a struct of three booleans that nothing set and nothing read. Now
    /// the same `Capability` an action declares, taken from what somebody has
    /// actually granted, and the worker refuses `fs`, `net` and
    /// `child_process` on the strength of it.
    ///
    /// Empty is the safe default and stays the default: a caller that forgets
    /// to fill it in gets an extension that can draw and nothing else.
    pub capabilities: Vec<Capability>,
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
        Self::with_preferences(entrypoint, extension, command, mode, json!({}))
    }

    /// The same, carrying what `getPreferenceValues()` should answer with.
    ///
    /// Was always `{}`, so every extension ran as though the user had cleared
    /// every setting, including the ones its manifest gives a default for.
    /// That surfaces inside the extension as an undefined where it expected a
    /// string, which reads as the extension being broken.
    pub fn with_preferences(
        entrypoint: impl Into<String>,
        extension: &str,
        command: &str,
        mode: CommandMode,
        preferences: Value,
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
            // An object, always. The host spreads this, and spreading null
            // throws where an empty object is simply empty.
            preferences: if preferences.is_object() {
                preferences
            } else {
                json!({})
            },
            arguments: json!({}),
            launch_type: LaunchType::User,
            capabilities: Vec::new(),
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
