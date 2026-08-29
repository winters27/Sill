//! What a thing in Sill *is*, as opposed to how it happens to be spelled.
//!
//! Everything the launcher can act on was a `CommandRecord` with a `mode`
//! field holding one of eleven strings, and every place that had to behave
//! differently per kind compared those strings by hand. That works, and it
//! stops working the moment two things need to be true at once: that a file
//! and a clipboard entry are different kinds, and that "copy the path" applies
//! to both.
//!
//! A kind is not a mode. `app` and `exe` launch identically and rank
//! differently, so they are one kind here and stay two modes in the index.
//! `quicklink` and `quicklink-arg` are the same kind of thing with a hole in
//! one of them. Keeping the two vocabularies separate is deliberate: the index
//! records how an entry was discovered, and this records what can be done
//! with it.

use serde::Serialize;

use crate::registry::CommandRecord;

/// What kind of thing an action is being asked to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    /// An installed application, or a bare executable found on `%PATH%`.
    Application,
    /// A file on disk.
    File,
    /// A folder on disk.
    Folder,
    /// A Raycast-compatible extension command.
    ExtensionCommand,
    /// A Windows settings page or Control Panel applet.
    SystemSetting,
    /// One of Sill's own settings.
    Setting,
    /// Something the launcher does to itself.
    Builtin,
    Snippet,
    Quicklink,
    /// A calculator result, which exists only for as long as it is on screen.
    Answer,
    /// A row of clipboard history.
    ClipboardEntry,
    /// Loose text: a selection, or whatever an action produced.
    Text,
    /// A window that is open right now.
    ///
    /// The first kind that is not in the index and never will be. The desktop
    /// changes faster than any scan could keep up with, so a window is
    /// enumerated at the moment it is searched for and its identity is a
    /// handle that stops being valid when it closes.
    Window,
}

impl ObjectKind {
    /// Every kind there is.
    ///
    /// Written out rather than derived, so adding a variant fails to compile
    /// here until someone has thought about whether the tests that walk this
    /// list still hold for it.
    pub const ALL: &'static [Self] = &[
        Self::Application,
        Self::File,
        Self::Folder,
        Self::ExtensionCommand,
        Self::SystemSetting,
        Self::Setting,
        Self::Builtin,
        Self::Snippet,
        Self::Quicklink,
        Self::Answer,
        Self::ClipboardEntry,
        Self::Text,
        Self::Window,
    ];

    /// The kind behind an index entry's `mode`.
    ///
    /// `None` for a mode nothing knows about, which is a build newer than this
    /// one having written the index. Better to leave such an entry inert than
    /// to guess a kind and run the wrong action on it.
    pub fn from_mode(mode: &str) -> Option<Self> {
        Some(match mode {
            // Two modes, one kind. They are told apart so ranking can push a
            // command-line tool below a real application, which is a fact
            // about search rather than about what you can do with it.
            "app" | "exe" => Self::Application,
            "file" => Self::File,
            "folder" => Self::Folder,
            "view" | "no-view" => Self::ExtensionCommand,
            "setting" => Self::SystemSetting,
            "sill-setting" => Self::Setting,
            "builtin" => Self::Builtin,
            "snippet" => Self::Snippet,
            // The argument version is the same kind of thing; whether it stops
            // to ask is a property of the link, not of what it is.
            "quicklink" | "quicklink-arg" => Self::Quicklink,
            "answer" => Self::Answer,
            // Not index entries. A clipboard row and a piece of loose text
            // reach an action through the window rather than through a scan,
            // and they still need a name to be dispatched on.
            "clipboard" => Self::ClipboardEntry,
            "text" => Self::Text,
            "window" => Self::Window,
            _ => return None,
        })
    }

    /// Whether acting on this is likely to hand the screen to something else.
    ///
    /// The launcher steps aside for these rather than sitting on top of what
    /// it just opened, which is the single most irritating thing an
    /// always-on-top window can do.
    pub fn hands_over_the_screen(self) -> bool {
        matches!(
            self,
            Self::Application
                | Self::File
                | Self::Folder
                | Self::SystemSetting
                | Self::Quicklink
                // Switching to a window is the clearest case of all: the whole
                // point is that something else ends up in front.
                | Self::Window
        )
    }
}

/// One thing, and enough to act on it.
///
/// Deliberately flat. Every kind above carries exactly one meaningful string
/// today: a path, a panel name, a snippet id, a row id, a result. A payload
/// enum would be inventing structure that nothing yet needs, and the shape of
/// the structure that *is* eventually needed is not knowable from here.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Object {
    pub kind: ObjectKind,
    /// Stable identity, used for ranking history and to find it again.
    pub id: String,
    /// What an action acts on: a path, a panel, a stored id, a value.
    pub target: String,
    /// What to call it when reporting what happened.
    pub title: String,
    /// The index mode this came from, when it came from the index.
    ///
    /// Carried because two modes can share a kind and an action occasionally
    /// has to tell them apart. An extension command is loaded as a view or run
    /// without one, and only the mode says which.
    pub mode: String,
}

impl Object {
    /// The object an open window stands for.
    ///
    /// The title is what the window calls itself and the application name goes
    /// beside it, because a window titled "Untitled" is not findable and a
    /// window titled "index.ts" could belong to any of four editors.
    pub fn from_window(window: &crate::windowing::Window) -> Self {
        Self {
            kind: ObjectKind::Window,
            id: format!("window:{}", window.id),
            // The handle, as the string every action parses back. Not the
            // title: two windows of one application routinely share a title,
            // and acting on the wrong one is indistinguishable from a bug.
            target: window.id.to_string(),
            title: if window.title.is_empty() {
                window.app.clone()
            } else {
                window.title.clone()
            },
            mode: "window".to_string(),
        }
    }

    /// The object an index entry stands for.
    pub fn from_record(record: &CommandRecord) -> Option<Self> {
        Some(Self {
            kind: ObjectKind::from_mode(&record.mode)?,
            id: record.id.clone(),
            target: record.entrypoint.clone(),
            title: record.title.clone(),
            mode: record.mode.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_the_index_can_hold_has_a_kind() {
        // The list is exhaustive on purpose. A mode added to the index with no
        // kind here produces an entry nothing can act on, and the symptom is a
        // row that does nothing when you press Enter.
        for mode in [
            "app",
            "exe",
            "file",
            "folder",
            "view",
            "no-view",
            "setting",
            "sill-setting",
            "builtin",
            "snippet",
            "quicklink",
            "quicklink-arg",
            "answer",
        ] {
            assert!(
                ObjectKind::from_mode(mode).is_some(),
                "{mode} has no kind, so nothing can be done with it"
            );
        }
    }

    #[test]
    fn a_window_is_identified_by_its_handle_and_not_its_title() {
        // Two windows of one application share a title constantly: two File
        // Explorer windows on the same folder, two documents called Untitled.
        // Identifying by title closes the wrong one.
        let a = crate::windowing::Window {
            id: 1234,
            title: "Downloads - File Explorer".into(),
            app: "File Explorer".into(),
            app_path: "C:/Windows/explorer.exe".into(),
            pid: 42,
            minimized: false,
            maximized: false,
            rect: crate::windowing::Rect::new(0, 0, 100, 100),
            monitor: 0,
        };
        let b = crate::windowing::Window {
            id: 5678,
            ..a.clone()
        };

        let first = Object::from_window(&a);
        let second = Object::from_window(&b);

        assert_eq!(first.title, second.title, "the titles really are the same");
        assert_ne!(first.id, second.id, "but they are not the same window");
        assert_eq!(first.target, "1234");
        assert_eq!(second.target, "5678");
        assert_eq!(first.kind, ObjectKind::Window);
    }

    #[test]
    fn a_window_with_no_title_falls_back_to_its_application() {
        // Better than an empty row. It is still selectable and still says
        // enough to be worth pressing Enter on.
        let nameless = crate::windowing::Window {
            id: 9,
            title: String::new(),
            app: "Steam".into(),
            app_path: String::new(),
            pid: 1,
            minimized: false,
            maximized: false,
            rect: crate::windowing::Rect::new(0, 0, 10, 10),
            monitor: 0,
        };

        assert_eq!(Object::from_window(&nameless).title, "Steam");
    }

    #[test]
    fn an_unknown_mode_is_refused_rather_than_guessed() {
        // An index written by a newer build. Guessing a kind here would run
        // some unrelated action on it.
        assert_eq!(ObjectKind::from_mode("something-new"), None);
        assert_eq!(ObjectKind::from_mode(""), None);
    }

    #[test]
    fn modes_that_only_differ_for_ranking_share_one_kind() {
        assert_eq!(
            ObjectKind::from_mode("app"),
            ObjectKind::from_mode("exe"),
            "an executable is launched exactly like an application"
        );
        assert_eq!(
            ObjectKind::from_mode("quicklink"),
            ObjectKind::from_mode("quicklink-arg"),
            "asking for a query first does not make it a different thing"
        );
        assert_eq!(
            ObjectKind::from_mode("view"),
            ObjectKind::from_mode("no-view"),
            "both are extension commands; only the loading differs"
        );
    }

    #[test]
    fn the_mode_survives_onto_the_object() {
        // Two modes share a kind, so an action that genuinely has to tell them
        // apart still can. Dropping it would make no-view commands unloadable.
        let record = CommandRecord {
            id: "ext:cmd".into(),
            extension: "ext".into(),
            extension_title: "Ext".into(),
            command: "cmd".into(),
            title: "Do The Thing".into(),
            subtitle: String::new(),
            description: String::new(),
            mode: "no-view".into(),
            entrypoint: "C:/build/cmd.js".into(),
            keywords: Vec::new(),
            icon: None,
            panel: None,
            preferences: serde_json::Value::Null,
        };

        let object = Object::from_record(&record).expect("a known mode");
        assert_eq!(object.kind, ObjectKind::ExtensionCommand);
        assert_eq!(object.mode, "no-view");
        assert_eq!(object.target, "C:/build/cmd.js");
    }
}
