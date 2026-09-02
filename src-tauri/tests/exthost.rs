//! Drives the real extension host from Rust and checks the full round trip.
//!
//! This is the M1 protocol gate. It needs `host/dist/host.js` built, which
//! `npm --prefix ../host run build` produces.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use sill_lib::action::Capability;
use sill_lib::exthost::permission::{needs_granting, plainly, AllowAll, Permits, NEEDED};
use sill_lib::exthost::{
    Alert, ApiLayer, AppInfo, Bridge, Clip, ExtHost, LoadOptions, Storage, UiEvent,
};
use tokio::sync::mpsc;
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(20);

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri has a parent")
        .to_path_buf()
}

fn host_js() -> PathBuf {
    repo_root().join("host").join("dist").join("host.js")
}

fn fixture() -> PathBuf {
    repo_root()
        .join("host")
        .join("test")
        .join("fixture")
        .join("list-command.js")
}

/// Waits for the first event matching `pick`, ignoring the rest.
async fn wait_for<T>(
    events: &mut mpsc::UnboundedReceiver<UiEvent>,
    label: &str,
    mut pick: impl FnMut(&UiEvent) -> Option<T>,
) -> T {
    let found = timeout(WAIT, async {
        while let Some(event) = events.recv().await {
            if let Some(value) = pick(&event) {
                return Some(value);
            }
        }
        None
    })
    .await;

    match found {
        Ok(Some(value)) => value,
        Ok(None) => panic!("event stream ended while waiting for {label}"),
        Err(_) => panic!("timed out waiting for {label}"),
    }
}

/// Finds the first `create` op for a tag and returns its props.
fn created_props(ops: &Value, tag: &str) -> Option<Value> {
    ops.as_array()?.iter().find_map(|op| {
        let is_create = op.get("op").and_then(Value::as_str) == Some("create");
        let matches = op.get("$t").and_then(Value::as_str) == Some(tag);
        (is_create && matches).then(|| op.get("props").cloned().unwrap_or(json!({})))
    })
}

#[tokio::test]
async fn runs_a_raycast_extension_end_to_end() {
    let host = host_js();
    assert!(
        host.exists(),
        "host bundle missing at {}; run: npm --prefix host run build",
        host.display()
    );

    let (tx, mut events) = mpsc::unbounded_channel();

    let exthost = ExtHost::spawn(&PathBuf::from("node"), &host, layer(tx).0)
        .await
        .expect("spawned the host");

    // ---- load, which also performs the ready handshake ----
    let opts = LoadOptions::view(fixture().to_string_lossy(), "fixture", "list");
    let session = exthost.load(&opts).await.expect("load succeeded");
    assert!(!session.is_empty(), "load returned a session id");

    assert_eq!(
        exthost.extension_of(&session).as_deref(),
        Some("fixture"),
        "session is tracked against its extension"
    );

    // ---- the first render ----
    let ops = wait_for(&mut events, "UI/render", |event| match event {
        UiEvent::Render { session: s, ops } => Some((s.clone(), ops.clone())),
        _ => None,
    })
    .await;

    assert_eq!(ops.0, session, "render arrived on the right session");
    let ops = ops.1;

    let count = ops.as_array().map(Vec::len).unwrap_or(0);
    assert!(count > 0, "render carried ops, got {count}");

    let list = created_props(&ops, "List");
    assert!(list.is_some(), "a List node was created");

    let item = created_props(&ops, "List.Item").expect("a List.Item was created");
    assert_eq!(
        item.get("title").and_then(Value::as_str),
        Some("Apple"),
        "the first item carried its title"
    );

    // ---- the handler id the UI would fire ----
    let action = created_props(&ops, "Action").expect("an Action was created");
    let handler = action
        .get("onAction")
        .and_then(|h| h.get("$handler"))
        .and_then(Value::as_str)
        .expect("onAction became a handler reference")
        .to_string();

    // ---- activate it; the extension should call back into us ----
    exthost
        .activate_handler(&session, &handler, json!([]))
        .await
        .expect("handler activation was accepted");

    let (toast_title, toast_style) = wait_for(&mut events, "UI/showToast", |event| match event {
        UiEvent::ShowToast { title, style, .. } => Some((title.clone(), style.clone())),
        _ => None,
    })
    .await;

    assert_eq!(toast_title, "Picked Apple", "toast carried the title");
    assert_eq!(toast_style, "success", "toast carried the style");

    // ---- unload ----
    assert!(
        exthost.unload(&session).await.expect("unload call"),
        "unload succeeded"
    );
    assert!(
        exthost.extension_of(&session).is_none(),
        "session was forgotten after unload"
    );
}

/// What the stub bridge was asked to do.
#[derive(Default)]
struct Asked {
    written: Vec<Clip>,
    pasted: Vec<Clip>,
    opened: Vec<(String, Option<String>)>,
    cleared: usize,
    confirmed: Vec<String>,
    defaults: Vec<String>,
}

/// A bridge that records rather than acts.
///
/// The point of the trait: these calls used to reach nothing at all, and
/// proving they now arrive must not require a clipboard, a shell or a person
/// to click a dialog.
struct StubBridge {
    asked: Mutex<Asked>,
    /// What `Clipboard/readContent` finds.
    holds: Option<String>,
    /// What the person says to `confirmAlert`.
    answer: bool,
    /// What is highlighted in the window the launcher came up over.
    selection: Option<String>,
}

impl StubBridge {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Asked::default()),
            holds: Some("already on the clipboard".to_string()),
            answer: true,
            selection: Some("what was highlighted".to_string()),
        })
    }

    fn asked(&self) -> std::sync::MutexGuard<'_, Asked> {
        self.asked.lock().expect("recorder poisoned")
    }
}

impl Bridge for StubBridge {
    fn clipboard_write(&self, clip: &Clip) -> Result<(), String> {
        self.asked().written.push(clip.clone());
        Ok(())
    }

    fn clipboard_read(&self) -> Result<Clip, String> {
        Ok(Clip {
            text: self.holds.clone(),
            ..Clip::default()
        })
    }

    fn clipboard_clear(&self) -> Result<(), String> {
        self.asked().cleared += 1;
        Ok(())
    }

    fn clipboard_paste(&self, clip: &Clip) -> Result<(), String> {
        self.asked().pasted.push(clip.clone());
        Ok(())
    }

    fn open(&self, target: &str, with: Option<&str>) -> Result<(), String> {
        self.asked()
            .opened
            .push((target.to_string(), with.map(str::to_string)));
        Ok(())
    }

    fn applications(&self) -> Result<Vec<AppInfo>, String> {
        Ok(vec![AppInfo {
            name: "Notepad".into(),
            path: r"C:\Windows\notepad.exe".into(),
            bundle_id: None,
        }])
    }

    fn confirm(&self, alert: &Alert) -> Result<bool, String> {
        self.asked().confirmed.push(alert.title.clone());
        Ok(self.answer)
    }

    fn selected_text(&self) -> Result<Option<String>, String> {
        Ok(self.selection.clone())
    }

    fn default_application(&self, target: &str) -> Result<Option<AppInfo>, String> {
        self.asked().defaults.push(target.to_string());

        // Only one thing has a handler here, so the other case is exercised
        // too: an address nothing is registered for is an ordinary state of a
        // machine rather than a fault.
        if target.ends_with(".txt") {
            return Ok(Some(AppInfo {
                name: "Notepad".into(),
                path: r"C:\Windows\notepad.exe".into(),
                bundle_id: Some(r"C:\Windows\notepad.exe".into()),
            }));
        }

        Ok(None)
    }
}

fn layer(tx: mpsc::UnboundedSender<UiEvent>) -> (Arc<ApiLayer>, Arc<StubBridge>) {
    let bridge = StubBridge::new();
    let storage = Arc::new(Storage::memory().expect("in-memory store"));
    (
        Arc::new(ApiLayer::new(
            tx,
            bridge.clone(),
            storage,
            Arc::new(AllowAll),
        )),
        bridge,
    )
}

#[tokio::test]
async fn unknown_api_methods_are_reported_not_defaulted() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, _bridge) = layer(tx);

    let err = api
        .dispatch("s1", "ext", "Nonsense/method", &json!({}))
        .await
        .expect_err("an unimplemented method must error");

    assert!(
        err.message.contains("Nonsense/method"),
        "the error names the missing method, got: {}",
        err.message
    );
}

#[tokio::test]
async fn storage_is_scoped_per_extension() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, _bridge) = layer(tx);

    api.dispatch(
        "s1",
        "alpha",
        "Storage/set",
        &json!({"key": "k", "value": "from-alpha"}),
    )
    .await
    .expect("set");

    let mine = api
        .dispatch("s1", "alpha", "Storage/get", &json!({"key": "k"}))
        .await
        .expect("get");
    assert_eq!(
        mine,
        json!("from-alpha"),
        "an extension reads its own value"
    );

    let theirs = api
        .dispatch("s2", "beta", "Storage/get", &json!({"key": "k"}))
        .await
        .expect("get");
    assert_eq!(theirs, Value::Null, "another extension cannot see it");
}

/// The regression this whole seam exists for.
///
/// `host/src/api/runtime.ts` has always called these nine methods and the
/// Rust side answered none of them, so every one returned "method not found".
/// `Clipboard.copy` is close to the most-used call in the Raycast ecosystem,
/// which made this the highest-severity gap in the extension platform.
#[tokio::test]
async fn the_methods_the_host_calls_are_all_answered() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, bridge) = layer(tx);

    let calls: Vec<(&str, Value)> = vec![
        ("Clipboard/copy", json!({"content": "hello"})),
        ("Clipboard/paste", json!({"content": "hello"})),
        ("Clipboard/clear", json!({})),
        ("Clipboard/readContent", json!({})),
        ("Application/open", json!({"target": "https://example.com"})),
        ("Application/list", json!({})),
        ("UI/confirmAlert", json!({"title": "Sure?"})),
    ];

    for (method, params) in calls {
        let result = api.dispatch("s1", "ext", method, &params).await;
        assert!(
            result.is_ok(),
            "{method} was not answered: {:?}",
            result.err().map(|e| e.message)
        );
    }

    let asked = bridge.asked();
    assert_eq!(asked.written.len(), 1, "copy reached the bridge");
    assert_eq!(asked.pasted.len(), 1, "paste reached the bridge");
    assert_eq!(asked.cleared, 1, "clear reached the bridge");
    assert_eq!(asked.opened.len(), 1, "open reached the bridge");
    assert_eq!(asked.confirmed, vec!["Sure?".to_string()]);
}

#[tokio::test]
async fn copying_a_secret_says_so_all_the_way_down() {
    // `concealed` is what stops a token an extension copied from being
    // written to clipboard history in plain text. It has to survive the trip.
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, bridge) = layer(tx);

    api.dispatch(
        "s1",
        "ext",
        "Clipboard/copy",
        &json!({"content": "hunter2", "options": {"concealed": true}}),
    )
    .await
    .expect("copy");

    let asked = bridge.asked();
    assert!(asked.written[0].concealed, "the secret arrived unmarked");
}

#[tokio::test]
async fn reading_the_clipboard_returns_raycasts_shape() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, _bridge) = layer(tx);

    let content = api
        .dispatch("s1", "ext", "Clipboard/readContent", &json!({}))
        .await
        .expect("read");

    // Extensions destructure this rather than probing it, so every key has to
    // be present even when there is nothing to put in it.
    assert_eq!(
        content.get("text"),
        Some(&json!("already on the clipboard"))
    );
    assert_eq!(content.get("html"), Some(&Value::Null));
    assert_eq!(content.get("file"), Some(&Value::Null));
}

#[tokio::test]
async fn opening_with_a_named_application_passes_it_through() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, bridge) = layer(tx);

    api.dispatch(
        "s1",
        "ext",
        "Application/open",
        &json!({"target": "https://example.com", "appId": "firefox.exe"}),
    )
    .await
    .expect("open");

    let asked = bridge.asked();
    assert_eq!(
        asked.opened[0],
        (
            "https://example.com".to_string(),
            Some("firefox.exe".to_string())
        )
    );
}

#[tokio::test]
async fn opening_nothing_is_an_error_rather_than_a_silent_no_op() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (api, bridge) = layer(tx);

    api.dispatch("s1", "ext", "Application/open", &json!({}))
        .await
        .expect_err("open with no target must fail");

    assert!(
        bridge.asked().opened.is_empty(),
        "an empty target reached the shell"
    );
}

/// Methods the host is allowed to call without Rust answering them.
///
/// Empty, and the test below is what keeps it honest: adding a call on the
/// host side without an answer on this side fails until it is either
/// implemented or written down here as a decision.
///
/// The last two came out on 2026-08-30. Neither needed the work their note
/// claimed: reading the selection is the capture the launcher already does for
/// its own text actions, and the default application is one `AssocQueryString`
/// call. **A gap listed as needing a large piece of work is worth re-reading
/// before it is believed.**
const DECLARED_GAPS: &[&str] = &[];

/// JSON-RPC's code for a method the server does not have.
const METHOD_NOT_FOUND: i32 = -32601;

/// Every method the host can call is answered, or is a declared gap.
///
/// This is the test that was missing. `host/src/api/runtime.ts` called nine
/// methods that `ApiLayer::dispatch` had no arm for, and every one failed at
/// runtime with "method not found". Nothing caught it: the Rust tests only
/// exercised the methods that existed, and `scripts/run-extension.mjs` serves
/// the API from its own hand-written table, so its stub answered calls the
/// real implementation did not. The gate was green because it was grading a
/// different program.
///
/// Reading the host's source is the point. The two sides cannot drift while
/// the list comes from one of them.
#[tokio::test]
async fn every_method_the_host_calls_is_answered_or_declared_missing() {
    let api_dir = repo_root().join("host").join("src").join("api");

    let mut wanted: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&api_dir).expect("host/src/api is readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ts") {
            continue;
        }

        let source = std::fs::read_to_string(&path).expect("a host source file");

        // `request("Service/method"` and `request<T>("Service/method"`, which
        // is every shape the bridge is called in.
        for (index, _) in source.match_indices("request") {
            let rest = &source[index..];
            let Some(open) = rest.find('"') else { continue };
            // A quote far past the call is some other string on a later line.
            if open > 40 {
                continue;
            }
            let Some(close) = rest[open + 1..].find('"') else {
                continue;
            };
            let method = &rest[open + 1..open + 1 + close];
            if method.contains('/') && !wanted.iter().any(|m| m == method) {
                wanted.push(method.to_string());
            }
        }
    }

    assert!(
        wanted.len() >= 15,
        "found only {} methods in {}; the scan is broken, not the host",
        wanted.len(),
        api_dir.display()
    );

    let (tx, _events) = mpsc::unbounded_channel();
    let (api, _bridge) = layer(tx);

    let mut missing: Vec<String> = Vec::new();
    for method in &wanted {
        // Empty params on purpose. A method that needs an argument should
        // complain about the argument, which is a different failure from not
        // existing, and only the second one is this test's business.
        let outcome = api.dispatch("s1", "ext", method, &json!({})).await;

        let absent = matches!(&outcome, Err(err) if err.code == METHOD_NOT_FOUND);
        if absent && !DECLARED_GAPS.contains(&method.as_str()) {
            missing.push(method.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "the host calls these and Rust answers none of them: {missing:?}\n\
         Implement them in exthost::api, or add them to DECLARED_GAPS with a reason."
    );

    // The other direction, so a gap that gets fixed does not sit in the list
    // forever pretending to be a known limitation.
    for gap in DECLARED_GAPS {
        let outcome = api.dispatch("s1", "ext", gap, &json!({})).await;
        assert!(
            matches!(&outcome, Err(err) if err.code == METHOD_NOT_FOUND),
            "{gap} is implemented now; take it out of DECLARED_GAPS"
        );
    }
}

#[test]
fn a_command_carries_the_preferences_its_manifest_declared() {
    // `LoadOptions::for_command` hardcoded `{}`, so `getPreferenceValues()`
    // answered with nothing however many defaults a manifest declared. All
    // four extensions in the sample set declare some, which meant all four ran
    // with every setting undefined.
    let opts = LoadOptions::with_preferences(
        "entry.js",
        "uuid-generator",
        "generateV7",
        sill_lib::exthost::CommandMode::View,
        json!({ "defaultAction": "copy", "base32Encoding": false }),
    );

    assert_eq!(opts.preferences.get("defaultAction"), Some(&json!("copy")));
    assert_eq!(opts.preferences.get("base32Encoding"), Some(&json!(false)));
}

#[test]
fn preferences_are_always_an_object_on_the_wire() {
    // The host spreads this into the bridge. Spreading null throws, where an
    // empty object is simply empty, so a record with no preferences must not
    // arrive as one.
    let from_nothing = LoadOptions::with_preferences(
        "entry.js",
        "ext",
        "cmd",
        sill_lib::exthost::CommandMode::View,
        Value::Null,
    );
    assert!(from_nothing.preferences.is_object());

    let plain = LoadOptions::for_command(
        "entry.js",
        "ext",
        "cmd",
        sill_lib::exthost::CommandMode::View,
    );
    assert!(plain.preferences.is_object());
}

#[test]
fn the_built_index_carries_preferences_through_to_the_record() {
    // End of the pipeline that `scripts/build-extension.mjs` starts: a
    // manifest default has to survive being written to index.json and read
    // back as a `CommandRecord`, or none of the above matters.
    let index = repo_root()
        .join("extensions")
        .join("build")
        .join("index.json");

    let commands = sill_lib::registry::load_index(&index);
    if commands.is_empty() {
        eprintln!("no built extensions; skipping");
        return;
    }

    let with_preferences: Vec<_> = commands
        .iter()
        .filter(|command| {
            command
                .preferences
                .as_object()
                .is_some_and(|p| !p.is_empty())
        })
        .collect();

    assert!(
        !with_preferences.is_empty(),
        "not one of the {} built commands carries a preference. Every manifest \
         in extensions/raycast-src declares some, so either the build script \
         stopped emitting them or the record stopped reading them",
        commands.len()
    );
}

// ------------------------------------- the last two calls that reached nothing

#[tokio::test]
async fn an_extension_can_read_what_is_selected() {
    // The host has always offered `getSelectedText`. Rust answered
    // `method not found`, so every extension acting on a selection failed at
    // the first line of its own command.
    let (tx, _rx) = mpsc::unbounded_channel();
    let (layer, _bridge) = layer(tx);

    let answered = layer
        .dispatch("s1", "ext", "UI/getSelectedText", &json!({}))
        .await
        .expect("answered");

    assert_eq!(answered, json!("what was highlighted"));
}

#[tokio::test]
async fn nothing_selected_reads_as_empty_rather_than_as_a_failure() {
    // The call is typed as returning a string and extensions go straight to
    // `.trim()` on it. Null would be a crash in somebody else's code.
    let (tx, _rx) = mpsc::unbounded_channel();
    let bridge = Arc::new(StubBridge {
        asked: Mutex::new(Asked::default()),
        holds: None,
        answer: true,
        selection: None,
    });
    let storage = Arc::new(Storage::memory().expect("in-memory store"));
    let layer = Arc::new(ApiLayer::new(
        tx,
        bridge.clone(),
        storage,
        Arc::new(AllowAll),
    ));

    let answered = layer
        .dispatch("s1", "ext", "UI/getSelectedText", &json!({}))
        .await
        .expect("answered");

    assert_eq!(answered, json!(""));
}

#[tokio::test]
async fn an_extension_can_ask_what_opens_a_file() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let (layer, bridge) = layer(tx);

    let answered = layer
        .dispatch(
            "s1",
            "ext",
            "Application/getDefault",
            &json!({ "target": "notes.txt" }),
        )
        .await
        .expect("answered");

    assert_eq!(answered["name"], json!("Notepad"));
    assert_eq!(bridge.asked().defaults, vec!["notes.txt".to_string()]);
}

#[tokio::test]
async fn an_address_nothing_handles_answers_null_rather_than_failing() {
    // A machine with no handler registered for something is an ordinary
    // machine, not a fault in the extension that asked about it.
    let (tx, _rx) = mpsc::unbounded_channel();
    let (layer, _bridge) = layer(tx);

    let answered = layer
        .dispatch(
            "s1",
            "ext",
            "Application/getDefault",
            &json!({ "target": "weird://thing" }),
        )
        .await
        .expect("answered");

    assert_eq!(answered, json!(null));
}

#[tokio::test]
async fn getting_a_default_for_nothing_is_refused() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let (layer, _bridge) = layer(tx);

    let refused = layer
        .dispatch(
            "s1",
            "ext",
            "Application/getDefault",
            &json!({ "target": "" }),
        )
        .await;

    assert!(refused.is_err(), "an empty target was accepted");
}

// --------------------------------------------------------------------------
// Permission, which is the part that has to hold when somebody says no.
// --------------------------------------------------------------------------

/// Refuses everything it is asked about, and records what it was asked.
struct RefuseAll {
    asked: Mutex<Vec<Vec<Capability>>>,
}

impl RefuseAll {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl Permits for RefuseAll {
    async fn allow(&self, _extension: &str, needs: &[Capability]) -> Result<(), String> {
        self.asked.lock().unwrap().push(needs.to_vec());

        match needs.iter().find(|c| needs_granting(c)) {
            Some(capability) => Err(format!("not allowed to {}", plainly(capability))),
            None => Ok(()),
        }
    }
}

fn refusing(
    tx: mpsc::UnboundedSender<UiEvent>,
) -> (Arc<ApiLayer>, Arc<StubBridge>, Arc<RefuseAll>) {
    let bridge = StubBridge::new();
    let permits = RefuseAll::new();
    let storage = Arc::new(Storage::memory().expect("in-memory store"));

    (
        Arc::new(ApiLayer::new(tx, bridge.clone(), storage, permits.clone())),
        bridge,
        permits,
    )
}

/// A refusal at `require` has to reach the window.
///
/// **This is the silent hang.** `fs`, `net` and `child_process` are gated
/// while a module loads, which is synchronous and has no RPC to hang an
/// approval card on, so an extension that needs one dies before it renders.
/// The load itself succeeds and hands back a session id, so the launcher shows
/// "opening ..." and waits for a first render that is never coming.
///
/// Nothing else catches it. `gate:views` builds extensions that are granted
/// everything, the fixture below existed with a comment saying a refusal
/// "arrives as an extension crash carrying the reason" and no test asserting
/// it, and a person reports it as the command doing nothing.
///
/// So: load it with nothing granted, and require that the crash arrives and
/// says which permission it was.
#[tokio::test]
async fn a_module_refused_while_loading_reaches_the_window() {
    let host = host_js();
    assert!(host.exists(), "host bundle missing at {}", host.display());

    let (tx, mut events) = mpsc::unbounded_channel();

    let exthost = ExtHost::spawn(&PathBuf::from("node"), &host, layer(tx).0)
        .await
        .expect("spawned the host");

    let wants_disk = repo_root()
        .join("host")
        .join("test")
        .join("fixture")
        .join("reads-disk.js");

    // Nothing granted, which is what an extension nobody has answered for gets.
    let opts = LoadOptions::view(wants_disk.to_string_lossy(), "reads-disk", "cmd");
    assert!(
        opts.capabilities.is_empty(),
        "this test is about the ungranted case"
    );

    let session = exthost.load(&opts).await.expect("the load itself succeeds");

    // The load succeeding is the whole trap: there is a session, so the window
    // has every reason to sit and wait for it.
    assert!(!session.is_empty());

    let reason = wait_for(&mut events, "the crash", |event| match event {
        UiEvent::Crashed { reason, .. } => Some(reason.clone()),
        _ => None,
    })
    .await;

    assert!(
        reason.contains("fs"),
        "the crash has to name the module that was refused, got {reason:?}"
    );
    assert!(
        reason.to_lowercase().contains("not allowed"),
        "and say it was a permission rather than a fault in the extension, got {reason:?}"
    );
}

/// The property the whole permission layer exists for.
///
/// Not "the call returns an error", which a check written after the work would
/// also satisfy while the clipboard had already been read. The bridge must
/// never have been touched, because by the time it is, the thing somebody said
/// no to has happened.
#[tokio::test]
async fn a_refused_call_never_reaches_the_machine() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (layer, bridge, _permits) = refusing(tx);

    let refused = layer
        .dispatch("s1", "ext", "Clipboard/copy", &json!({ "text": "taken" }))
        .await;

    assert!(refused.is_err(), "a refused copy reported success");
    assert!(
        bridge.asked.lock().unwrap().written.is_empty(),
        "the clipboard was written despite the refusal",
    );
}

/// Refusing has to say which permission, or nobody can turn it on.
#[tokio::test]
async fn a_refusal_names_the_permission_it_refused() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (layer, _bridge, _permits) = refusing(tx);

    let refused = layer
        .dispatch(
            "s1",
            "ext",
            "Application/open",
            &json!({ "target": "https://x" }),
        )
        .await
        .expect_err("should refuse");

    assert!(
        format!("{refused:?}").contains("open programs"),
        "a refusal that does not name the permission: {refused:?}",
    );
}

/// An extension drawing its own view and using its own storage asks nobody.
///
/// If these ever start needing permission, every extension prompts on startup
/// for things that reach nothing, and people learn to agree without reading.
#[tokio::test]
async fn drawing_and_own_storage_are_never_refused() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (layer, _bridge, _permits) = refusing(tx);

    for (method, params) in [
        ("UI/render", json!({ "ops": [] })),
        ("Storage/set", json!({ "key": "k", "value": "v" })),
        ("Storage/get", json!({ "key": "k" })),
    ] {
        assert!(
            layer.dispatch("s1", "ext", method, &params).await.is_ok(),
            "{method} was refused",
        );
    }
}

/// Every method is checked, not only the ones somebody remembered.
///
/// Walks what the API answers and asserts the permission layer was consulted
/// for each. A method added later that skips the gate fails here rather than
/// in somebody's clipboard history.
#[tokio::test]
async fn every_method_passes_through_the_gate() {
    let (tx, _events) = mpsc::unbounded_channel();
    let (layer, _bridge, permits) = refusing(tx);

    for (method, _needs) in NEEDED {
        let _ = layer.dispatch("s1", "ext", method, &json!({})).await;
    }

    assert_eq!(
        permits.asked.lock().unwrap().len(),
        NEEDED.len(),
        "some methods reached the machine without being checked",
    );
}

/// Forgetting to fill in what an extension may reach must cost it everything,
/// not give it everything.
///
/// The struct this replaced defaulted to three `false` booleans that nothing
/// read, so its default was neither safe nor unsafe; it simply did not matter.
/// This one matters, and the default is the direction where a mistake is an
/// extension that cannot open a file rather than one that can.
#[test]
fn an_extension_is_allowed_nothing_until_somebody_says_otherwise() {
    let opts = LoadOptions::view("entry.js", "ext", "cmd");

    assert!(
        opts.capabilities.is_empty(),
        "a freshly built load carries permissions nobody granted: {:?}",
        opts.capabilities,
    );
}

/// A host that has died says so, and stops being handed out.
///
/// Nothing watched the child, so a crashed host left its handle in place and
/// every later launch got it back and failed with "channel is closed". Worse,
/// asking for the host is what marks it used, so the idle watchdog never
/// considered it idle and never replaced it: extensions stayed broken until
/// Sill was restarted.
///
/// The stream ending is the signal. Killing the process from outside is the
/// closest thing to the crash this is about.
#[tokio::test]
async fn a_host_that_dies_is_not_handed_out_again() {
    let host = host_js();
    assert!(
        host.exists(),
        "host bundle missing; run: npm --prefix host run build"
    );

    let (tx, _events) = mpsc::unbounded_channel();

    let exthost = ExtHost::spawn(&PathBuf::from("node"), &host, layer(tx).0)
        .await
        .expect("spawned the host");

    assert!(exthost.alive(), "a host that has just started is alive");

    // Loading proves it is answering, so the flag below means something.
    let opts = LoadOptions::view(fixture().to_string_lossy(), "fixture", "list");
    exthost.load(&opts).await.expect("load succeeded");

    let id = exthost.child_id().expect("the child has a process id");

    #[cfg(windows)]
    std::process::Command::new("taskkill")
        .args(["/PID", &id.to_string(), "/F", "/T"])
        .output()
        .expect("killed the host");

    #[cfg(not(windows))]
    std::process::Command::new("kill")
        .args(["-9", &id.to_string()])
        .output()
        .expect("killed the host");

    // The reader task notices when the stream ends, which is not instant.
    for _ in 0..100 {
        if !exthost.alive() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    assert!(
        !exthost.alive(),
        "the host was killed and still reports itself as answering, so \
         `host_of` would hand it back for the rest of the session"
    );

    // And anything still waiting was told, rather than left holding a
    // `oneshot` whose sender is gone.
    let after = exthost.load(&opts).await;
    assert!(
        after.is_err(),
        "a load against a dead host has to fail rather than hang"
    );
}
