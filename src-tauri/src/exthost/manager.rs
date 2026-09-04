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
    /// The thing this command was run on, when it was run as an action.
    ///
    /// Absent for every ordinary launch, which is what somebody picking the
    /// command out of the root list is: there is nothing it was run on. Present
    /// only when the command was reached through the action panel of a file, a
    /// folder or whatever else it declared it acts on, and the worker hands it
    /// back through `@sill/api`.
    ///
    /// The domain object itself rather than a shape invented for the wire. It
    /// is four small strings and a kind, every one of which the extension needs
    /// to do anything useful, and a second type here would be a second answer
    /// to "what is a thing in Sill".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<crate::object::Object>,
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
            on: None,
            cwd: None,
        }
    }

    /// An argument object with a key for everything the command declared.
    ///
    /// Raycast collects these in the launcher's own bar before the command
    /// starts; nothing here collects them yet, so every one is absent. Absent
    /// is `""` rather than missing on purpose: an extension destructuring
    /// `props.arguments` and handing the result to a search is the ordinary
    /// shape, and `undefined` there is a crash where an empty string is an
    /// empty search.
    ///
    /// A dropdown with a first choice starts on it, because a dropdown with no
    /// selection is not a state Raycast's own bar can be in.
    pub fn blank_arguments(declared: &[crate::extension_install::Argument]) -> Value {
        let mut answer = serde_json::Map::new();

        for argument in declared {
            let value = match argument.kind.as_deref() {
                Some("dropdown") => argument
                    .data
                    .first()
                    .map(|choice| choice.value.clone())
                    .unwrap_or_else(|| json!("")),
                _ => json!(""),
            };
            answer.insert(argument.name.clone(), value);
        }

        Value::Object(answer)
    }
}

/// What one running command is costing, as the host measured it.
///
/// Bytes rather than megabytes, and a share of a core rather than a name for
/// how busy something is, because rounding and wording are the window's job
/// and a number that has already been rounded cannot be un-rounded.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Reading {
    pub session_id: String,
    /// Bytes of heap in use, or nothing when the worker did not answer.
    ///
    /// **Not answering is a reading.** The command most worth asking about is
    /// the one stuck in a loop, and a thread in a loop cannot answer anything,
    /// so a question with no deadline would hang on exactly the extension this
    /// exists to name.
    pub heap_bytes: Option<u64>,
    /// The cap a worker is stopped at, which is the host's own and not V8's.
    pub heap_limit_bytes: u64,
    /// How much of one processor core it used since the last time anybody
    /// asked, as a percentage.
    pub core_percent: f64,
    pub answering: bool,
}

/// What unloading a command answered with.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Unloaded {
    /// Whether there was anything there to unload.
    pub ok: bool,
    /// What it was holding at the end, when it was able to say.
    pub heap_bytes: Option<u64>,
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

    /// Ends one command, and says what it was holding on the way out.
    ///
    /// The memory figure comes back with the unload rather than being asked
    /// for separately, because this is the last moment it exists: the worker
    /// is gone by the time this returns. It is also the only figure that lets
    /// two extensions be compared after the fact, since a launcher usually has
    /// one command loaded and one figure is not a comparison.
    pub async fn unload(&self, session_id: &str) -> Result<Unloaded, RpcError> {
        let result = self
            .peer
            .request("Manager/unload", json!({ "session_id": session_id }))
            .await?;

        Ok(Unloaded {
            ok: result
                .get("ok")
                .and_then(Value::as_bool)
                .unwrap_or_default(),
            heap_bytes: result.get("heap_bytes").and_then(Value::as_u64),
        })
    }

    /// Tells one running command what it is allowed to reach now.
    ///
    /// The other half of the `capabilities` field on [`LoadOptions`], which
    /// arrives once and was the only time the worker ever heard about this.
    /// A permission taken away in Settings reached the file and the next
    /// launch, and never the command somebody was looking at.
    ///
    /// The whole list, not a difference. What an extension holds is one answer
    /// held in one place, and sending "this one has gone" would make the worker
    /// keep a second copy of the answer that has to agree with the first.
    pub async fn set_capabilities(
        &self,
        session_id: &str,
        capabilities: &[Capability],
    ) -> Result<bool, RpcError> {
        let result = self
            .peer
            .request(
                "Manager/setCapabilities",
                json!({ "session_id": session_id, "capabilities": capabilities }),
            )
            .await?;
        Ok(result.as_bool().unwrap_or(false))
    }

    /// What every loaded command is costing right now.
    ///
    /// One call for all of them rather than one per session, because the panel
    /// asking this wants a comparison and a comparison of readings taken
    /// seconds apart is not one.
    pub async fn diagnostics(&self) -> Result<Vec<Reading>, RpcError> {
        let result = self.peer.request("Manager/diagnostics", json!({})).await?;

        let workers = result
            .get("workers")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));

        serde_json::from_value(workers)
            .map_err(|err| RpcError::internal(format!("diagnostics were unreadable: {err}")))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(json: &str) -> Vec<crate::extension_install::Argument> {
        serde_json::from_str(json).expect("arguments parse")
    }

    /// A command that declares one used to get an object with nothing in it.
    ///
    /// `const { query } = props.arguments` then leaves `query` undefined, and
    /// an extension that hands it to a search throws in its first line, which
    /// reads as the extension being broken.
    #[test]
    fn every_declared_argument_has_a_key_before_the_command_starts() {
        let declared = arguments(
            r#"[
                { "name": "query", "type": "text" },
                { "name": "secret", "type": "password" }
            ]"#,
        );

        let blank = LoadOptions::blank_arguments(&declared);

        assert_eq!(blank["query"], "", "absent is an empty search, not a crash");
        assert_eq!(blank["secret"], "");
        assert_eq!(blank.as_object().expect("an object").len(), 2);
    }

    /// A dropdown with no selection is not a state Raycast's bar can be in.
    #[test]
    fn a_dropdown_starts_on_its_first_choice() {
        let declared = arguments(
            r#"[{
                "name": "scope",
                "type": "dropdown",
                "data": [{ "title": "All", "value": "all" }, { "title": "Mine", "value": "mine" }]
            }]"#,
        );

        assert_eq!(LoadOptions::blank_arguments(&declared)["scope"], "all");
    }

    #[test]
    fn a_command_declaring_nothing_still_gets_an_object() {
        // The host spreads this, and spreading anything but an object throws.
        assert!(LoadOptions::blank_arguments(&[]).is_object());
    }
}
