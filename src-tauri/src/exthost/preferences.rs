//! What somebody has set on an installed extension.
//!
//! A manifest declares preferences and gives some of them a default, and until
//! now the default was the whole story: `getPreferenceValues()` answered with
//! what the author wrote and there was no way to change it. Half the store
//! needs an API key to do anything at all, so half the store installed and
//! then said "please set your token in preferences" about a screen that did
//! not exist.
//!
//! ## Where a value belongs
//!
//! Raycast scopes a preference by where it was declared. One at the top of the
//! manifest is the extension's and every command shares it, which is what makes
//! one API key serve nine commands; one inside a command is that command's
//! alone. The two are kept apart here for the same reason, keyed by
//! `<extension>` and `<extension>:<command>`, and [`effective`] is what puts
//! them back together in that order.
//!
//! ## Why the file is its own
//!
//! Not `preferences.json`. A `password` preference holds an API key, and the
//! settings file's sealing is a hand-written list of paths (`SEALED`) that
//! cannot name a key nobody has declared yet. An extension's credential
//! written there would be in plain text in a synced file, silently, which is
//! the exact failure that list exists to prevent.
//!
//! So this file seals on the way in instead, deciding by the declared type,
//! and unseals on the way out by recognising what it wrote. Everything else
//! about keeping the file (staging, renaming, refusing a newer version, moving
//! an unreadable one aside) is [`crate::json_store`]'s, because a ninth
//! hand-rolled load and save pair is how the eighth one drifted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::extension_install::Preference;
use crate::json_store::{Layout, Schema, Shape, Unreadable};

/// How this file is kept.
const SCHEMA: Schema = Schema {
    version: 1,
    // The payload is a map, which cannot carry a field beside itself.
    shape: Shape::Around,
    // Small, and something somebody may well open to see what an extension
    // was given.
    layout: Layout::Readable,
    // Somebody typed these. Falling back to defaults and writing them over the
    // top would turn one torn write into every extension losing its token.
    unreadable: Unreadable::KeepAside,
    what: "extension preferences",
};

/// Every value somebody has set, by scope.
///
/// A map of maps rather than a struct, and deliberately: the keys are
/// extension names nobody can know in advance, and there is no field here to
/// forget `#[serde(default)]` on. That is the shape that cannot repeat what
/// `P0-03` had to fix three times.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Values(pub BTreeMap<String, Map<String, Value>>);

/// The scope key for an extension's own preferences.
pub fn extension_scope(extension: &str) -> String {
    extension.to_string()
}

/// The scope key for one command's own preferences.
pub fn command_scope(extension: &str, command: &str) -> String {
    format!("{extension}:{command}")
}

impl Values {
    /// What is set in one scope.
    pub fn in_scope(&self, scope: &str) -> Option<&Map<String, Value>> {
        self.0.get(scope)
    }

    /// Sets one value, sealing it when its declaration says it is a secret.
    ///
    /// The declaration decides rather than the name, because `token` and
    /// `apiKey` are conventions and `type: "password"` is a statement.
    /// Anything without a declaration is stored as it arrived: a value for a
    /// preference the manifest does not have is not a credential, it is
    /// something left over from a version that did.
    pub fn set(&mut self, scope: &str, name: &str, value: Value, declared: &[Preference]) {
        let secret = declared
            .iter()
            .any(|it| it.name == name && it.kind.as_deref() == Some("password"));

        let stored = match (secret, value.as_str()) {
            (true, Some(text)) if !text.is_empty() => crate::secrets::seal(text)
                .map(Value::String)
                // Sealing can fail, and writing the key in the clear because
                // it did would be the worst of the three answers. Dropping it
                // means the field is empty and somebody types it again.
                .unwrap_or(Value::Null),
            _ => value,
        };

        let holding = self.0.entry(scope.to_string()).or_default();

        // An empty answer is "unset", not "set to nothing". That is what puts
        // a preference back to its manifest default rather than overriding it
        // with a blank, and it is the only way to undo a value from a screen
        // made of text fields.
        match &stored {
            Value::String(text) if text.is_empty() => holding.remove(name),
            Value::Null => holding.remove(name),
            _ => holding.insert(name.to_string(), stored),
        };

        if holding.is_empty() {
            self.0.remove(scope);
        }
    }

    /// Everything one extension holds, in every scope belonging to it.
    ///
    /// Matched on the whole name before the colon rather than on a prefix,
    /// because an extension called `git` and one called `github` share one and
    /// removing the first would take half of the second with it. The same trap
    /// `without_extension` names.
    pub fn forget(&mut self, extension: &str) -> bool {
        let before = self.0.len();
        self.0
            .retain(|scope, _| scope != extension && !scope.starts_with(&format!("{extension}:")));
        before != self.0.len()
    }
}

/// Where the file is.
pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("extension-preferences.json")
}

/// Reads it.
pub fn load(data_dir: &Path) -> Values {
    crate::json_store::load(&path(data_dir), &SCHEMA)
}

/// Writes it, staged and renamed.
pub fn save(data_dir: &Path, values: &Values) -> Result<(), String> {
    crate::json_store::save_atomic(&path(data_dir), values, &SCHEMA)
        .map_err(|err| format!("could not save extension preferences: {err}"))
}

/// What `getPreferenceValues()` should answer with for one command.
///
/// Three layers, lowest first: what the manifest defaults to, then what
/// somebody set for the whole extension, then what they set for this command.
/// That is Raycast's own order, and it is what lets one API key at the top of
/// a manifest serve every command while a command that redeclares it keeps its
/// own.
///
/// Sealed values are opened here, which is the only place they are: the file
/// holds them encrypted and the worker is handed the plain text, because an
/// extension asking for its own API key cannot use a DPAPI blob.
pub fn effective(
    defaults: &Value,
    extension: Option<&Map<String, Value>>,
    command: Option<&Map<String, Value>>,
) -> Value {
    let mut answer = defaults.as_object().cloned().unwrap_or_else(Map::new);

    for layer in [extension, command].into_iter().flatten() {
        for (name, value) in layer {
            answer.insert(name.clone(), opened(value));
        }
    }

    Value::Object(answer)
}

/// A stored value as the extension should see it.
fn opened(value: &Value) -> Value {
    match value.as_str() {
        Some(text) if crate::secrets::is_sealed(text) => crate::secrets::unseal(text)
            .map(Value::String)
            // A blob this machine cannot open is one another machine sealed,
            // which happens when the settings folder is synced. Empty rather
            // than the blob: an extension handed `dpapi:v1:...` as its API key
            // sends it to somebody's server.
            .unwrap_or_else(|| Value::String(String::new())),
        _ => value.clone(),
    }
}

/// Whether a required preference has no value, for every one that is required.
///
/// Named rather than counted. Raycast refuses to launch a command whose
/// required preference is unset and says which one; Sill has been launching it
/// and letting the extension throw on an undefined, which reads as the
/// extension being broken.
pub fn missing_required(declared: &[Preference], effective: &Value) -> Vec<String> {
    declared
        .iter()
        .filter(|preference| preference.required)
        .filter(|preference| match effective.get(&preference.name) {
            None | Some(Value::Null) => true,
            Some(Value::String(text)) => text.is_empty(),
            _ => false,
        })
        .map(|preference| {
            preference
                .title
                .clone()
                .or_else(|| preference.label.clone())
                .unwrap_or_else(|| preference.name.clone())
        })
        .collect()
}

/// Where one extension may write files of its own.
///
/// `environment.supportPath` in the API, and it was the empty string, so an
/// extension that keeps a cache or a database had nowhere to put it. Outside
/// the installed directory on purpose: installing clears its destination, so a
/// support folder inside it would be somebody's data deleted by an update.
pub fn support_path(data_dir: &Path, extension: &str) -> PathBuf {
    data_dir.join("extension-support").join(extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn declaring(json: &str) -> Vec<Preference> {
        serde_json::from_str(json).expect("preferences parse")
    }

    /// The order the three layers apply in.
    #[test]
    fn a_command_setting_beats_the_extensions_which_beats_the_default() {
        let defaults = json!({ "key": "from the manifest", "other": 1 });

        let extension: Map<String, Value> =
            serde_json::from_value(json!({ "key": "set once for everything" })).unwrap();
        let command: Map<String, Value> =
            serde_json::from_value(json!({ "key": "set for this command" })).unwrap();

        assert_eq!(
            effective(&defaults, Some(&extension), Some(&command))["key"],
            "set for this command"
        );
        assert_eq!(
            effective(&defaults, Some(&extension), None)["key"],
            "set once for everything"
        );
        assert_eq!(effective(&defaults, None, None)["key"], "from the manifest");
        assert_eq!(
            effective(&defaults, Some(&extension), Some(&command))["other"],
            1,
            "a default nobody overrode is still there"
        );
    }

    /// The whole point of the file: what is typed survives being forgotten.
    ///
    /// Written, dropped, read back from the disk it was written to. An
    /// in-memory assertion would pass on a save that never reached a file,
    /// which is the failure this is guarding against.
    #[test]
    fn a_value_survives_a_round_trip_through_the_disk() {
        let dir = tempfile::tempdir().expect("temp dir");

        let declared = declaring(r#"[{ "name": "host", "type": "textfield" }]"#);
        let mut values = Values::default();
        values.set(
            &extension_scope("demo"),
            "host",
            json!("https://example.test"),
            &declared,
        );
        save(dir.path(), &values).expect("it saves");

        // The in-memory copy goes, so what is asserted below can only have
        // come from the file.
        drop(values);

        let read = load(dir.path());
        assert_eq!(
            read.in_scope("demo").and_then(|it| it.get("host")),
            Some(&json!("https://example.test"))
        );
    }

    /// A credential does not sit in the file in plain text.
    #[test]
    fn a_password_preference_is_sealed_on_its_way_to_disk() {
        let dir = tempfile::tempdir().expect("temp dir");

        let declared = declaring(
            r#"[
                { "name": "token", "type": "password" },
                { "name": "host", "type": "textfield" }
            ]"#,
        );

        let mut values = Values::default();
        values.set(
            &extension_scope("demo"),
            "token",
            json!("super-secret-value"),
            &declared,
        );
        values.set(
            &extension_scope("demo"),
            "host",
            json!("example.test"),
            &declared,
        );
        save(dir.path(), &values).expect("it saves");

        let written = std::fs::read_to_string(path(dir.path())).expect("the file is there");
        assert!(
            !written.contains("super-secret-value"),
            "an extension's API key is in the settings folder in plain text:\n{written}"
        );
        assert!(
            written.contains("example.test"),
            "and an ordinary field is not encrypted for nothing"
        );
    }

    /// What the worker is handed is the key, not the blob it was stored as.
    #[test]
    fn a_sealed_value_is_opened_before_an_extension_sees_it() {
        let Some(sealed) = crate::secrets::seal("the real token") else {
            // Sealing is Windows-only. There is nothing to open elsewhere.
            return;
        };

        let held: Map<String, Value> =
            serde_json::from_value(json!({ "token": sealed })).expect("a map");

        assert_eq!(
            effective(&json!({}), Some(&held), None)["token"],
            "the real token"
        );
    }

    /// A blob from another machine must not be handed over as if it were the
    /// value. An extension would send it somewhere.
    #[test]
    fn a_blob_this_machine_cannot_open_reads_as_empty() {
        let held: Map<String, Value> =
            serde_json::from_value(json!({ "token": "dpapi:v1:AQAAAA==" })).expect("a map");

        assert_eq!(effective(&json!({}), Some(&held), None)["token"], "");
    }

    /// Clearing a field puts the manifest's default back rather than blanking
    /// it, which is the only way to undo a value on a screen of text fields.
    #[test]
    fn emptying_a_field_unsets_it_rather_than_storing_nothing() {
        let declared = declaring(r#"[{ "name": "host", "type": "textfield" }]"#);

        let mut values = Values::default();
        values.set(&extension_scope("demo"), "host", json!("typed"), &declared);
        values.set(&extension_scope("demo"), "host", json!(""), &declared);

        assert!(values.in_scope("demo").is_none(), "the scope is empty too");
        assert_eq!(
            effective(&json!({ "host": "the default" }), None, None)["host"],
            "the default"
        );
    }

    /// Removing an extension takes its settings, and only its settings.
    #[test]
    fn forgetting_one_extension_matches_the_whole_name_not_a_prefix() {
        let declared = declaring(r#"[{ "name": "k" }]"#);

        let mut values = Values::default();
        for scope in [
            extension_scope("git"),
            command_scope("git", "log"),
            extension_scope("github"),
            command_scope("github", "issues"),
        ] {
            values.set(&scope, "k", json!(1), &declared);
        }

        assert!(values.forget("git"));

        let left: Vec<&String> = values.0.keys().collect();
        assert_eq!(
            left,
            ["github", "github:issues"],
            "an extension whose name begins another's went with it"
        );
    }

    #[test]
    fn forgetting_something_that_set_nothing_changes_nothing() {
        let mut values = Values::default();
        assert!(!values.forget("absent"));
    }

    /// A required preference nobody filled in is named, not counted.
    #[test]
    fn a_required_preference_with_no_value_is_named() {
        let declared = declaring(
            r#"[
                { "name": "token", "type": "password", "required": true, "title": "API Key" },
                { "name": "empty", "type": "textfield", "required": true },
                { "name": "set", "type": "textfield", "required": true },
                { "name": "optional", "type": "textfield" }
            ]"#,
        );

        let effective = json!({ "empty": "", "set": "a value" });

        assert_eq!(
            missing_required(&declared, &effective),
            vec!["API Key".to_string(), "empty".to_string()],
            "a title is used when there is one, and an empty string is not a value"
        );
    }

    /// An update clears the extension's directory, so this must not be in it.
    #[test]
    fn the_support_folder_is_outside_the_one_an_update_clears() {
        let data = Path::new("C:\\data");
        let support = support_path(data, "demo");
        let installed = crate::store::extensions_home(data).join("demo");

        assert!(
            !support.starts_with(&installed),
            "{} is inside {}, so an update would delete it",
            support.display(),
            installed.display()
        );
    }
}
