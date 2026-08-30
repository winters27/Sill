//! Starting, finding and retiring the extension host process.
//!
//! Its own module because the lifecycle is a subject in itself: where the
//! bundle lives, when Node is worth starting, and when it should be allowed
//! to go away again.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use crate::exthost::{ExtHost, UiEvent};
use crate::state::{data_dir, HostState};

/// The repository root, as it was when this binary was compiled.
///
/// Only meaningful on the machine that built it, which is exactly what makes
/// it a development path and nothing else. An installed copy is somewhere
/// else entirely and its `CARGO_MANIFEST_DIR` points at a directory that
/// almost certainly does not exist.
pub(crate) fn dev_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

/// Where the extensions Sill can run are listed.
///
/// Two places, and both count. Installed extensions live under the user's data
/// directory, because they are installed rather than shipped. Extensions built
/// from the repository live in the working tree. A developer has both, and
/// preferring one would make the other invisible for no reason.
pub(crate) fn index_paths(app: &AppHandle) -> Vec<PathBuf> {
    let mut paths = vec![data_dir(app).join("extensions").join("index.json")];

    let dev = dev_root()
        .join("extensions")
        .join("build")
        .join("index.json");
    if dev.exists() {
        paths.push(dev);
    }

    paths
}

/// What to say when Node is not on the machine.
///
/// Named as a sentence somebody can act on. Extensions are Node programs and
/// Sill runs them in a Node process, which is a requirement nothing in the
/// application had ever mentioned: the first sign of it was a spawn failing
/// with "the system cannot find the file specified", naming a file the person
/// reading it had never heard of.
pub(crate) const NO_NODE: &str =
    "Extensions need Node.js, which is not installed. Get it from nodejs.org, \
     or run: winget install OpenJS.NodeJS.LTS";

/// The Node interpreter, if this machine has one.
///
/// `PATH` first, because that is what a developer's shell would find and what
/// version managers arrange. Then the usual install locations, because a
/// desktop application does not inherit the shell's `PATH` on Windows in the
/// way a terminal does, and a Node installed while Sill was running is not in
/// the environment Sill started with.
pub(crate) fn node_exe() -> Option<PathBuf> {
    if which("node").is_some() {
        return Some(PathBuf::from("node"));
    }

    [
        r"C:\Program Files\nodejs\node.exe",
        r"C:\Program Files (x86)\nodejs\node.exe",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|candidate| candidate.is_file())
}

/// Whether a bare program name resolves on this machine.
///
/// Asked by running it rather than by walking `PATH` by hand: `PATHEXT`,
/// shims, and the store aliases all make the second one wrong in ways that
/// only show up on somebody else's computer.
fn which(program: &str) -> Option<()> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(program);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window. Without this every check flashes one up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.status().ok().filter(|it| it.success()).map(|_| ())
}

/// Where the extension host bundle is.
///
/// This used to be the development path and only the development path, which
/// meant **an installed Sill could never run an extension**: the resolved
/// location was a directory on whichever machine compiled the binary.
///
/// Three candidates, in order, and each is there for a reason:
///
/// 1. `SILL_HOST_JS`, so a rebuilt host can be pointed at without reinstalling.
/// 2. The bundled resource, which is what an installed copy uses.
/// 3. The repository's build output, which is what `cargo run` uses.
///
/// Only a candidate that exists is returned. Handing back a path that is not
/// there produces "could not spawn node" several steps later, which says
/// nothing about which of three places was looked at.
pub(crate) fn host_js(app: &AppHandle) -> PathBuf {
    let bundled = app
        .path()
        .resolve("host/host.js", tauri::path::BaseDirectory::Resource)
        .unwrap_or_else(|_| PathBuf::from("host/host.js"));

    let candidates = [
        (
            "SILL_HOST_JS",
            std::env::var_os("SILL_HOST_JS").map(PathBuf::from),
        ),
        ("bundled resource", Some(bundled.clone())),
        (
            "development build",
            Some(dev_root().join("host").join("dist").join("host.js")),
        ),
    ];

    for (source, candidate) in candidates {
        let Some(path) = candidate else { continue };
        if path.exists() {
            println!("[sill] extension host: {} ({source})", path.display());
            return path;
        }
    }

    // Nothing found. The resource location is the one an installed copy is
    // missing, so it is the useful thing to name when a launch fails.
    crate::say!(
        "no extension host found. Expected it at {} or built at host/dist/host.js",
        bundled.display()
    );
    bundled
}

/// How long the host may sit unused before it is shut down.
///
/// Far shorter than the whisper server's half hour, because the cost of being
/// wrong is different: reloading a whisper model is seconds off a cold disk,
/// while respawning Node is a fraction of one.
pub(crate) const HOST_IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

pub(crate) const HOST_IDLE_CHECK: std::time::Duration = std::time::Duration::from_secs(60);

/// The running host, or nothing if it is not up.
///
/// Never starts one. Used by the calls that only make sense against a session
/// that already exists, so unloading a command from a host that has since
/// been shut down does not resurrect Node to be told there is nothing to do.
pub(crate) async fn running_host(state: &HostState) -> Option<Arc<ExtHost>> {
    state.inner.lock().await.clone()
}

/// The host, started if it is not already running.
///
/// The lock is held across the spawn on purpose: two commands launched at
/// once must wait for one host rather than race and start two.
pub(crate) async fn host_of(state: &HostState) -> Result<Arc<ExtHost>, String> {
    *state.last_used.lock().expect("host clock poisoned") = std::time::Instant::now();

    let mut slot = state.inner.lock().await;

    if let Some(host) = slot.as_ref() {
        return Ok(host.clone());
    }

    if !state.host_js.exists() {
        return Err(format!(
            "extension host bundle missing at {}. Run: npm --prefix host run build",
            state.host_js.display()
        ));
    }

    // Asked before spawning, so the answer names the missing thing rather
    // than the symptom. Failing at the spawn gives "The system cannot find the
    // file specified", which is true of an interpreter nobody knew was needed.
    let Some(node) = node_exe() else {
        return Err(NO_NODE.to_string());
    };

    let host = ExtHost::spawn(&node, &state.host_js, state.api.clone())
        .await
        .map_err(|err| format!("could not start the extension host: {err}"))?;

    let host = Arc::new(host);
    *slot = Some(host.clone());
    drop(slot);

    crate::say!("extension host started");
    start_host_watchdog(state.clone());

    Ok(host)
}

/// Shuts the host down once nothing has used it for a while.
///
/// Started when the host is spawned and returns when it fires, so a machine
/// that never opens an extension never runs this timer at all. A permanent
/// one-minute tick waiting for a process that is usually not there is exactly
/// the "why are we waking up?" this is meant to avoid.
pub(crate) fn start_host_watchdog(state: HostState) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(HOST_IDLE_CHECK).await;

            let idle = state
                .last_used
                .lock()
                .expect("host clock poisoned")
                .elapsed();

            let mut slot = state.inner.lock().await;

            // Somebody else already shut it down. Nothing left to watch.
            let Some(host) = slot.as_ref() else { return };

            // A loaded command is a reason to stay up however long it has sat
            // there: the user is looking at it.
            if idle < HOST_IDLE_TIMEOUT || host.session_count() > 0 {
                continue;
            }

            crate::say!(
                "extension host idle for {}s; shutting it down",
                idle.as_secs()
            );
            // Dropping the last handle kills the child: `ExtHost` holds it
            // with `kill_on_drop`.
            slot.take();
            return;
        }
    });
}

/// Forwards everything the extension asks for to the window.
///
/// One channel to one event name keeps ordering intact: a toast raised during
/// a render must not overtake the render that caused it.
pub(crate) fn forward_events(app: AppHandle, mut events: mpsc::UnboundedReceiver<UiEvent>) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = events.recv().await {
            if let Err(err) = app.emit("sill://ui", &event) {
                crate::say!("could not forward a UI event: {err}");
            }
        }
    });
}
