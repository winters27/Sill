//! Drives the real extension host from Rust and checks the full round trip.
//!
//! This is the M1 protocol gate. It needs `host/dist/host.js` built, which
//! `npm --prefix ../host run build` produces.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
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
    assert!(exthost.unload(&session).await.expect("unload call"), "unload succeeded");
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
}

impl StubBridge {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            asked: Mutex::new(Asked::default()),
            holds: Some("already on the clipboard".to_string()),
            answer: true,
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
}

fn layer(tx: mpsc::UnboundedSender<UiEvent>) -> (Arc<ApiLayer>, Arc<StubBridge>) {
    let bridge = StubBridge::new();
    let storage = Arc::new(Storage::memory().expect("in-memory store"));
    (
        Arc::new(ApiLayer::new(tx, bridge.clone(), storage)),
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

    api.dispatch("s1", "alpha", "Storage/set", &json!({"key": "k", "value": "from-alpha"}))
        .await
        .expect("set");

    let mine = api
        .dispatch("s1", "alpha", "Storage/get", &json!({"key": "k"}))
        .await
        .expect("get");
    assert_eq!(mine, json!("from-alpha"), "an extension reads its own value");

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
    assert_eq!(content.get("text"), Some(&json!("already on the clipboard")));
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
/// Each one is a decision, not an oversight. Both need work that belongs
/// elsewhere: reading the foreground selection is UI Automation, and the
/// default application for a file type is `AssocQueryString`. Until then they
/// fail loudly, naming themselves, which is the house rule for a gap.
const DECLARED_GAPS: &[&str] = &["UI/getSelectedText", "Application/getDefault"];

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
