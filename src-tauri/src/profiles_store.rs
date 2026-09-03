//! Where the arrangements live, and the commands that use them.
//!
//! A JSON file, for the reason quicklinks and snippets use one: a person has a
//! handful of these, not thousands, and one they can open and read is worth
//! more than a database.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::json_store;
use crate::profiles::Profile;

pub fn path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("workspaces.json")
}

/// How the file is kept. See `json_store` for what each part buys.
///
/// `Profile` has required fields, so one arrangement written by a build whose
/// shape has since changed used to cost every other one. `load_list` drops
/// that one and keeps the rest.
const SCHEMA: json_store::Schema = json_store::Schema {
    version: 1,
    shape: json_store::Shape::Around,
    layout: json_store::Layout::Readable,
    unreadable: json_store::Unreadable::KeepAside,
    what: "workspaces",
};

/// Everything saved, in the order it was saved.
///
/// A missing file is an empty list rather than an error: not having made one
/// yet is the ordinary state.
pub fn load(file: &Path) -> Vec<Profile> {
    json_store::load_list(file, &SCHEMA)
}

pub fn save(file: &Path, profiles: &[Profile]) -> std::io::Result<()> {
    json_store::save_atomic(file, &profiles, &SCHEMA)
}

/// Adds one, replacing any with the same name.
///
/// Replacing rather than refusing, because saving a workspace twice is how
/// somebody adjusts one: they move a window, save again, and expect the second
/// one to be what they get back. A second profile with the same name is two
/// rows nobody can tell apart.
pub fn put(profiles: Vec<Profile>, adding: Profile) -> Vec<Profile> {
    let mut out: Vec<Profile> = profiles
        .into_iter()
        .filter(|one| !one.name.eq_ignore_ascii_case(&adding.name))
        .collect();

    out.push(adding);
    out
}

/// One row per saved arrangement, so they are searched like everything else.
///
/// Takes the file rather than the app, because the index is built on a
/// blocking task that has the paths it needs and no handle.
pub fn records(file: &Path) -> Vec<crate::registry::CommandRecord> {
    load(file)
        .into_iter()
        .map(|profile| {
            let count = profile.windows.len();

            crate::registry::CommandRecord {
                id: format!("workspace:{}", profile.name),
                extension: "workspace".to_string(),
                extension_title: "Workspaces".to_string(),
                command: profile.name.clone(),
                title: profile.name.clone(),
                subtitle: format!(
                    "Put {count} {} back where you left {}",
                    if count == 1 { "window" } else { "windows" },
                    if count == 1 { "it" } else { "them" }
                ),
                description: String::new(),
                mode: "workspace".to_string(),
                entrypoint: profile.name,
                keywords: vec![
                    "workspace".to_string(),
                    "layout".to_string(),
                    "arrange".to_string(),
                    "restore".to_string(),
                ],
                icon: None,
                toggle: None,
                panel: None,
                preferences: serde_json::Value::Null,
                manifest: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            windows: Vec::new(),
        }
    }

    /// Saving again is how a workspace is adjusted, so it replaces.
    #[test]
    fn saving_the_same_name_twice_leaves_one() {
        let all = put(vec![profile("Work"), profile("Play")], profile("Work"));

        assert_eq!(all.len(), 2);
        assert_eq!(all.iter().filter(|p| p.name == "Work").count(), 1);
    }

    /// Names are for people, and people do not think in case.
    #[test]
    fn a_name_that_differs_only_in_case_is_the_same_name() {
        let all = put(vec![profile("Work")], profile("work"));

        assert_eq!(all.len(), 1, "two rows nobody could tell apart");
        assert_eq!(all[0].name, "work", "and the newer one wins");
    }

    #[test]
    fn a_new_name_is_added_rather_than_replacing() {
        let all = put(vec![profile("Work")], profile("Evening"));
        assert_eq!(all.len(), 2);
    }

    /// Every workspaces file on disk is a bare list with no version in it.
    #[test]
    fn a_file_written_before_versioning_still_reads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("workspaces.json");
        std::fs::write(&file, r#"[{"name":"Work","windows":[]}]"#).expect("writes");

        let all = load(&file);

        assert_eq!(all.len(), 1, "the file this build inherits has to read");
        assert_eq!(all[0].name, "Work");
    }

    /// `Profile` has required fields, so one bad entry used to cost them all.
    #[test]
    fn one_arrangement_that_cannot_be_read_does_not_take_the_others() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("workspaces.json");
        // The middle one has no `name`, which is what a hand edit or a field
        // added in a later version looks like from here.
        std::fs::write(
            &file,
            r#"[{"name":"Work","windows":[]},{"windows":[]},{"name":"Evening","windows":[]}]"#,
        )
        .expect("writes");

        let all = load(&file);

        assert_eq!(all.len(), 2, "the readable arrangements survive");
        assert_eq!(all[0].name, "Work");
        assert_eq!(all[1].name, "Evening");
    }

    /// This file was written in place, so a torn write lost every arrangement.
    #[test]
    fn saving_stages_the_write_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("workspaces.json");

        save(&file, &[profile("Work")]).expect("saves");

        assert_eq!(load(&file).len(), 1);
        assert!(!file.with_extension("json.partial").exists());
    }
}
