//! The local whisper.cpp server, kept resident between dictations.
//!
//! Transcription goes over HTTP to a long-lived `whisper-server` rather than
//! spawning `whisper-cli` per utterance, because the model would otherwise be
//! reloaded every single time: 148 MB for `base.en`, 1.5 GB for `medium.en`.
//! For a feature used dozens of times an hour that is the whole difference
//! between usable and not.
//!
//! The server binds an ephemeral loopback port and is addressed through the
//! ordinary `local` transcription provider, so a remote whisper server on
//! another machine goes down the same code path.
//!
//! Its lifetime is tied to Asyar's by a job object (see `job`), so a crash
//! takes the server down with it rather than stranding the model in memory.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::dictation::assets;
use crate::dictation::engine;
use crate::dictation::error::{DictationError, Result};
use crate::job::Job;

/// The runtime catalog name for whisper.cpp.
pub const RUNTIME: &str = "whisper";

/// How long to wait for a freshly spawned server to answer. Generous because
/// this covers loading the model: `medium.en` is 1.5 GB off a cold disk.
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const READY_POLL: Duration = Duration::from_millis(100);

/// Shut an idle server down rather than hold its model resident forever.
/// Long enough that a normal working day never pays the reload, short enough
/// that a machine left alone overnight gets its gigabyte back.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const IDLE_CHECK: Duration = Duration::from_secs(60);

/// Lines of server output kept for error reporting.
const LOG_TAIL: usize = 24;

/// Never take the whole machine: dictation runs *while* the user is working,
/// and whisper's gains flatten out well before the core count does.
const MAX_THREADS: usize = 8;
/// Cores left for everything else.
const RESERVED_CORES: usize = 2;

/// Arguments for `whisper-server`.
pub fn server_args(model: &Path, port: u16, threads: usize) -> Vec<String> {
    vec![
        "-m".to_string(),
        model.to_string_lossy().into_owned(),
        // Loopback only. whisper-server has no authentication whatsoever, so
        // any other bind address would transcribe for the whole network.
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "-t".to_string(),
        threads.to_string(),
        // The transcript is pasted as text; timestamps in it would be noise.
        "-nt".to_string(),
    ]
}

/// Base URL the transcription provider posts to.
pub fn base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// Worker threads to run the model with, given the machine's parallelism.
pub fn thread_count(available: usize) -> usize {
    available
        .saturating_sub(RESERVED_CORES)
        .clamp(1, MAX_THREADS)
}

/// Whether a server already running `running` has to be restarted to serve
/// `wanted`.
///
/// whisper-server does expose `POST /load` to swap models in place, but it
/// takes a server-side path rather than an upload, and a failed load leaves
/// the server with no model at all. Respawning costs a few seconds once per
/// model change and cannot half-succeed.
pub fn needs_restart(running: &str, wanted: &str) -> bool {
    running != wanted
}

struct Running {
    child: Child,
    port: u16,
    model_id: String,
    /// When the server first answered, not when it was spawned: the gap is
    /// the model load, which is the part worth excluding from "up for".
    started: Instant,
    last_used: Instant,
    log: Arc<Mutex<VecDeque<String>>>,
}

/// What a running server is doing right now.
///
/// Read by the settings panel, which polls it: a status surface that says
/// "running" and nothing else is only marginally better than saying nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshot {
    pub port: u16,
    pub model_id: String,
    pub pid: u32,
    /// Seconds since it answered its first readiness probe.
    pub uptime_seconds: u64,
    /// Seconds since the last dictation used it.
    pub idle_seconds: u64,
    /// How long it sits idle before shutting itself down.
    pub idle_timeout_seconds: u64,
    /// Working set in bytes, which is what Task Manager's Memory column
    /// shows. Private committed bytes run far higher because whisper commits
    /// compute buffers it never touches, so the working set is the honest
    /// number to put in front of someone.
    pub memory_bytes: u64,
}

#[derive(Default)]
struct Inner {
    running: Mutex<Option<Running>>,
    /// Serialises starts so two dictations racing on a cold server do not
    /// spawn two of them.
    starting: tokio::sync::Mutex<()>,
    watchdog: Mutex<bool>,
    /// Created on first use and held for as long as this server exists.
    /// Dropping it is what kills the members, so it must outlive them.
    job: OnceLock<Option<Job>>,
}

/// Owns the `whisper-server` process. Registered as Tauri managed state.
#[derive(Clone, Default)]
pub struct WhisperServer {
    inner: Arc<Inner>,
}

impl WhisperServer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The job every spawned server joins, or `None` where the platform has
    /// no equivalent.
    fn job(&self) -> Option<&Job> {
        self.inner.job.get_or_init(Job::new).as_ref()
    }

    /// Live details of the running server, or `None` when there is none.
    pub fn snapshot(&self) -> Option<ServerSnapshot> {
        let mut state = self.inner.running.lock().ok()?;
        let running = state.as_mut()?;

        // `try_wait` is the only honest liveness check: the process may have
        // been killed from Task Manager, and then everything below would be
        // reporting a server that is not there.
        if !matches!(running.child.try_wait(), Ok(None)) {
            return None;
        }

        let pid = running.child.id();
        Some(ServerSnapshot {
            port: running.port,
            model_id: running.model_id.clone(),
            pid,
            uptime_seconds: running.started.elapsed().as_secs(),
            idle_seconds: running.last_used.elapsed().as_secs(),
            idle_timeout_seconds: IDLE_TIMEOUT.as_secs(),
            memory_bytes: working_set(pid),
        })
    }

    /// Whether a server is up right now.
    pub fn is_running(&self) -> bool {
        self.inner
            .running
            .lock()
            .map(|state| state.is_some())
            .unwrap_or(false)
    }

    /// The base URL of a server serving `model_id`, starting or restarting
    /// one as needed. Returns the URL to hand to the `local` provider.
    pub async fn ensure(&self, app: &AppHandle, model_id: &str) -> Result<String> {
        if let Some(url) = self.reuse(model_id) {
            return Ok(url);
        }

        let _guard = self.inner.starting.lock().await;
        // Another caller may have started it while we waited for the lock.
        if let Some(url) = self.reuse(model_id) {
            return Ok(url);
        }
        self.stop();

        let exe = engine::binary_path(app)?;
        if !exe.is_file() {
            return Err(DictationError::NotFound(
                "whisper.cpp is not installed yet".to_string(),
            ));
        }
        let model = assets::model_path(app, model_id)?;
        if !model.is_file() {
            return Err(DictationError::NotFound(format!(
                "The {model_id} model has not been downloaded yet"
            )));
        }

        let running = spawn(&exe, &model, model_id, self.job()).await?;
        let url = base_url(running.port);
        if let Ok(mut state) = self.inner.running.lock() {
            *state = Some(running);
        }
        self.start_watchdog();
        Ok(url)
    }

    /// Returns the running server's URL when it already serves `model_id`
    /// and has not died, marking it used so the idle watchdog backs off.
    fn reuse(&self, model_id: &str) -> Option<String> {
        let mut state = self.inner.running.lock().ok()?;
        let running = state.as_mut()?;
        if needs_restart(&running.model_id, model_id) {
            return None;
        }
        // `try_wait` is the only honest liveness check: the process may have
        // been killed from Task Manager, in which case the port is gone and
        // the next POST would fail with a connection error instead.
        match running.child.try_wait() {
            Ok(None) => {
                running.last_used = Instant::now();
                Some(base_url(running.port))
            }
            // It died on us. `ensure` will start a replacement, but this is
            // the only moment the dead server's own account of why it stopped
            // is still in hand.
            Ok(Some(status)) => {
                crate::say!(
                    "the whisper server exited on its own ({status}). {}",
                    tail(&running.log)
                );
                None
            }
            Err(_) => None,
        }
    }

    /// Stops the server if one is running.
    pub fn stop(&self) {
        let Ok(mut state) = self.inner.running.lock() else {
            return;
        };
        if let Some(mut running) = state.take() {
            crate::say!("stopping the whisper server");
            let _ = running.child.kill();
            let _ = running.child.wait();
        }
    }

    /// One thread, started with the first server, that reclaims the model's
    /// memory when dictation goes unused for a while.
    fn start_watchdog(&self) {
        {
            let Ok(mut started) = self.inner.watchdog.lock() else {
                return;
            };
            if *started {
                return;
            }
            *started = true;
        }

        let inner = Arc::clone(&self.inner);
        std::thread::Builder::new()
            .name("whisper-idle".to_string())
            .spawn(move || loop {
                std::thread::sleep(IDLE_CHECK);
                let idle = {
                    let Ok(state) = inner.running.lock() else {
                        continue;
                    };
                    match state.as_ref() {
                        Some(running) => running.last_used.elapsed() >= IDLE_TIMEOUT,
                        None => false,
                    }
                };
                if idle {
                    crate::say!("whisper server idle; shutting it down");
                    WhisperServer {
                        inner: Arc::clone(&inner),
                    }
                    .stop();
                }
            })
            .ok();
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if let Ok(mut state) = self.running.lock() {
            if let Some(mut running) = state.take() {
                let _ = running.child.kill();
                let _ = running.child.wait();
            }
        }
    }
}

/// The process's working set in bytes, or 0 when it cannot be read.
///
/// Zero rather than an error: this is decoration on a status panel, and a
/// dictation that failed because a memory counter was unavailable would be
/// absurd.
#[cfg(windows)]
fn working_set(pid: u32) -> u64 {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    // SAFETY: the handle is closed on every path out, and the counters
    // struct is zeroed with its own size declared, as the API requires.
    unsafe {
        let Ok(handle) = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        ) else {
            return 0;
        };

        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            ..Default::default()
        };
        let read = GetProcessMemoryInfo(handle, &mut counters, counters.cb).is_ok();
        let _ = CloseHandle(handle);

        if read {
            counters.WorkingSetSize as u64
        } else {
            0
        }
    }
}

#[cfg(not(windows))]
fn working_set(_pid: u32) -> u64 {
    0
}

/// Spawns the server on a free loopback port and waits for it to answer.
async fn spawn(exe: &Path, model: &Path, model_id: &str, job: Option<&Job>) -> Result<Running> {
    let port = free_port()?;
    let threads = thread_count(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    );
    let args = server_args(model, port, threads);
    crate::say!("starting whisper server on port {port} with {threads} threads");

    let mut command = Command::new(exe);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // Piped, not null: whisper.cpp reports why it could not load a model
        // here, and that message is the only thing worth showing the user
        // when startup fails.
        .stderr(Stdio::piped());
    no_window(&mut command);

    let mut child = command.spawn().map_err(DictationError::Io)?;
    // Before the readiness wait, not after it: that wait runs for as long as
    // the model takes to load, and a crash during it would otherwise strand
    // the server.
    if let Some(job) = job {
        job.adopt(&child);
    }
    let log = drain_stderr(&mut child);

    let deadline = Instant::now() + READY_TIMEOUT;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        // Loopback. A proxy configured for the wider network must not be
        // consulted, and on some setups it would swallow the request.
        .no_proxy()
        .build()
        .map_err(|e| DictationError::Network(format!("readiness client: {e}")))?;
    let url = base_url(port);

    while Instant::now() < deadline {
        // The socket is only bound after the model is fully loaded (a bad
        // model path exits before the port ever opens), so any HTTP answer
        // at all means the server is ready to transcribe.
        if client.get(&url).send().await.is_ok() {
            return Ok(Running {
                child,
                port,
                model_id: model_id.to_string(),
                started: Instant::now(),
                last_used: Instant::now(),
                log,
            });
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(DictationError::Other(format!(
                "The whisper server exited immediately ({status}). {}",
                tail(&log)
            )));
        }
        tokio::time::sleep(READY_POLL).await;
    }

    let _ = child.kill();
    let _ = child.wait();
    Err(DictationError::Other(format!(
        "The whisper server did not start within {}s. {}",
        READY_TIMEOUT.as_secs(),
        tail(&log)
    )))
}

/// Asks the OS for an unused loopback port by binding one and letting go.
///
/// Racy in principle, since something else could take the port between the
/// close and the spawn. In practice that window is microseconds, and the
/// alternative (a fixed port) collides with a whisper server the user
/// started themselves, which is far more likely.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(DictationError::Io)?;
    let port = listener.local_addr().map_err(DictationError::Io)?.port();
    Ok(port)
}

/// Reads the child's stderr on its own thread, keeping the last few lines.
///
/// Draining is not optional: an unread pipe fills its buffer and then blocks
/// the child mid-write, and whisper.cpp is chatty enough to hit that.
fn drain_stderr(child: &mut Child) -> Arc<Mutex<VecDeque<String>>> {
    let log = Arc::new(Mutex::new(VecDeque::with_capacity(LOG_TAIL)));
    let Some(stderr) = child.stderr.take() else {
        return log;
    };
    let sink = Arc::clone(&log);
    std::thread::Builder::new()
        .name("whisper-stderr".to_string())
        .spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|line| line.ok()) {
                crate::say!("whisper: {line}");
                if let Ok(mut sink) = sink.lock() {
                    if sink.len() == LOG_TAIL {
                        sink.pop_front();
                    }
                    sink.push_back(line);
                }
            }
        })
        .ok();
    log
}

fn tail(log: &Arc<Mutex<VecDeque<String>>>) -> String {
    log.lock()
        .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join(" / "))
        .unwrap_or_default()
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: whisper-server is a console subsystem binary, and
    // without this every dictation session flashes a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn args(port: u16, threads: usize) -> Vec<String> {
        server_args(&PathBuf::from("/models/ggml-small.en.bin"), port, threads)
    }

    fn flag_value(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    }

    #[test]
    fn the_server_is_told_the_model_port_and_thread_count() {
        let args = args(51234, 8);
        assert_eq!(
            flag_value(&args, "-m").as_deref(),
            Some("/models/ggml-small.en.bin")
        );
        assert_eq!(flag_value(&args, "--port").as_deref(), Some("51234"));
        assert_eq!(flag_value(&args, "-t").as_deref(), Some("8"));
    }

    #[test]
    fn the_server_binds_loopback_only() {
        // Dictation audio is the user's speech. A server bound to 0.0.0.0
        // would transcribe for anyone on the network, and whisper-server has
        // no authentication at all.
        assert_eq!(
            flag_value(&args(51234, 8), "--host").as_deref(),
            Some("127.0.0.1")
        );
    }

    #[test]
    fn timestamps_are_suppressed() {
        // The transcript is pasted straight into whatever has focus, so a
        // leading `[00:00:00.000 --> ...]` would land in the document.
        assert!(args(1, 1).iter().any(|a| a == "-nt"));
    }

    #[test]
    fn the_base_url_is_loopback_at_the_given_port() {
        assert_eq!(base_url(51234), "http://127.0.0.1:51234");
    }

    #[test]
    fn the_base_url_carries_no_trailing_slash() {
        // `providers::local_request` appends `/inference` after trimming one
        // trailing slash, so a URL ending in `//` would still double up.
        let url = base_url(1);
        assert!(!url.ends_with('/'), "{url}");
        assert!(url.ends_with('1'), "{url}");
    }

    #[test]
    fn the_base_url_is_what_the_local_provider_accepts() {
        // The two halves live in different files; a mismatch would only show
        // up as a failed dictation.
        use crate::dictation::providers::{transcription_request, TranscribeOptions};
        let config = crate::dictation::provider::ProviderConfig {
            base_url: Some(base_url(51234)),
            ..crate::dictation::models::DictationSettings::default().provider
        };
        let request =
            transcription_request("local", &config, &TranscribeOptions::default()).unwrap();
        assert_eq!(request.url, "http://127.0.0.1:51234/inference");
    }

    /// Starts a real `whisper-server` and transcribes through it.
    ///
    /// Covers what the unit tests above cannot: that the arguments are ones
    /// the binary accepts, that waiting on an HTTP answer really does mean
    /// the model is loaded, and that the URL handed back transcribes.
    ///
    /// ```text
    /// $env:ASYAR_WHISPER_SERVER = "C:\...\whisper-server.exe"
    /// $env:ASYAR_WHISPER_MODEL  = "C:\...\ggml-base.en.bin"
    /// cargo test --lib dictation::server::tests::spawn -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "needs whisper-server and a model on disk"]
    async fn spawn_serves_a_transcription() {
        use crate::dictation::providers::{transcription_request, TranscribeOptions};
        use crate::dictation::transcriber::{build_transcription_client, transcribe};

        let (Ok(exe), Ok(model)) = (
            std::env::var("ASYAR_WHISPER_SERVER"),
            std::env::var("ASYAR_WHISPER_MODEL"),
        ) else {
            panic!("set ASYAR_WHISPER_SERVER and ASYAR_WHISPER_MODEL");
        };

        let server = WhisperServer::new();
        let started = Instant::now();
        let running = spawn(Path::new(&exe), Path::new(&model), "probe", server.job())
            .await
            .expect("the server should start");
        println!("server ready in {:?}", started.elapsed());

        // Half a second of a 440 Hz tone. The transcript is expected to be
        // empty; what is under test is that the round trip completes rather
        // than what the model hears in a sine wave.
        let samples: Vec<f32> = (0..8000)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 0.1)
            .collect();
        let clip = crate::dictation::wav::encode_mono_16bit(&samples, 16_000);

        let config = crate::dictation::provider::ProviderConfig {
            base_url: Some(base_url(running.port)),
            ..crate::dictation::models::DictationSettings::default().provider
        };
        let request =
            transcription_request("local", &config, &TranscribeOptions::default()).unwrap();

        let at = Instant::now();
        let transcript = transcribe(&build_transcription_client(), &request, clip)
            .await
            .expect("the server should transcribe");
        println!("transcribed in {:?}: {transcript:?}", at.elapsed());

        // A transcript with a timestamp in it means `-nt` did not take, and
        // that text would be pasted into the user's document verbatim.
        assert!(!transcript.contains("-->"), "{transcript:?}");

        *server.inner.running.lock().unwrap() = Some(running);
        assert!(server.is_running());

        // The status panel reports these, so they have to be real rather
        // than plausible: a zero working set means the memory read failed
        // silently and the panel would show "0 MB" for a loaded model.
        let live = server.snapshot().expect("a running server reports itself");
        println!("{live:?}");
        assert!(live.port > 0);
        assert!(live.pid > 0);
        assert!(
            live.memory_bytes > 50 * 1024 * 1024,
            "a loaded whisper model holds far more than {} bytes",
            live.memory_bytes
        );
        assert_eq!(live.idle_timeout_seconds, IDLE_TIMEOUT.as_secs());

        server.stop();
        assert!(!server.is_running());
        assert!(
            server.snapshot().is_none(),
            "a stopped server must not still report a port and a pid"
        );
    }

    #[test]
    fn threads_track_the_machine_but_leave_room_for_it() {
        // Dictation runs while the user is working. Taking every core makes
        // the transcription fast and everything else stutter.
        assert!(thread_count(16) < 16);
        assert!(thread_count(16) >= 4);
        assert!(thread_count(32) <= thread_count(64));
    }

    #[test]
    fn at_least_one_thread_is_always_requested() {
        // `available_parallelism` can fail and report 1; `-t 0` makes
        // whisper-server do no work at all.
        assert!(thread_count(1) >= 1);
        assert!(thread_count(0) >= 1);
    }

    #[test]
    fn threads_never_exceed_the_cap_however_big_the_machine_is() {
        assert_eq!(thread_count(1024), MAX_THREADS);
    }

    #[test]
    fn a_different_model_forces_a_restart() {
        assert!(needs_restart("base.en", "small.en"));
    }

    #[test]
    fn the_same_model_is_served_by_the_running_server() {
        assert!(!needs_restart("small.en", "small.en"));
    }

    #[test]
    fn a_free_port_is_a_real_one_we_could_bind() {
        let port = free_port().unwrap();
        assert!(port > 0);
        // Handing the port back means it must be bindable again.
        std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    }

    /// Starts a real server and then parks, so an external force-kill of
    /// this test process stands in for Asyar crashing.
    ///
    /// The job-object unit tests prove a closed job kills its members, and
    /// this proves the whisper server is actually one of them, using the
    /// production spawn path rather than a stand-in child. Prints the pid so
    /// the killer knows what to look for afterwards.
    #[tokio::test]
    #[ignore = "parks forever; meant to be force-killed from outside"]
    async fn spawn_parks_a_server_for_an_external_kill() {
        let (Ok(exe), Ok(model)) = (
            std::env::var("ASYAR_WHISPER_SERVER"),
            std::env::var("ASYAR_WHISPER_MODEL"),
        ) else {
            panic!("set ASYAR_WHISPER_SERVER and ASYAR_WHISPER_MODEL");
        };

        let server = WhisperServer::new();
        let running = spawn(Path::new(&exe), Path::new(&model), "probe", server.job())
            .await
            .expect("the server should start");
        println!("WHISPER_PID={}", running.child.id());
        println!("HARNESS_PID={}", std::process::id());
        *server.inner.running.lock().unwrap() = Some(running);

        // Deliberately never returns. Nothing after this line runs, so
        // nothing gets a chance to clean up: exactly the situation a crash
        // leaves behind.
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    #[test]
    fn a_fresh_server_reports_nothing_running() {
        assert!(!WhisperServer::new().is_running());
    }

    #[cfg(windows)]
    #[test]
    fn a_job_is_available_so_the_orphan_scan_is_never_needed() {
        // `ensure` skips scanning every process on the machine whenever a job
        // exists. If job creation ever started failing here, that scan would
        // silently come back on every cold start.
        assert!(WhisperServer::new().job().is_some());
    }

    #[test]
    fn the_job_is_created_once_and_reused() {
        // A second job would not hold the first one's members, so a server
        // spawned earlier would stop being covered.
        let server = WhisperServer::new();
        let first = server.job().map(std::ptr::from_ref);
        let second = server.job().map(std::ptr::from_ref);
        assert_eq!(first, second);
    }

    #[test]
    fn stopping_a_server_that_never_started_is_harmless() {
        let server = WhisperServer::new();
        server.stop();
        server.stop();
        assert!(!server.is_running());
        assert!(server.snapshot().is_none());
    }

    #[test]
    fn the_binary_this_spawns_is_the_one_the_installer_writes() {
        // These are edited in different files, and a mismatch would only ever
        // surface as "whisper.cpp is not installed yet", forever, on a
        // machine where it plainly is.
        assert!(
            crate::dictation::engine::ENTRY.starts_with(RUNTIME),
            "the installed binary {} should be the {RUNTIME} server",
            crate::dictation::engine::ENTRY
        );
    }
}
