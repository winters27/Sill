//! Does this build read the files already on this machine?
//!
//! The persistence layer grew a version and, for six of the eight stores, a
//! wrapper around the payload. Every file on disk was written before either
//! existed: `workspaces.json` is a bare list, `preferences.json` a bare
//! object, and neither carries a version.
//!
//! If any of them fails to read, the launcher starts with defaults and then
//! writes those defaults over somebody's settings, snippets, saved
//! arrangements and permission grants. That is the failure this whole item is
//! meant to prevent, so it is worth asserting against the real thing rather
//! than against a fixture written to match the new code.
//!
//! Ignored, because it reads a real profile directory and there is not one on
//! a build agent:
//!
//! ```text
//! cargo test --test reads_the_real_files -- --ignored --nocapture
//! ```
use std::path::PathBuf;

fn profile() -> PathBuf {
    PathBuf::from(std::env::var("APPDATA").expect("APPDATA")).join("app.winters.sill")
}

/// Copied, never read in place. A test must not be able to damage the thing it
/// is asking about.
fn copy_of(name: &str) -> Option<(tempfile::TempDir, PathBuf)> {
    let source = profile().join(name);
    if !source.is_file() {
        return None;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let into = dir.path().join(name);
    std::fs::copy(&source, &into).expect("copied");
    Some((dir, into))
}

#[test]
#[ignore]
fn the_preferences_on_this_machine_still_read() {
    let Some((_dir, path)) = copy_of("preferences.json") else {
        println!("no preferences.json on this machine");
        return;
    };

    let before = std::fs::read_to_string(&path).expect("readable");
    let prefs = sill_lib::preferences::Preferences::load(&path);

    assert!(
        !path.with_extension("json.broken").exists(),
        "the preferences were moved aside as unreadable"
    );

    // A value nobody would get by accident: the summon key this machine has
    // actually been configured with.
    let on_disk: serde_json::Value = serde_json::from_str(&before).expect("json");
    let summon = on_disk["hotkey"]["summon"].as_str().unwrap_or("Alt+Space");
    assert_eq!(prefs.hotkey.summon, summon, "the summon key was lost");

    println!(
        "preferences read: summon {summon}, {} bindings",
        prefs.bindings.len()
    );
}

#[test]
#[ignore]
fn the_saved_arrangements_on_this_machine_still_read() {
    let Some((_dir, path)) = copy_of("workspaces.json") else {
        println!("no workspaces.json on this machine");
        return;
    };

    let before: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("a list");

    let loaded = sill_lib::profiles_store::load(&path);

    assert_eq!(
        loaded.len(),
        before.len(),
        "a bare list written before the wrapper existed lost entries"
    );
    println!("arrangements read: {}", loaded.len());
}

#[test]
#[ignore]
fn the_conversations_on_this_machine_still_read() {
    let Some((_dir, path)) = copy_of("conversations.json") else {
        println!("no conversations.json on this machine");
        return;
    };

    let before: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("a list");

    let chat = sill_lib::ai::chat::Chat::new();
    chat.load(&path.parent().expect("parent").to_path_buf());

    assert_eq!(
        chat.summaries(0).len(),
        before.len(),
        "a bare list of conversations lost entries"
    );
    println!("conversations read: {}", before.len());
}

#[test]
#[ignore]
fn the_ranking_on_this_machine_still_reads() {
    let Some((_dir, path)) = copy_of("frecency.json") else {
        println!("no frecency.json on this machine");
        return;
    };

    let before: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("json");
    let entries = before["entries"].as_object().map(|m| m.len()).unwrap_or(0);

    let ranking = sill_lib::registry::Frecency::load(&path);

    assert_eq!(ranking.len(), entries, "the launch history was lost");
    println!("ranking read: {entries} entries");
}

/// Reading is half of it. This build must also write what it read.
///
/// The failure this catches is worse than a failed read: the file loads,
/// something is dropped on the way through, and the next save writes the
/// smaller thing over the original. Nobody notices until they look for the
/// setting that is gone.
#[test]
#[ignore]
fn what_is_written_back_is_what_was_read() {
    let Some((dir, path)) = copy_of("preferences.json") else {
        println!("no preferences.json on this machine");
        return;
    };

    let prefs = sill_lib::preferences::Preferences::load(&path);
    let written = dir.path().join("written.json");
    prefs.save(&written);

    let again = sill_lib::preferences::Preferences::load(&written);

    // Compared as values rather than field by field, so a section added later
    // is covered without anybody remembering to add it here.
    let first = serde_json::to_value(&prefs).expect("serialisable");
    let second = serde_json::to_value(&again).expect("serialisable");
    assert_eq!(first, second, "a save-then-load lost something");

    // And against what was on disk to begin with: every key that was there
    // must still be there. Extra keys are fine, they are new settings with
    // their defaults.
    let before: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("readable")).expect("json");
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&written).expect("readable")).expect("json");

    let mut lost = Vec::new();
    walk(&before, &after, String::new(), &mut lost);
    assert!(
        lost.is_empty(),
        "these settings did not survive a save: {lost:?}"
    );

    // A sealed secret is deliberately different every time it is written:
    // DPAPI adds entropy, so the same key encrypted twice is two different
    // blobs. Comparing the bytes would fail on a correct save, so the
    // question to ask is whether it still unseals to the same secret.
    let keys = |doc: &serde_json::Value| -> Vec<Option<String>> {
        doc["ai"]["providers"]
            .as_array()
            .map(|all| {
                all.iter()
                    .map(|p| {
                        let raw = p["apiKey"].as_str().unwrap_or_default();
                        if raw.is_empty() {
                            Some(String::new())
                        } else {
                            sill_lib::secrets::unseal(raw)
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let was = keys(&before);
    let now = keys(&after);
    assert_eq!(was.len(), now.len(), "a provider was lost");
    for (at, (was, now)) in was.iter().zip(now.iter()).enumerate() {
        // Compared as unsealed values, never printed: this is a real key.
        assert!(
            was.is_some(),
            "provider {at} could not be unsealed before the save"
        );
        assert_eq!(
            was, now,
            "provider {at} does not hold the same key after a save"
        );
    }
    println!("{} provider secrets unsealed to the same value", was.len());

    println!("preferences round-tripped, {} keys checked", count(&before));
}

/// Every leaf in `before` reached, with the same value, in `after`.
fn walk(before: &serde_json::Value, after: &serde_json::Value, at: String, lost: &mut Vec<String>) {
    match before {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                let path = if at.is_empty() {
                    key.clone()
                } else {
                    format!("{at}.{key}")
                };
                match after.get(key) {
                    Some(theirs) => walk(value, theirs, path, lost),
                    None => lost.push(path),
                }
            }
        }
        serde_json::Value::Array(items) => {
            // Recursed into, not compared whole: an array holding one sealed
            // secret would otherwise fail as a single unequal value and hide
            // whatever else in it had genuinely changed.
            let theirs = after.as_array().map(|a| a.len()).unwrap_or(0);
            if theirs != items.len() {
                lost.push(format!(
                    "{at} had {} entries and now has {theirs}",
                    items.len()
                ));
                return;
            }

            for (index, value) in items.iter().enumerate() {
                walk(value, &after[index], format!("{at}[{index}]"), lost);
            }
        }
        _ => {
            // A sealed secret is different bytes every write by design; it is
            // checked by unsealing instead, just below.
            let sealed = before
                .as_str()
                .map(|text| text.starts_with("dpapi:"))
                .unwrap_or(false);

            if !sealed && before != after {
                lost.push(format!("{at} ({before} became {after})"));
            }
        }
    }
}

fn count(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(fields) => {
            fields.values().map(count).sum::<usize>() + fields.len()
        }
        _ => 0,
    }
}
