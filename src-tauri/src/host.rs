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
    // Named rather than assembled, because the installer writes to the same
    // place and two spellings of one path is the shape that disagrees.
    let mut paths = vec![crate::store::index_file(&crate::store::extensions_home(
        &data_dir(app),
    ))];

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
/// The Node interpreter, remembered once it has been found.
///
/// Takes the slot that holds the answer rather than reaching for a global one,
/// and takes the slot rather than the whole `HostState`, because that is the
/// dependency it actually has. A test can then hand it an empty one instead of
/// standing up an extension host to ask a question about a file path.
pub fn node_exe(remembered: &std::sync::Mutex<Option<PathBuf>>) -> Option<PathBuf> {
    if let Some(known) = remembered.lock().ok().and_then(|held| held.clone()) {
        return Some(known);
    }

    let found = look_for_node();

    if let Some(path) = found.as_ref() {
        if let Ok(mut held) = remembered.lock() {
            *held = Some(path.clone());
        }
    }

    found
}

fn look_for_node() -> Option<PathBuf> {
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
            let path = without_extended_prefix(path);
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

/// Windows' extended-length prefix, taken off before Node is handed the path.
///
/// `AppHandle::path().resolve` canonicalises, and on Windows canonicalising
/// yields `\\?\C:\...`. Node cannot use one of those as its entry script: it
/// reads the leading `\\` as a UNC root, takes `?` for the server and `C:` for
/// the share, and `resolveMainPath` then calls `realpath` on `C:` and dies
/// with `EISDIR: illegal operation on a directory, lstat 'C:'`.
///
/// That kills the host before a line of it has run, so **every** extension
/// fails to start, and the message names neither the host nor the extension
/// nor the path it choked on. It reads as one extension being broken.
///
/// Only the plain disk form is unwrapped. `\\?\UNC\server\share` shortens to
/// `\\server\share`, which is a different rewrite and a second thing to be
/// wrong about, so it is left as it is.
fn without_extended_prefix(path: PathBuf) -> PathBuf {
    let shortened = path.to_str().and_then(|text| {
        let rest = text.strip_prefix(r"\\?\")?;
        (!rest.starts_with(r"UNC\")).then(|| PathBuf::from(rest))
    });

    shortened.unwrap_or(path)
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
///
/// The handle is here for the watchdog this starts, which has to be able to
/// ask whether anybody is looking at Sill before it lets a command go.
pub(crate) async fn host_of(app: &AppHandle, state: &HostState) -> Result<Arc<ExtHost>, String> {
    // Recovered rather than propagated: a poisoned clock must not stop
    // every later extension launch. The worst it holds is a stale instant,
    // which the idle watchdog corrects on its next pass.
    *state.last_used.lock().unwrap_or_else(|e| e.into_inner()) = std::time::Instant::now();

    let mut slot = state.inner.lock().await;

    /*
     * A stored host is only worth handing back if it is still answering.
     *
     * A host that crashed left its handle here and every later launch got it
     * back and failed with "channel is closed". The idle watchdog could not
     * clear it either, because asking for the host is what marks it as used,
     * so a dead host looked permanently busy. Extensions stayed broken until
     * a restart.
     */
    if let Some(host) = slot.as_ref() {
        if host.alive() {
            return Ok(host.clone());
        }

        crate::say!("the extension host had stopped; starting another");
        *slot = None;
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
    let Some(node) = node_exe(&state.node) else {
        return Err(NO_NODE.to_string());
    };

    let host = ExtHost::spawn(&node, &state.host_js, state.api.clone())
        .await
        .map_err(|err| format!("could not start the extension host: {err}"))?;

    let host = Arc::new(host);
    *slot = Some(host.clone());
    drop(slot);

    crate::say!("extension host started");
    start_host_watchdog(app.clone(), state.clone());

    Ok(host)
}

/// Which extension a session belongs to, if the host still has it.
///
/// `None` when nothing is running under that id, which is the answer that
/// matters: an action claiming to come from a session nobody has is not an
/// extension's action.
pub(crate) async fn extension_of(state: &HostState, session: &str) -> Option<String> {
    running_host(state).await?.extension_of(session)
}

/// What one pass of the idle watchdog concludes.
#[derive(Debug, PartialEq, Eq)]
enum Idle {
    /// Not long enough yet.
    TooSoon,
    /// Long enough, but Sill is on screen with a command loaded.
    Watched,
    /// Nothing is looking and nothing has been used. Let it all go.
    LetGo,
}

/// Whether an idle pass may take the host away.
///
/// Separated from the loop so the rule can be read and tested without a
/// window, a Node process or a five minute wait. What it encodes is that
/// **elapsed time alone is not evidence that nobody is there**: the clock only
/// moves when the host is asked for, and somebody five minutes into an
/// extension's form has not asked for anything since it opened. Every
/// keystroke of that goes into the window, not across the boundary, so a
/// watchdog reading the clock alone concludes the user has gone and closes the
/// form they are typing into.
fn idle_pass(idle: std::time::Duration, on_screen: bool, sessions: usize) -> Idle {
    if idle < HOST_IDLE_TIMEOUT {
        return Idle::TooSoon;
    }

    // On screen with nothing loaded is not somebody watching an extension, so
    // the host still goes. It is the pairing that means "in use".
    if on_screen && sessions > 0 {
        return Idle::Watched;
    }

    Idle::LetGo
}

/// Lets go of every command view that is loaded, and says so to the window.
///
/// Two callers mean the same thing by this: a launcher put away long enough to
/// sleep, and a host nothing has touched for five minutes. In both, the view is
/// one nobody can see.
///
/// **Telling the window is half the job.** It is holding a tree of a worker
/// that no longer exists, and a view left on screen after its session has gone
/// looks exactly like a working one until something is pressed, at which point
/// every action fails with "no such session".
///
/// Emitted straight at the window rather than through the channel that carries
/// renders. Ordering is what that channel is for, and a session being closed
/// has nothing further to render; the caller that matters here suspends the
/// renderer immediately afterwards and needs the message posted before it does.
async fn close_views(app: &AppHandle, host: &ExtHost, why: &str) -> usize {
    let views = host.view_session_ids();

    for session in &views {
        // Said before the unload rather than after it, because the unload is a
        // round trip to Node and the worker gets five seconds to unmount. That
        // is long enough for a caller waiting on this to give up and suspend
        // the renderer first, which is the one outcome this ordering is for.
        // The window's own way out unloads the session as well, so a message
        // arriving before the worker is gone costs nothing.
        let _ = app.emit(
            "sill://ui",
            UiEvent::Closed {
                session: session.clone(),
                reason: why.to_string(),
            },
        );

        let _ = host.unload(session).await;
    }

    views.len()
}

/// How long a dismissal waits for the views to go before the renderer sleeps.
///
/// A deadline rather than a promise. Unloading talks to Node, and a host that
/// has wedged answers nothing; the renderer going to sleep is worth more than
/// the window being told tidily, so the wait gives up and lets the sleep
/// happen.
const RELEASE_WITHIN: std::time::Duration = std::time::Duration::from_secs(5);

/// Lets go of the views once the launcher has been put away for good.
///
/// Called from the sleep timer, at the moment a dismissal has stood long
/// enough that the renderer is about to be suspended. That is the honest
/// signal for this: a view exists to be looked at, and Sill has just concluded
/// that nobody is going to.
///
/// Blocking, deliberately, on a thread that is only sleeping anyway. The
/// caller suspends the renderer next, and a suspended renderer is a poor place
/// to send an event: at best the message waits until the window is summoned,
/// and at worst evaluating it is what wakes the renderer back up, which would
/// undo the saving the sleep exists for.
pub(crate) fn release_views(app: &AppHandle) {
    let (done, finished) = std::sync::mpsc::channel();
    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        // `try_state`, because a window can be put away before the setup hook
        // has managed anything: Tauri creates them all first.
        if let Some(state) = app
            .try_state::<HostState>()
            .map(|held| held.inner().clone())
        {
            if let Some(host) = running_host(&state).await {
                let closed = close_views(&app, &host, "the launcher was put away").await;
                if closed > 0 {
                    crate::say!("let go of {closed} extension view(s): the launcher was put away");
                }
            }
        }

        let _ = done.send(());
    });

    let _ = finished.recv_timeout(RELEASE_WITHIN);
}

/// Shuts the host down once nothing has used it for a while.
///
/// Started when the host is spawned and returns when it fires, so a machine
/// that never opens an extension never runs this timer at all. A permanent
/// one-minute tick waiting for a process that is usually not there is exactly
/// the "why are we waking up?" this is meant to avoid.
pub(crate) fn start_host_watchdog(app: AppHandle, state: HostState) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(HOST_IDLE_CHECK).await;

            let idle = state
                .last_used
                .lock()
                .expect("host clock poisoned")
                .elapsed();

            let slot = state.inner.lock().await;

            // Somebody else already shut it down. Nothing left to watch.
            let Some(host) = slot.as_ref() else { return };

            /*
             * The lock is dropped before any of this is awaited.
             *
             * Unloading talks to the host, and a host that has wedged does not
             * reply. Holding `state.inner` across that meant every later
             * launch waited behind a shutdown that could never finish. The
             * handle is cloned out, the lock released, and the slot cleared
             * afterwards; `unload` has a deadline of its own as well, because
             * one belt is not enough when the failure is a hang.
             */
            let host = host.clone();
            drop(slot);

            match idle_pass(
                idle,
                crate::summon::anything_visible(&app),
                host.session_count(),
            ) {
                Idle::TooSoon => continue,

                Idle::Watched => {
                    /*
                     * Counted as a use, so the clock starts again from here.
                     *
                     * Otherwise the moment the window is put away the host is
                     * already five minutes idle and goes at the next tick,
                     * which spends a Node start on somebody who dismissed the
                     * launcher and came back. What "idle" is supposed to mean
                     * is five minutes since anybody wanted it.
                     */
                    *state
                        .last_used
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                        std::time::Instant::now();
                    continue;
                }

                Idle::LetGo => {}
            }

            /*
             * A session that sat through the whole timeout is let go.
             *
             * This used to read "a loaded command is a reason to stay up
             * however long it has sat there: the user is looking at it". They
             * are not, and this now asks rather than assuming: a window that is
             * on screen with a command loaded is answered above, and everything
             * that gets here is a view nobody can see.
             *
             * The window did not clean up either: `unloadExtension` is called
             * when Escape goes back to the root list and on no other path out.
             * Every other way of leaving left a worker running and, because a
             * live session vetoed the shutdown, kept the whole Node host
             * resident for the rest of the session. An idle pass that any
             * window can veto by forgetting one call is not an idle pass.
             *
             * Unloaded here rather than fixed only in the window, because this
             * is the layer that can see the truth: the window knows what it
             * meant to do, and this knows what is actually still running.
             */
            let closed = close_views(
                &app,
                &host,
                &format!(
                    "nothing had happened in it for {} minutes",
                    HOST_IDLE_TIMEOUT.as_secs() / 60
                ),
            )
            .await;

            if closed > 0 {
                crate::say!(
                    "let go of {closed} extension view(s): idle {}s",
                    idle.as_secs()
                );
            }

            for session in host.session_ids() {
                crate::say!(
                    "unloading extension session {session}: idle {}s",
                    idle.as_secs()
                );
                let _ = host.unload(&session).await;
            }

            crate::say!(
                "extension host idle for {}s; shutting it down",
                idle.as_secs()
            );
            // Dropping the last handle kills the child: `ExtHost` holds it
            // with `kill_on_drop`.
            drop(host);
            state.inner.lock().await.take();
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

#[cfg(test)]
mod idle_sweep {
    use super::{idle_pass, Idle, HOST_IDLE_TIMEOUT};
    use std::time::Duration;

    /// The bug this rule exists for.
    ///
    /// The idle clock only moves when something asks for the host, and filling
    /// in an extension's form asks for nothing: every keystroke goes into the
    /// window. So somebody five minutes into a form looked exactly like
    /// somebody who had walked away, and the sweep closed the form under them.
    #[test]
    fn a_view_on_screen_is_not_an_idle_host() {
        assert_eq!(
            idle_pass(HOST_IDLE_TIMEOUT + Duration::from_secs(60), true, 1),
            Idle::Watched
        );
    }

    /// And the same host, with the launcher put away, is let go.
    ///
    /// The other half. A rule that never lets anything go would keep Node
    /// resident for the rest of the run, which is what this whole sweep exists
    /// to prevent.
    #[test]
    fn the_same_view_with_nobody_looking_goes() {
        assert_eq!(
            idle_pass(HOST_IDLE_TIMEOUT + Duration::from_secs(60), false, 1),
            Idle::LetGo
        );
    }

    /// Being on screen is not on its own a reason to keep a Node process.
    ///
    /// The launcher can sit open for an hour with nothing loaded. Nothing is
    /// being pulled out from under anybody there, and holding the host would
    /// be residency bought with nothing.
    #[test]
    fn a_window_with_nothing_loaded_is_not_using_the_host() {
        assert_eq!(
            idle_pass(HOST_IDLE_TIMEOUT + Duration::from_secs(60), true, 0),
            Idle::LetGo
        );
    }

    #[test]
    fn nothing_happens_before_the_timeout() {
        assert_eq!(
            idle_pass(HOST_IDLE_TIMEOUT / 2, false, 1),
            Idle::TooSoon,
            "a host was taken away before it had been idle for the timeout"
        );
    }
}

/// Looking for Node runs a process, so it is looked for once.
///
/// `which` runs `node --version` and waits for it, because `PATHEXT`, shims
/// and store aliases make walking `PATH` by hand wrong in ways that only show
/// up on somebody else's computer. That was paid on every cold activation,
/// **under the host lock**, and on every store readiness check.
///
/// This was one wall-clock test, and it had two of the diseases the audit
/// names. It compared the second call's elapsed time against a quarter of the
/// first, in a run that shares the machine with thirteen hundred other tests,
/// which is the shape that measures the machine. And on a computer with no
/// Node it returned before asserting anything at all, so it passed by not
/// running.
///
/// The slot is the seam its own comment said did not exist. Seeding it and
/// reading it back afterwards asks the same two questions without a clock, and
/// both halves hold whether or not this machine has Node. The measurement is
/// kept below, ignored, because a number is still worth having on demand.
#[cfg(test)]
mod finding_node {
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// A remembered answer is handed back, and no process is started.
    ///
    /// Seeded with a path that is plainly not Node: getting it back is only
    /// possible by reading the slot. Take the read away and this hands back
    /// whatever the machine has, or `None`, and either fails.
    #[test]
    fn a_remembered_answer_is_handed_back_without_looking() {
        let sentinel = PathBuf::from(r"C:\nowhere\this-is-not-node.exe");
        let state = Mutex::new(Some(sentinel.clone()));

        assert_eq!(
            super::node_exe(&state),
            Some(sentinel),
            "the remembered answer was not used, so every caller pays a process"
        );
    }

    /// Finding it writes it down, and failing to find it does not.
    ///
    /// One assertion for both, because they are the same invariant read twice:
    /// after asking, the slot holds exactly what was answered. A positive
    /// answer must be there or the next caller pays for it again; a negative
    /// one must **not** be, because somebody can install Node while Sill is
    /// open and the store is where they would try again.
    #[test]
    fn the_slot_ends_up_holding_exactly_what_was_answered() {
        let state = Mutex::new(None);

        let found = super::node_exe(&state);
        let remembered = state.lock().expect("not poisoned").clone();

        assert_eq!(
            remembered, found,
            "asking for Node answered {found:?} and left {remembered:?} written down"
        );
    }

    /// What the second answer actually costs, on demand.
    ///
    /// Ignored, for the reason the store's browse budget was given headroom:
    /// cargo runs tests in parallel, so a wall-clock ratio inside the ordinary
    /// suite is a reading of how busy the machine is. Run it alone with
    /// `--ignored` when the number is the question.
    #[test]
    #[ignore]
    fn how_much_the_second_answer_costs() {
        let state = Mutex::new(None);

        let first = std::time::Instant::now();
        let found = super::node_exe(&state);
        let looking = first.elapsed();

        assert!(
            found.is_some(),
            "no Node on this machine, so there is nothing to time"
        );

        let again = std::time::Instant::now();
        let _ = super::node_exe(&state);
        let remembering = again.elapsed();

        eprintln!("looking took {looking:?}, remembering took {remembering:?}");
    }
}
