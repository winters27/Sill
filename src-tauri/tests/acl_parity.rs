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
            out.extend(windows.iter().filter_map(|w| w.as_str().map(str::to_owned)));
        }
    }

    out
}

#[test]
fn every_declared_window_can_invoke() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let conf =
        std::fs::read_to_string(root.join("tauri.conf.json")).expect("tauri.conf.json is readable");
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

/// Every window label handed to `WebviewWindowBuilder`, found in the source.
///
/// Two of Sill's windows are not in the config at all: settings and the Ask
/// window are both built the first time somebody opens one, because a window
/// declared up front costs a renderer whether or not it is ever shown. This
/// reads them out of the code rather than keeping a list beside it.
/// The windows `lazy_windows` builds on demand, read from its own list.
///
/// These are created through a `builder(label)` helper rather than by naming a
/// label at a `WebviewWindowBuilder::new(` call, so scanning for that marker
/// cannot see them and reported `capture` and `dictation` as windows that do
/// not exist. They do exist; they are made the moment something asks.
///
/// Read out of `DEFERRED` rather than listed here, because that constant is
/// what `ensure` dispatches on, so the two cannot drift apart. Repeating the
/// labels here would be the third list this test's own comment warns about.
fn deferred(dir: &Path) -> BTreeSet<String> {
    const MARKER: &str = "DEFERRED: &[&str] = &[";

    let Ok(text) = std::fs::read_to_string(dir.join("lazy_windows.rs")) else {
        return BTreeSet::new();
    };

    let Some(at) = text.find(MARKER) else {
        return BTreeSet::new();
    };

    let rest = &text[at + MARKER.len()..];
    let list = rest.split(']').next().unwrap_or_default();

    list.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn built_at_runtime(dir: &Path) -> BTreeSet<String> {
    const MARKER: &str = "WebviewWindowBuilder::new(";

    let mut found = deferred(dir);

    for file in rust_files(dir) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };

        for (at, _) in text.match_indices(MARKER) {
            // The label is the first string literal after the opening bracket,
            // whether it is on the same line or three lines down.
            let rest = &text[at + MARKER.len()..];
            let Some(open) = rest.find('"') else { continue };
            let Some(close) = rest[open + 1..].find('"') else {
                continue;
            };

            found.insert(rest[open + 1..open + 1 + close].to_string());
        }
    }

    found
}

fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();

    let Ok(reading) = std::fs::read_dir(dir) else {
        return out;
    };

    for entry in reading.flatten() {
        let path = entry.path();

        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            out.push(path);
        }
    }

    out
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

    // Windows created at runtime rather than declared up front, read out of
    // the source that creates them rather than listed here. A list here would
    // be a third place that has to agree with the other two, and the way it
    // fails is a window that works perfectly while this test says it does not
    // exist.
    let runtime = built_at_runtime(&root.join("src"));

    let stray: Vec<&String> = permitted
        .difference(&declared)
        .filter(|label| !runtime.contains(*label))
        .collect();

    assert!(
        stray.is_empty(),
        "these capabilities name windows that are never created: {stray:?}"
    );
}

/// The windows an approval card can appear in are windows that exist.
///
/// `ai::approval::raise` opens the chat window when none of these is visible.
/// A label naming no window is never found visible, which is silent in exactly
/// the way this file is about: no error, nothing logged, and instead a second
/// window arriving in front of whatever somebody was doing every single time
/// the model asks to change something, while the card they already had was
/// perfectly readable. `get_webview_window` on a name that does not exist
/// simply answers `None`.
#[test]
fn every_window_an_approval_card_can_appear_in_exists() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let conf = std::fs::read_to_string(root.join("tauri.conf.json")).expect("readable");
    let conf: serde_json::Value = serde_json::from_str(&conf).expect("valid JSON");

    let mut real = declared_windows(&conf);
    real.extend(built_at_runtime(&root.join("src")));

    let surfaces = sill_lib::ai::approval::SURFACES;
    assert!(
        !surfaces.is_empty(),
        "a card would have nowhere to appear at all"
    );

    let missing: Vec<&&str> = surfaces
        .iter()
        .filter(|label| !real.contains(**label))
        .collect();

    assert!(
        missing.is_empty(),
        "an approval card is looking for windows that are never created: {missing:?}"
    );
}
