//! The quicklinks themselves.
//!
//! A JSON file, for the same reason snippets use one: a person has tens of
//! these, not tens of thousands, and one they can open and edit by hand is
//! worth more than an index.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::json_store;

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

/// How the file is kept. See `json_store` for what each part buys.
///
/// Readable, because a quicklink is a URL with a placeholder in it and editing
/// twenty of them by hand is faster than clicking through twenty forms.
const SCHEMA: json_store::Schema = json_store::Schema {
    version: 1,
    shape: json_store::Shape::Around,
    layout: json_store::Layout::Readable,
    unreadable: json_store::Unreadable::KeepAside,
    what: "quicklinks",
};

/// Everything on disk, newest first.
///
/// A missing file is an empty list rather than an error: the launcher has to
/// open either way, and a quicklink nobody has created yet is
/// indistinguishable from a file that has not been written.
pub fn load(path: &Path) -> Vec<Quicklink> {
    json_store::load_list(path, &SCHEMA)
}

pub fn save(path: &Path, links: &[Quicklink]) -> std::io::Result<()> {
    json_store::save_atomic(path, &links, &SCHEMA)
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

    fn named(name: &str) -> Quicklink {
        Quicklink {
            id: name.to_string(),
            name: name.to_string(),
            link: "https://example.com".to_string(),
            ..Default::default()
        }
    }

    /// Every quicklinks file on disk is a bare list with no version in it.
    #[test]
    fn a_file_written_before_versioning_still_reads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("quicklinks.json");
        std::fs::write(
            &file,
            r#"[{"id":"a","name":"Search","link":"https://example.com/{query}"}]"#,
        )
        .expect("writes");

        let all = load(&file);

        assert_eq!(all.len(), 1, "the file this build inherits has to read");
        assert_eq!(all[0].name, "Search");
    }

    /// This file was written in place, so a torn write lost every quicklink.
    #[test]
    fn saving_stages_the_write_and_leaves_nothing_behind() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("quicklinks.json");

        save(&file, &[named("one")]).expect("saves");

        assert_eq!(load(&file).len(), 1);
        assert!(
            !file.with_extension("json.partial").exists(),
            "the staging file was left on disk"
        );
    }

    /// Notepad writes a byte order mark, and this used to empty the file.
    #[test]
    fn a_file_saved_with_a_byte_order_mark_still_reads() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("quicklinks.json");
        std::fs::write(
            &file,
            "\u{feff}[{\"id\":\"a\",\"name\":\"Hand edited\",\"link\":\"https://example.com\"}]",
        )
        .expect("writes");

        let all = load(&file);

        assert_eq!(
            all.len(),
            1,
            "three bytes of encoding threw away every quicklink"
        );
        assert_eq!(all[0].name, "Hand edited");
    }

    /// A file this build cannot read is kept, not left for the next save.
    #[test]
    fn an_unreadable_file_is_kept_aside() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("quicklinks.json");
        std::fs::write(&file, "[ not json").expect("writes");

        assert!(load(&file).is_empty());
        assert_eq!(
            std::fs::read_to_string(file.with_extension("json.broken")).expect("kept aside"),
            "[ not json"
        );
    }
}
