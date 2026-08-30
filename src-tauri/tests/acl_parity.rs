//! Every window Sill declares can actually call Rust.
//!
//! Tauri 2 gates `invoke` per window: a webview whose label is absent from a
//! capability's `windows` list has every command denied at the ACL layer,
//! **silently**. Nothing throws in Rust, nothing appears in the log, and the
//! page renders perfectly. The only symptom is that clicking does nothing,
//! which reads as a dead frontend rather than as a permission.
//!
//! That is exactly how the tray menu shipped broken: the window was added to
//! `tauri.conf.json` and the route was written, but `capabilities/default.json`
//! still listed only the three windows that existed before it.
//!
//! Comparing the two files is the whole test. It costs nothing and it is the
//! only thing that catches the next window somebody adds.

use std::collections::BTreeSet;
use std::path::Path;

/// Window labels declared in `tauri.conf.json`.
fn declared_windows(conf: &serde_json::Value) -> BTreeSet<String> {
    conf.get("app")
        .and_then(|app| app.get("windows"))
        .and_then(|w| w.as_array())
        .map(|windows| {
            windows
                .iter()
                .filter_map(|w| w.get("label")?.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Window labels any capability grants permissions to.
fn permitted_windows(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();

    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            panic!("{} is not valid JSON", path.display());
        };

        if let Some(windows) = value.get("windows").and_then(|w| w.as_array()) {
            out.extend(
                windows
                    .iter()
                    .filter_map(|w| w.as_str().map(str::to_owned)),
            );
        }
    }

    out
}

#[test]
fn every_declared_window_can_invoke() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let conf = std::fs::read_to_string(root.join("tauri.conf.json"))
        .expect("tauri.conf.json is readable");
    let conf: serde_json::Value =
        serde_json::from_str(&conf).expect("tauri.conf.json is valid JSON");

    let declared = declared_windows(&conf);
    assert!(
        !declared.is_empty(),
        "no windows found in tauri.conf.json, so this test is not checking anything"
    );

    let permitted = permitted_windows(&root.join("capabilities"));

    let missing: Vec<&String> = declared.difference(&permitted).collect();
    assert!(
        missing.is_empty(),
        "these windows exist but no capability lists them, so every command they \
         invoke is denied silently and their UI will look dead: {missing:?}"
    );
}

/// The reverse, which is a typo rather than a broken feature.
///
/// A capability naming a window that does not exist grants nothing to nobody.
/// Harmless at runtime, but it is always either a rename that was half done or
/// a misspelling, and both are worth hearing about while the context is fresh.
#[test]
fn no_capability_names_a_window_that_does_not_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let conf = std::fs::read_to_string(root.join("tauri.conf.json")).expect("readable");
    let conf: serde_json::Value = serde_json::from_str(&conf).expect("valid JSON");

    let declared = declared_windows(&conf);
    let permitted = permitted_windows(&root.join("capabilities"));

    // Windows created at runtime rather than declared up front. `settings` is
    // built by `WebviewWindowBuilder` when it is first opened, so it is
    // legitimately absent from the config.
    let runtime: BTreeSet<String> = ["settings".to_string()].into_iter().collect();

    let stray: Vec<&String> = permitted
        .difference(&declared)
        .filter(|label| !runtime.contains(*label))
        .collect();

    assert!(
        stray.is_empty(),
        "these capabilities name windows that are never created: {stray:?}"
    );
}
