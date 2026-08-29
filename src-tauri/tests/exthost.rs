//! Drives the real extension host from Rust and checks the full round trip.
//!
//! This is the M1 protocol gate. It needs `host/dist/host.js` built, which
//! `npm --prefix ../host run build` produces.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use sill_lib::exthost::{ExtHost, LoadOptions, UiEvent};
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

    let exthost = ExtHost::spawn(&PathBuf::from("node"), &host, tx)
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

#[tokio::test]
async fn unknown_api_methods_are_reported_not_defaulted() {
    let (tx, _events) = mpsc::unbounded_channel();
    let api = sill_lib::exthost::ApiLayer::new(tx);

    let err = api
        .dispatch("s1", "ext", "Nonsense/method", &json!({}))
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
    let api = sill_lib::exthost::ApiLayer::new(tx);

    api.dispatch("s1", "alpha", "Storage/set", &json!({"key": "k", "value": "from-alpha"}))
        .expect("set");

    let mine = api
        .dispatch("s1", "alpha", "Storage/get", &json!({"key": "k"}))
        .expect("get");
    assert_eq!(mine, json!("from-alpha"), "an extension reads its own value");

    let theirs = api
        .dispatch("s2", "beta", "Storage/get", &json!({"key": "k"}))
        .expect("get");
    assert_eq!(
        theirs,
        Value::Null,
        "another extension cannot see it"
    );
}
