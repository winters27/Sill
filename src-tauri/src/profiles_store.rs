//! Where the arrangements live, and the commands that use them.
//!
//! A JSON file, for the reason quicklinks and snippets use one: a person has a
//! handful of these, not thousands, and one they can open and read is worth
//! more than a database.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::profiles::Profile;

pub fn path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("workspaces.json")
}

/// Everything saved, in the order it was saved.
///
/// A missing or unreadable file is an empty list rather than an error: not
/// having made one yet is the ordinary state.
pub fn load(file: &Path) -> Vec<Profile> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(file: &Path, profiles: &[Profile]) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(profiles).unwrap_or_else(|_| "[]".into());
    std::fs::write(file, text)
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
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> Profile {
        Profile { name: name.to_string(), windows: Vec::new() }
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
}
