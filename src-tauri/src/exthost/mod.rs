//! Owns the extension host process and everything spoken over it.
//!
//! Layering matches `host/`: a Manager layer for process and session
//! lifecycle, and an API layer per session carried inside it as an opaque
//! payload. Each session therefore gets its own [`RpcPeer`] whose transport is
//! `Manager/messageExtension`, exactly as the worker has one whose transport is
//! `postMessage`.

pub mod api;
pub mod bridge;
pub mod framing;
pub mod grants;
pub mod manager;
pub mod permission;
pub mod rpc;
pub mod storage;

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::process::{Child, Command};
use tokio_util::codec::{FramedRead, FramedWrite};

pub use api::{ApiLayer, UiEvent};
pub use bridge::{Alert, AppInfo, Bridge, Clip};
pub use manager::{CommandEnv, CommandMode, LaunchType, LoadOptions, ManagerClient};
pub use rpc::{Incoming, RpcError, RpcPeer};
pub use storage::Storage;

/// A loaded command.
struct Session {
    /// Scopes storage and names the extension in logs.
    extension: String,
    /// The API-layer conversation with this extension.
    peer: RpcPeer,
    /// Whether anybody is looking at this, or it is off doing something.
    ///
    /// The difference decides what a dismissal may do. A view exists to be on
    /// screen and is worth nothing once the launcher is not, while a no-view
    /// command is work in flight that unloads itself when it finishes.
    mode: CommandMode,
}

/**
How long the host gets to answer a Manager call before it is treated as gone.

Not on every RPC: a render is the extension's own code and slow is not the same
as wedged. This is for the calls the launcher makes about the host itself,
where no answer means no answer.
*/
const ANSWER_WITHIN: std::time::Duration = std::time::Duration::from_secs(20);

pub struct ExtHost {
    manager: ManagerClient,
    api: Arc<ApiLayer>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// Kept so the host process is killed when this is dropped.
    _child: Child,
    /**
    False once the host's stream has ended, which is how a dead one is known.

    Nothing watched the child, so a host that crashed left its handle in place
    and every later launch got it back and failed with "channel is closed".
    Worse, each of those attempts touched the clock the idle watchdog reads, so
    the watchdog never considered it idle and never replaced it: **extensions
    stayed broken until five idle minutes or a restart of Sill.**

    The stream ending is the signal rather than waiting on the process,
    because the `Child` is owned here to be killed on drop and cannot also be
    awaited elsewhere. It closes when the process goes, which is the same
    moment for this purpose.
    */
    alive: Arc<AtomicBool>,
}

impl ExtHost {
    /// Spawns the bundled host and starts pumping both directions.
    ///
    /// `node_exe` is the interpreter and `host_js` the bundled artifact. The
    /// host refuses to run on a TTY, so stdio must be piped.
    ///
    /// Async purely to encode a requirement: `tokio::process` registers the
    /// child with the reactor, so this must run inside a Tokio runtime. It was
    /// previously sync, which compiled fine and then panicked at startup with
    /// "there is no reactor running" because Tauri's `setup` hook is not in
    /// one. Making it async moves that mistake to compile time.
    ///
    /// The API layer is built by the caller and outlives the host process.
    ///
    /// It owns `LocalStorage`, which is a file on disk and must survive an
    /// idle shutdown: an extension that saved a token before lunch has to
    /// still have it afterwards, and it would not if the store were created
    /// alongside each new Node process.
    pub async fn spawn(
        node_exe: &Path,
        host_js: &Path,
        api: Arc<ApiLayer>,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(node_exe)
            .arg(host_js)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let (peer, mut outbound, mut incoming) = RpcPeer::new();

        // Outbound: framed writes to the host.
        let mut writer = FramedWrite::new(stdin, framing::codec());
        tokio::spawn(async move {
            while let Some(text) = outbound.recv().await {
                if writer.send(text.into()).await.is_err() {
                    break;
                }
            }
        });

        let alive = Arc::new(AtomicBool::new(true));

        // Inbound: framed reads fed to the peer.
        let reader_peer = peer.clone();
        let reader_alive = alive.clone();
        let mut reader = FramedRead::new(stdout, framing::codec());
        tokio::spawn(async move {
            while let Some(frame) = reader.next().await {
                match frame {
                    Ok(bytes) => match std::str::from_utf8(&bytes) {
                        Ok(text) => reader_peer.receive(text),
                        Err(_) => crate::say!("extension host sent a non-UTF-8 frame"),
                    },
                    Err(err) => {
                        crate::say!("extension host framing error: {err}");
                        break;
                    }
                }
            }

            /*
             * The stream ended, so the host is gone.
             *
             * Marked before the pending requests are failed, so anything that
             * reacts to the failure by asking for the host again gets a fresh
             * one rather than this corpse.
             */
            reader_alive.store(false, std::sync::atomic::Ordering::Relaxed);
            crate::say!("extension host stopped answering");
            reader_peer.give_up_on_everything("the extension host stopped");
        });

        /*
         * The host's stderr is where extension console output and the host's
         * own warnings surface, so it must not be swallowed.
         *
         * Said rather than printed. `eprintln!` alone goes nowhere in a release
         * build, which is compiled without a console, so an extension's own
         * account of what it was doing was invisible in the only build anybody
         * runs. The whole point of carrying it this far is that somebody can
         * read it, and `sill.log` is where they would look.
         */
        tokio::spawn(async move {
            let mut lines = tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stderr));
            while let Ok(Some(line)) = lines.next_line().await {
                crate::say!("[host] {line}");
            }
        });

        let manager = ManagerClient::new(peer);
        let sessions: Arc<Mutex<HashMap<String, Session>>> = Arc::new(Mutex::new(HashMap::new()));

        // Manager-layer traffic coming up from the host.
        {
            let sessions = sessions.clone();
            let api = api.clone();
            tokio::spawn(async move {
                while let Some(work) = incoming.recv().await {
                    match work {
                        Incoming::Event { method, params }
                            if method == "Manager/extensionMessage" =>
                        {
                            let session_id = params
                                .get("session_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            let payload = params
                                .get("payload")
                                .and_then(Value::as_str)
                                .unwrap_or_default();

                            // Hand it to that session's own conversation.
                            let peer = sessions
                                .lock()
                                .expect("sessions poisoned")
                                .get(session_id)
                                .map(|s| s.peer.clone());

                            match peer {
                                Some(peer) => peer.receive(payload),
                                None => crate::say!(
                                    "message for unknown session {session_id}, dropped"
                                ),
                            }
                        }

                        Incoming::Event { method, params }
                            if method == "Manager/extensionCrash" =>
                        {
                            let session_id = params
                                .get("session_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            let reason = params
                                .get("reason")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown");
                            crate::say!("extension crashed in session {session_id}: {reason}");
                            // Told to the window as well as the log. Without
                            // this it sits on an empty view waiting for a
                            // first render that is never coming.
                            api.report_crash(session_id, reason);
                        }

                        Incoming::Event { method, .. } => {
                            crate::say!("unhandled manager event: {method}");
                        }

                        Incoming::Request { method, .. } => {
                            // The host never calls into Rust at the Manager
                            // layer; everything it wants goes through the API
                            // layer inside a payload.
                            crate::say!("unexpected manager request: {method}");
                        }
                    }
                }
            });
        }

        Ok(Self {
            manager,
            api,
            sessions,
            _child: child,
            alive,
        })
    }

    /// Loads a command and performs the ready handshake.
    ///
    /// The session is registered before `ready` is sent, which is the whole
    /// point of the handshake: the host buffers the extension's first messages
    /// until there is somewhere to deliver them.
    pub async fn load(&self, opts: &LoadOptions) -> Result<String, RpcError> {
        let session_id = self.manager.load(opts).await?;

        let (peer, mut outbound, mut incoming) = RpcPeer::new();

        self.sessions.lock().expect("sessions poisoned").insert(
            session_id.clone(),
            Session {
                extension: opts.extension_id.clone(),
                peer: peer.clone(),
                mode: opts.mode,
            },
        );

        // This session's outbound API traffic, wrapped for the Manager layer.
        {
            let manager = self.manager.clone();
            let session_id = session_id.clone();
            tokio::spawn(async move {
                while let Some(text) = outbound.recv().await {
                    if manager.message_extension(&session_id, &text).await.is_err() {
                        break;
                    }
                }
            });
        }

        // This session's inbound API calls.
        {
            let api = self.api.clone();
            let peer = peer.clone();
            let session_id = session_id.clone();
            let extension = opts.extension_id.clone();
            tokio::spawn(async move {
                while let Some(work) = incoming.recv().await {
                    match work {
                        Incoming::Request { id, method, params } => {
                            let result = api
                                .dispatch(&session_id, &extension, &method, &params)
                                .await;
                            peer.respond(id, result);
                        }
                        Incoming::Event { method, params } => {
                            // Notifications get dispatched too; the result is
                            // simply discarded. UI/render arrives this way.
                            if let Err(err) = api
                                .dispatch(&session_id, &extension, &method, &params)
                                .await
                            {
                                crate::say!("{method}: {err}");
                            }
                        }
                    }
                }
            });
        }

        self.manager.ready(&session_id).await?;
        Ok(session_id)
    }

    /// Fires an extension callback by the handler id carried in its props.
    pub async fn activate_handler(
        &self,
        session_id: &str,
        handler_id: &str,
        args: Value,
    ) -> Result<Value, RpcError> {
        let peer = self
            .sessions
            .lock()
            .expect("sessions poisoned")
            .get(session_id)
            .map(|s| s.peer.clone())
            .ok_or_else(|| RpcError::internal(format!("no such session: {session_id}")))?;

        peer.request(
            "EventCore/handlerActivated",
            json!({ "id": handler_id, "args": args }),
        )
        .await
    }

    /**
    Lets one command go, and gives up if the host will not answer.

    The deadline is the point. The idle watchdog unloads every session before
    shutting the host down, and it did that while holding the lock that every
    launch waits on. A host wedged in native code never answers, so the unload
    never returned, **the lock was never released, and every later extension
    launch deadlocked behind a shutdown that could not finish.**

    Generous next to what an unload actually does: the host gives the worker
    five seconds and then terminates it. Anything past this is not slow, it is
    not coming.
    */
    pub async fn unload(&self, session_id: &str) -> Result<bool, RpcError> {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id);

        match tokio::time::timeout(ANSWER_WITHIN, self.manager.unload(session_id)).await {
            Ok(answered) => answered,
            Err(_) => {
                crate::say!("the extension host did not answer an unload; giving up on it");
                Err(RpcError::internal("the extension host stopped answering"))
            }
        }
    }

    /// How many commands are loaded right now.
    ///
    /// The idle watchdog's "is anyone using this?" test. A loaded command is
    /// a view the user is looking at, so the host stays up for it however
    /// long it has been since anything was launched.
    /// Every session still loaded, so a watchdog can let them go.
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .expect("sessions poisoned")
            .keys()
            .cloned()
            .collect()
    }

    pub fn session_count(&self) -> usize {
        self.sessions.lock().expect("sessions poisoned").len()
    }

    /// Every session that is drawing something, so a dismissal can let it go.
    ///
    /// Views only, and that is the whole reason this exists next to
    /// [`Self::session_ids`]. A no-view command is doing a piece of work rather
    /// than showing a screen: the launcher going away is not a reason to kill
    /// it half way through, and it asks to be unloaded itself the moment it
    /// finishes.
    pub fn view_session_ids(&self) -> Vec<String> {
        views_in(&self.sessions.lock().expect("sessions poisoned"))
    }

    /// Name of the extension backing a session, if it is still loaded.
    /// The host process's id, for a test that needs to kill it.
    ///
    /// `None` once the child has been reaped, which for this type means never
    /// in practice: it is held until the host is dropped.
    pub fn child_id(&self) -> Option<u32> {
        self._child.id()
    }

    /// Whether the host is still answering.
    ///
    /// Asked before a stored host is handed out again. See the `alive` field.
    pub fn alive(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn extension_of(&self, session_id: &str) -> Option<String> {
        self.sessions
            .lock()
            .expect("sessions poisoned")
            .get(session_id)
            .map(|s| s.extension.clone())
    }
}

/// Which of these sessions are drawing something.
///
/// A free function over the map rather than a method, so it can be asked about
/// sessions somebody made up. Standing up an [`ExtHost`] costs a Node process,
/// which is a poor thing to need in order to ask which of two loaded commands
/// is a view.
fn views_in(sessions: &HashMap<String, Session>) -> Vec<String> {
    sessions
        .iter()
        .filter(|(_, session)| session.mode == CommandMode::View)
        .map(|(id, _)| id.clone())
        .collect()
}

#[cfg(test)]
mod letting_views_go {
    use super::*;

    fn session(mode: CommandMode) -> Session {
        let (peer, _outbound, _incoming) = RpcPeer::new();
        Session {
            extension: "fixture".to_string(),
            peer,
            mode,
        }
    }

    /// A dismissal closes the screens, not the work.
    ///
    /// The launcher going away means nobody can see a view, which is the whole
    /// reason a view is running. It means nothing about a no-view command:
    /// that is a piece of work somebody started, it usually finishes after the
    /// window has gone, and it unloads itself when it does. Closing those on a
    /// dismissal would kill every background command half way through.
    #[test]
    fn only_the_ones_somebody_was_looking_at() {
        let mut sessions = HashMap::new();
        sessions.insert("drawing".to_string(), session(CommandMode::View));
        sessions.insert("working".to_string(), session(CommandMode::NoView));

        assert_eq!(views_in(&sessions), vec!["drawing".to_string()]);
    }
}
