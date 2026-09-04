//! And nothing more than it needs.
//!
//! `acl_parity` asks whether a window can invoke at all. This asks the opposite
//! question, which is the one a capability file is actually for: whether a
//! window has been handed something it never calls.
//!
//! One file granting one list to all seven windows meant the dictation pill,
//! which reaches `invoke` and `listen` and nothing else, could register global
//! shortcuts, build webviews and open any address the shell would take. That is
//! not a theoretical grant. Tauri decides by window LABEL, so it holds wherever
//! the page navigates inside the bundle, and a renderer is the part of this
//! application that runs other people's markdown, other people's extension UI
//! and whatever a model wrote.
//!
//! The expectation is DERIVED rather than listed. A table of which window may
//! do what, kept beside the capability files, is the shape of mistake this
//! codebase keeps paying for: two lists that must agree, with nothing making
//! them agree. So the import graph is walked from each window's route and the
//! grant is compared against what that route can actually reach.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A window and the route files it loads.
///
/// Which route a label loads is a fact about how the window is built, in
/// `lazy_windows.rs` and `commands/settings.rs`. Naming it here is one line
/// against a resolver nobody would read.
const ROUTES: &[(&str, &[&str])] = &[
    ("main", &["+page.svelte"]),
    ("traymenu", &["traymenu/+page.svelte"]),
    ("ask", &["ask/+page.svelte"]),
    ("settings", &["settings/+page.svelte", "settings/+page.ts"]),
    ("capture", &["capture/+page.svelte"]),
    ("dictation", &["dictation/+page.svelte"]),
    ("markup", &["markup/+page.svelte"]),
    ("note", &["note/+page.svelte"]),
];

/// A permission, and the thing in the frontend that needs it.
///
/// The marker is what the call looks like in the source, because that is what
/// somebody adding the call writes. Every one was checked against the tree by
/// hand first: each appears in exactly the places it means and nowhere else,
/// which is why a substring is enough and a parser is not.
///
/// The last three are granted to nobody on purpose. They were in the old
/// single capability and no window has ever imported them.
const NEEDS: &[(&str, &str)] = &[
    (
        "clipboard-manager:allow-write-text",
        "@tauri-apps/plugin-clipboard-manager",
    ),
    ("dialog:allow-open", "@tauri-apps/plugin-dialog"),
    ("core:window:allow-minimize", ".minimize()"),
    ("core:window:allow-close", ".close()"),
    ("core:window:allow-start-dragging", "data-tauri-drag-region"),
    ("core:window:allow-set-size", ".setSize("),
    ("core:window:allow-center", ".center()"),
    ("core:window:allow-hide", ".hide()"),
    (
        "global-shortcut:default",
        "@tauri-apps/plugin-global-shortcut",
    ),
    ("opener:default", "@tauri-apps/plugin-opener"),
    (
        "core:webview:allow-create-webview-window",
        "@tauri-apps/api/webviewWindow",
    ),
];

/// Which windows each capability file grants each permission to.
fn granted(dir: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for entry in std::fs::read_dir(dir)
        .expect("capabilities is readable")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }

        let text = std::fs::read_to_string(&path).expect("readable");
        let value: serde_json::Value = serde_json::from_str(&text)
            .unwrap_or_else(|_| panic!("{} is valid JSON", path.display()));

        let windows: Vec<String> = value
            .get("windows")
            .and_then(|w| w.as_array())
            .map(|w| {
                w.iter()
                    .filter_map(|w| w.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        let Some(permissions) = value.get("permissions").and_then(|p| p.as_array()) else {
            continue;
        };

        for permission in permissions.iter().filter_map(|p| p.as_str()) {
            out.entry(permission.to_string())
                .or_default()
                .extend(windows.iter().cloned());
        }
    }

    out
}

/// Every file a route pulls in, following imports until nothing new appears.
///
/// Deliberately naive: it reads import specifiers by text rather than parsing
/// the module, because a `$lib/x` written in a comment that happens to resolve
/// only ever makes the closure LARGER. A closure that is too large can make
/// this test demand a permission that is not needed, which somebody notices.
/// It cannot hide one, which is the direction that matters.
fn closure(root: &Path, entries: &[&str]) -> BTreeSet<PathBuf> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();

    // The root layout loads under every route, so every closure contains it.
    let mut todo: Vec<PathBuf> = vec![root.join("routes/+layout.ts")];
    todo.extend(entries.iter().map(|entry| root.join("routes").join(entry)));

    while let Some(file) = todo.pop() {
        if !file.is_file() || !seen.insert(file.clone()) {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };

        for specifier in specifiers(&text) {
            if let Some(found) = resolve(root, &file, &specifier) {
                todo.push(found);
            }
        }
    }

    seen
}

/// The quoted part of every `from "..."` and `import("...")`.
fn specifiers(text: &str) -> Vec<String> {
    let mut out = Vec::new();

    for marker in [" from \"", "import(\""] {
        for (at, _) in text.match_indices(marker) {
            let rest = &text[at + marker.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
    }

    out
}

/// A specifier as a file on disk, or nothing when it names a package.
fn resolve(root: &Path, from: &Path, specifier: &str) -> Option<PathBuf> {
    let base = if let Some(rest) = specifier.strip_prefix("$lib/") {
        root.join("lib").join(rest)
    } else if specifier.starts_with('.') {
        from.parent()?.join(specifier)
    } else {
        // A package. Nothing of ours lives inside one.
        return None;
    };

    for suffix in ["", ".ts", ".svelte", ".js", "/index.ts"] {
        let candidate = PathBuf::from(format!("{}{suffix}", base.display()));
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// The windows whose route can reach `marker`.
fn reaching(root: &Path, marker: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();

    for (label, entries) in ROUTES {
        let reaches = closure(root, entries).iter().any(|file| {
            std::fs::read_to_string(file)
                .map(|text| text.contains(marker))
                .unwrap_or(false)
        });

        if reaches {
            out.insert((*label).to_string());
        }
    }

    out
}

#[test]
fn no_window_is_granted_something_its_route_never_calls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src = root.join("..").join("src");
    let grants = granted(&root.join("capabilities"));

    // Were the walker to resolve nothing, this test would pass by concluding
    // that every permission is needed by nobody and granted to nobody, which
    // is the one way it could be silently useless.
    assert!(
        closure(&src, &["+page.svelte"]).len() > 10,
        "the import walker found almost nothing, so it is not checking anything"
    );

    let mut wrong = Vec::new();

    for (permission, marker) in NEEDS {
        let needs = reaching(&src, marker);
        let has = grants.get(*permission).cloned().unwrap_or_default();

        for label in has.difference(&needs) {
            wrong.push(format!(
                "{label} is granted {permission} but nothing its route imports contains \
                 `{marker}`, so it is reach a compromised renderer gets for free"
            ));
        }

        for label in needs.difference(&has) {
            wrong.push(format!(
                "{label} calls `{marker}` but is not granted {permission}, so that call is \
                 denied at the ACL layer in silence and the control will look dead"
            ));
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The page is served with a content security policy at all.
///
/// `null` was the value the template ships with, and it means a renderer that
/// is talked into evaluating a string can fetch from anywhere. The markdown a
/// model writes and the interface an extension draws both render in there.
#[test]
fn the_windows_are_served_with_a_content_security_policy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let conf = std::fs::read_to_string(root.join("tauri.conf.json")).expect("readable");
    let conf: serde_json::Value = serde_json::from_str(&conf).expect("valid JSON");

    let csp = conf
        .pointer("/app/security/csp")
        .and_then(|csp| csp.as_str())
        .expect("a content security policy is set");

    for directive in [
        "default-src 'self'",
        "script-src 'self'",
        "object-src 'none'",
        "frame-src 'none'",
        "base-uri 'self'",
        "form-action 'none'",
    ] {
        assert!(
            csp.contains(directive),
            "the policy is missing `{directive}`: {csp}"
        );
    }

    // The two Tauri needs on Windows, and forgetting either is an application
    // whose every command fails with nothing in the log.
    assert!(
        csp.contains("http://ipc.localhost"),
        "IPC would be blocked: {csp}"
    );

    // A nonce and a hash are added by Tauri for its own injected script and
    // for SvelteKit's bootstrap. `unsafe-inline` and `unsafe-eval` would undo
    // the whole directive, and neither is needed.
    assert!(
        !csp.contains("script-src 'self' 'unsafe-inline'") && !csp.contains("'unsafe-eval'"),
        "scripts may be inlined or evaluated, which is what the policy is for: {csp}"
    );
}
