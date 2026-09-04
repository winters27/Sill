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
pub mod icons;
pub mod manager;
pub mod permission;
pub mod preferences;
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
pub use manager::{CommandEnv, CommandMode, LaunchType, LoadOptions, ManagerClient, Reading};
pub use rpc::{Incoming, RpcError, RpcPeer};
pub use storage::Storage;

/// A loaded command.
struct Session {
    /// Scopes storage and names the extension in logs.
    extension: String,
    /// Which of its commands this is, so a reading can say which one is heavy.
    ///
    /// An extension is not one program. "Emoji Search is using 47 MB" is only
    /// half an answer when the extension also has a command that lists nothing,
    /// and the half that is missing is the one somebody would act on.
    command: String,
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
                                None => {
                                    crate::say!("message for unknown session {session_id}, dropped")
                                }
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
                command: opts.command_name.clone(),
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
        let extension = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id)
            .map(|session| session.extension);

        let answered =
            match tokio::time::timeout(ANSWER_WITHIN, self.manager.unload(session_id)).await {
                Ok(answered) => answered?,
                Err(_) => {
                    crate::say!("the extension host did not answer an unload; giving up on it");
                    return Err(RpcError::internal("the extension host stopped answering"));
                }
            };

        /*
         * What it was holding at the end, written down before it is gone.
         *
         * The host asks the worker on its way out and hands the answer back
         * with the unload, so this costs nothing beyond a field on a reply
         * that was already being made. It is the figure that makes the
         * Extensions panel a comparison: a launcher has one command loaded at
         * a time, and somebody hunting for the expensive extension has closed
         * the other three by the time they come to look.
         */
        if let (Some(extension), Some(bytes)) = (extension, answered.heap_bytes) {
            self.api.timings().held_on_closing(&extension, bytes);
        }

        Ok(answered.ok)
    }

    /**
    What every loaded command is costing, right now.

    Asked of the host rather than sampled into a running total, and that is
    the whole design. A per-worker heap figure can only come from inside the
    worker, so a history of it would be a timer waking threads on a machine
    where nobody is looking, which is the cost this launcher exists not to
    pay. Somebody opening the Extensions panel is the reason to look.

    An empty answer is an ordinary one: no commands loaded, or a host that
    stopped answering. Neither is worth failing a panel over, and the panel
    has something true to say in both cases.
    */
    pub async fn worker_readings(&self) -> Vec<Running> {
        let readings = match tokio::time::timeout(ANSWER_WITHIN, self.manager.diagnostics()).await {
            Ok(Ok(readings)) => readings,
            Ok(Err(err)) => {
                crate::say!("could not read what the extensions are costing: {err}");
                return Vec::new();
            }
            Err(_) => {
                crate::say!("the extension host did not answer a diagnostics read");
                return Vec::new();
            }
        };

        joined(
            &self.sessions.lock().unwrap_or_else(|e| e.into_inner()),
            readings,
        )
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

    /**
    Tells every running command of one extension what it may reach now.

    **This is what makes a revoke mean something.** What an extension holds was
    handed to the worker once, at load, so taking a permission away wrote the
    file, satisfied the next launch, and left the command on screen using the
    thing somebody had just taken from it. The RPC side of the host never had
    this problem, because it asks `Permits` per call; the worker's own gate on
    `require` and on `fetch` is a set it was given, and a set nobody updates.

    Sent to every session of that extension rather than to one, because a
    person can have a view open and a background command running from the same
    extension and both hold the same permissions.

    Failures are logged and not returned. The caller is somebody changing a
    switch in Settings, the file is already written, and there is nothing they
    could do with "the host did not answer" except see an error for something
    that did work. A host that is not answering is one that is about to be
    replaced, and a fresh one reads the file.
    */
    pub async fn tell_running(&self, extension: &str, capabilities: &[crate::action::Capability]) {
        let sessions = sessions_of(
            &self.sessions.lock().unwrap_or_else(|e| e.into_inner()),
            extension,
        );

        for session in sessions {
            if let Err(err) = self.manager.set_capabilities(&session, capabilities).await {
                crate::say!("could not tell {extension} what it now holds: {err}");
            }
        }
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

/// One running command, and what it costs.
///
/// Names as well as numbers, because the numbers are useless on their own:
/// the question this answers is "which one", and a session id is not an
/// answer to that.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub session: String,
    pub extension: String,
    pub command: String,
    /// Bytes of heap, or nothing when the worker did not answer in time.
    pub heap_bytes: Option<u64>,
    pub heap_limit_bytes: u64,
    /// How much of one processor core it used since the last reading.
    pub core_percent: f64,
    /// Whether it answered at all. A command in a loop cannot.
    pub answering: bool,
}

/// Puts names to the host's readings, and drops the ones nobody has any more.
///
/// A free function over the map for the reason [`views_in`] is one, and with
/// a second reason of its own: **a session can be killed between the question
/// and the answer**, and revoking a permission does exactly that. A reading
/// for a session Rust no longer holds is a row about a command that has gone,
/// and drawing it would leave a dead extension on the panel until something
/// else redrew it. So the map decides what exists and the readings only say
/// what it costs.
fn joined(sessions: &HashMap<String, Session>, readings: Vec<Reading>) -> Vec<Running> {
    readings
        .into_iter()
        .filter_map(|reading| {
            let session = sessions.get(&reading.session_id)?;

            Some(Running {
                session: reading.session_id,
                extension: session.extension.clone(),
                command: session.command.clone(),
                heap_bytes: reading.heap_bytes,
                heap_limit_bytes: reading.heap_limit_bytes,
                core_percent: reading.core_percent,
                answering: reading.answering,
            })
        })
        .collect()
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

/// Which of these sessions belong to one extension.
///
/// A free function over the map for the reason [`views_in`] is one: standing up
/// an [`ExtHost`] costs a Node process, and "which of these three sessions is
/// the extension somebody just revoked" is a question about a map.
///
/// Every mode, deliberately. A revoke has to reach a no-view command as well as
/// a view: the background one is the one that is off doing something with the
/// permission right now, and it is the one nobody is looking at.
fn sessions_of(sessions: &HashMap<String, Session>, extension: &str) -> Vec<String> {
    sessions
        .iter()
        .filter(|(_, session)| session.extension == extension)
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
            command: "only".to_string(),
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

#[cfg(test)]
mod reaching_what_is_running {
    use super::*;

    fn session(extension: &str, mode: CommandMode) -> Session {
        let (peer, _outbound, _incoming) = RpcPeer::new();
        Session {
            extension: extension.to_string(),
            command: "only".to_string(),
            peer,
            mode,
        }
    }

    /// A revoke has to find the commands of the extension it revoked, and only
    /// those.
    ///
    /// Telling the wrong session is worse than telling none: the list sent is
    /// the whole answer, so another extension's worker would be handed a set of
    /// permissions belonging to something else and believe it.
    #[test]
    fn only_the_extension_that_was_revoked() {
        let mut sessions = HashMap::new();
        sessions.insert("a".to_string(), session("clipboard", CommandMode::View));
        sessions.insert("b".to_string(), session("translate", CommandMode::View));

        assert_eq!(sessions_of(&sessions, "clipboard"), vec!["a".to_string()]);
        assert_eq!(sessions_of(&sessions, "translate"), vec!["b".to_string()]);
        assert!(sessions_of(&sessions, "never-installed").is_empty());
    }

    /// Both of them, and the one nobody is looking at especially.
    ///
    /// A no-view command is work in flight. It is the session most likely to be
    /// part way through using the permission at the moment it is taken away, and
    /// it is the one a "close the screens" rule would skip.
    #[test]
    fn every_command_it_has_running_including_the_invisible_one() {
        let mut sessions = HashMap::new();
        sessions.insert("watching".to_string(), session("notes", CommandMode::View));
        sessions.insert("working".to_string(), session("notes", CommandMode::NoView));

        let mut found = sessions_of(&sessions, "notes");
        found.sort();

        assert_eq!(found, vec!["watching".to_string(), "working".to_string()]);
    }
}

#[cfg(test)]
mod what_is_running_costs {
    use super::*;

    fn session(extension: &str, command: &str) -> Session {
        let (peer, _outbound, _incoming) = RpcPeer::new();
        Session {
            extension: extension.to_string(),
            command: command.to_string(),
            peer,
            mode: CommandMode::View,
        }
    }

    fn reading(id: &str) -> Reading {
        Reading {
            session_id: id.to_string(),
            heap_bytes: Some(63 * 1024 * 1024),
            heap_limit_bytes: 512 * 1024 * 1024,
            core_percent: 10.0,
            answering: true,
        }
    }

    /// A number is only worth having with a name on it.
    #[test]
    fn a_reading_is_named_from_the_session_it_belongs_to() {
        let mut sessions = HashMap::new();
        sessions.insert("a".to_string(), session("emoji", "Search Emoji"));

        let named = joined(&sessions, vec![reading("a")]);

        assert_eq!(named.len(), 1);
        assert_eq!(named[0].extension, "emoji");
        assert_eq!(named[0].command, "Search Emoji");
        assert_eq!(named[0].heap_bytes, Some(63 * 1024 * 1024));
    }

    /// A command killed between the question and the answer leaves no row.
    ///
    /// Revoking a permission reaches a running worker and can end it, which
    /// this project made true deliberately. So a reading can describe a
    /// command that no longer exists by the time it arrives, and drawing that
    /// would leave a dead extension on the panel reporting memory it is not
    /// using until something else redrew the screen.
    #[test]
    fn a_command_that_has_gone_is_not_drawn() {
        let sessions = HashMap::new();

        assert!(
            joined(&sessions, vec![reading("killed")]).is_empty(),
            "a reading outlived its session and was drawn anyway"
        );
    }
}
