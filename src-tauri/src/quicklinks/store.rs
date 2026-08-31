//! The quicklinks themselves.
//!
//! A JSON file, for the same reason snippets use one: a person has tens of
//! these, not tens of thousands, and one they can open and edit by hand is
//! worth more than an index.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// One quicklink.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Quicklink {
    /// Stable across renames, because frecency and the editor both refer to
    /// it and the name is the thing most likely to change.
    pub id: String,
    pub name: String,
    /// Where it goes, with placeholders still in it.
    ///
    /// A URL, a file path, or a folder. `{query}` is what makes one worth
    /// keeping rather than bookmarking.
    pub link: String,
    /// Typed in the launcher to reach it directly. Empty is fine; the name
    /// still finds it.
    pub keyword: String,
    /// Which application opens it, or empty for whatever the system uses.
    ///
    /// A path to an executable. The point is a link that always opens in the
    /// browser you meant, rather than whichever one is default this week.
    pub open_with: String,
    pub uses: u64,
    /// Unix seconds.
    pub created: i64,
}

impl Default for Quicklink {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            link: String::new(),
            keyword: String::new(),
            open_with: String::new(),
            uses: 0,
            created: 0,
        }
    }
}

impl Quicklink {
    /// Whether opening this needs something typed first.
    ///
    /// Only `{query}` counts. The other placeholders answer themselves from
    /// the clipboard or the clock, so a link using those opens immediately.
    pub fn needs_argument(&self) -> bool {
        crate::snippets::placeholder::mentions(&self.link, "query")
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("quicklinks.json")
}

pub fn data_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Everything on disk, newest first.
///
/// A missing or unreadable file is an empty list rather than an error: the
/// launcher has to open either way, and a quicklink nobody has created yet is
/// indistinguishable from a file that has not been written.
pub fn load(path: &Path) -> Vec<Quicklink> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(path: &Path, links: &[Quicklink]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(links).unwrap_or_else(|_| "[]".into());
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(target: &str) -> Quicklink {
        Quicklink {
            link: target.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn only_query_asks_for_something() {
        assert!(link("https://google.com/search?q={query}").needs_argument());
        // These answer themselves, so the link opens straight away.
        assert!(!link("https://example.com/{date}").needs_argument());
        assert!(!link("https://example.com/{clipboard}").needs_argument());
        assert!(!link("https://example.com").needs_argument());
    }

    #[test]
    fn a_missing_file_is_an_empty_list_not_an_error() {
        assert!(load(Path::new("does-not-exist.json")).is_empty());
    }
}
