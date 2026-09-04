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

use serde::{Deserialize, Serialize};

use crate::registry::CommandRecord;

/// What kind of thing an action is being asked to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// A switch belonging to Windows: the volume, the theme, the lock screen.
    ///
    /// Separate from [`ObjectKind::SystemSetting`], which opens a page and
    /// leaves the changing to somebody. These change the machine outright.
    SystemControl,
    Snippet,
    Quicklink,
    /// A file on disk with a header saying it is a command.
    ///
    /// Not a [`ObjectKind::File`]: a script command is a thing somebody wrote
    /// to be run, and the actions that apply to it are running it, not
    /// revealing it in Explorer.
    Script,
    /// A calculator result, which exists only for as long as it is on screen.
    Answer,
    /// A row of clipboard history.
    ClipboardEntry,
    /// Loose text: a selection, or whatever an action produced.
    Text,
    /// One emoji, which is a piece of text with a name.
    Emoji,
    /// A window that is open right now.
    ///
    /// The first kind that is not in the index and never will be. The desktop
    /// changes faster than any scan could keep up with, so a window is
    /// enumerated at the moment it is searched for and its identity is a
    /// handle that stops being valid when it closes.
    Window,
    /// One tab of a browser that is running right now.
    ///
    /// Not an [`ObjectKind::Url`], which is an address anything can open, and
    /// not an [`ObjectKind::Window`], which is what the tab is inside. A tab
    /// already exists somewhere, the only useful thing to do with one is go to
    /// it, and going to it is neither opening an address nor raising a window.
    BrowserTab,
    /// Words to look up on the web.
    ///
    /// Not a [`ObjectKind::Url`]: there is no address yet. Which engine turns
    /// these words into one is a setting, and it can change between this being
    /// offered and being chosen.
    Search,
    /// A web address.
    ///
    /// Distinct from [`ObjectKind::Quicklink`], which is a saved link somebody
    /// wrote and may carry a hole to fill in. This is an address that already
    /// exists somewhere else, such as a page a browser remembers.
    Url,
    /// One program's own volume, as Windows keeps it.
    ///
    /// Like a [`ObjectKind::Window`] and for the same reason: it is not in the
    /// index and never will be. A program has a session only while it is
    /// playing something, so the list is enumerated at the moment it is asked
    /// for and a row's identity stops meaning anything when the program goes
    /// quiet.
    AudioSession,
    /// Whatever is playing right now, as one row.
    ///
    /// Not in the index, like a window and an audio session, and one step
    /// further out than either: there is no list of these to enumerate. There
    /// is one, or there is none, and which one it is belongs to Windows rather
    /// than to Sill.
    NowPlaying,
    /// A running program.
    ///
    /// Like a window and an audio session: enumerated the moment it is asked
    /// for and never in the index, because what is running changes constantly
    /// and a process id stops meaning anything the moment it exits.
    Process,
    /// A saved window arrangement.
    Workspace,
    /// A conversation with the model, as it sits in the list of past ones.
    ///
    /// Not in the index either, and for a different reason from a window: it
    /// exists on disk and could be indexed, but a list of everything ever
    /// asked has no business competing with applications for a keystroke.
    /// It reaches the launcher through its own view, and this is the name the
    /// action registry dispatches on when it does.
    Conversation,
    /// One extension as the store lists it, installed here or not.
    ///
    /// The one kind that is not a thing on this machine. It is a row in
    /// somebody else's catalogue, joined against what is installed, and its
    /// identity is the extension's name because that is the string every
    /// store operation already takes and the one part of a listing that
    /// survives the catalogue being fetched again.
    ///
    /// Not an [`ObjectKind::ExtensionCommand`], which is a command that is
    /// installed and can be run. A listing has no entrypoint and may have no
    /// files on this machine at all.
    StoreListing,
    /// One way of opening a terminal: a Windows Terminal profile, or a WSL
    /// distribution that has no profile of its own.
    ///
    /// Not an [`ObjectKind::Application`]. `wt.exe` is the application and a
    /// profile is an argument to it, so launching one is not launching a
    /// program at a path; and the two kinds of row here start two different
    /// programs, which is a fact about the row rather than about the file
    /// system. Not in the index either: the list is Terminal's settings file
    /// and the registry's WSL keys, read when somebody asks for it.
    TerminalProfile,
    /// One button, checkbox, menu item or tab of a window that is open now.
    ///
    /// The shortest-lived kind there is. A window is a handle that lasts until
    /// it closes; a control inside one lasts until that window redraws itself,
    /// and a program is free to rebuild its toolbar between somebody reading a
    /// row and pressing Enter on it. So this carries the provider's own
    /// identifier **and** the name it was read under, and refuses when either
    /// has moved. See [`crate::controls`].
    ScreenControl,
}

impl ObjectKind {
    /// Every kind there is.
    ///
    /// The invariant tests walk this, so a kind missing from it is a kind
    /// nothing checks. **Three were missing**: a Windows switch, a web search
    /// and a web address were in the enum and in none of the tests that ask
    /// whether two actions claim Enter or whether every action declares what
    /// it touches.
    ///
    /// The comment here used to claim that adding a variant fails to compile
    /// until it is listed. It did not, because a list is not a match. The test
    /// beside this one is what makes that true now.
    pub const ALL: &'static [Self] = &[
        Self::Application,
        Self::File,
        Self::Folder,
        Self::ExtensionCommand,
        Self::SystemSetting,
        Self::Setting,
        Self::Builtin,
        Self::SystemControl,
        Self::Snippet,
        Self::Quicklink,
        Self::Answer,
        Self::ClipboardEntry,
        Self::Text,
        Self::Emoji,
        Self::Window,
        Self::BrowserTab,
        Self::Search,
        Self::Url,
        Self::AudioSession,
        Self::NowPlaying,
        Self::Process,
        Self::Workspace,
        Self::Script,
        Self::Conversation,
        Self::StoreListing,
        Self::TerminalProfile,
        Self::ScreenControl,
    ];

    /**
    What this is, said the way somebody would say it.

    For the model, which is told what it is looking at rather than shown a
    `mode` string. **Exhaustive on purpose, with no catch-all**: this was a
    match on the mode string with `_ => "result"`, and nine kinds fell into it,
    so a script, an emoji, a program's volume, a running process, a saved
    arrangement, a web search and a remembered page were all described to the
    model as "result". It could not ask about what it could not name.

    Derived from the kind rather than the mode for the same reason: there are
    more modes than kinds and the kinds are the thing actions are declared
    against, so a new one is a compile error here instead of a silent
    "result".
    */
    pub fn plainly(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::File => "file",
            Self::Folder => "folder",
            Self::ExtensionCommand => "extension command",
            Self::SystemSetting => "Windows setting",
            Self::Setting => "Sill setting",
            Self::Builtin => "Sill command",
            Self::SystemControl => "system switch",
            Self::Snippet => "saved snippet",
            Self::Quicklink => "saved link",
            Self::Script => "script",
            Self::Answer => "calculated answer",
            Self::ClipboardEntry => "clipboard entry",
            Self::Text => "piece of text",
            Self::Emoji => "emoji",
            Self::Window => "open window",
            Self::BrowserTab => "browser tab",
            Self::Search => "web search",
            Self::Url => "web page",
            Self::AudioSession => "program's volume",
            Self::NowPlaying => "what is playing",
            Self::Process => "running program",
            Self::Workspace => "saved arrangement",
            Self::Conversation => "conversation",
            Self::StoreListing => "extension in the store",
            Self::TerminalProfile => "terminal profile",
            Self::ScreenControl => "control on screen",
        }
    }

    /**
    The kind an extension's manifest names, or nothing for a word Sill has
    never heard of.

    Through serde rather than a second match, deliberately. This is the same
    string [`Object`] serialises as `kind`, so an extension declaring
    `"extensionCommand"` and Sill sending `"extensionCommand"` to the worker
    are one spelling with one definition. A hand-written table beside the
    derive is two answers to the question "what is this kind called", and the
    one that is wrong is whichever nobody looked at.

    `None` rather than a default, for the reason [`Self::from_mode`] gives.
    An unknown kind is refused at install with the word in it; a kind that
    somehow reaches a built index is left inert rather than guessed at.
    */
    pub fn named(name: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(name.to_string())).ok()
    }

    /// What this kind is called on the wire and in a manifest.
    ///
    /// The other direction of [`Self::named`], and the test beside them holds
    /// the two together over every kind in [`Self::ALL`].
    pub fn name(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::File => "file",
            Self::Folder => "folder",
            Self::ExtensionCommand => "extensionCommand",
            Self::SystemSetting => "systemSetting",
            Self::Setting => "setting",
            Self::Builtin => "builtin",
            Self::SystemControl => "systemControl",
            Self::Snippet => "snippet",
            Self::Quicklink => "quicklink",
            Self::TerminalProfile => "terminalProfile",
            Self::ScreenControl => "screenControl",
            Self::Script => "script",
            Self::Answer => "answer",
            Self::ClipboardEntry => "clipboardEntry",
            Self::Text => "text",
            Self::Emoji => "emoji",
            Self::Window => "window",
            Self::BrowserTab => "browserTab",
            Self::Search => "search",
            Self::Url => "url",
            Self::AudioSession => "audioSession",
            Self::NowPlaying => "nowPlaying",
            Self::Process => "process",
            Self::Workspace => "workspace",
            Self::Conversation => "conversation",
            Self::StoreListing => "storeListing",
        }
    }

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
            // Not a Sill feature. Changing the volume or the theme changes
            // Windows, and a row that looked like one of Sill's own commands
            // would be hiding that.
            "system" => Self::SystemControl,
            "snippet" => Self::Snippet,
            // The argument version is the same kind of thing; whether it stops
            // to ask is a property of the link, not of what it is.
            "quicklink" | "quicklink-arg" => Self::Quicklink,
            // Same for a script: whether it stops to ask for an argument is a
            // property of the script's own header, not of what it is.
            "script" | "script-arg" => Self::Script,
            "answer" => Self::Answer,
            "url" => Self::Url,
            "websearch" => Self::Search,
            // Not index entries. A clipboard row and a piece of loose text
            // reach an action through the window rather than through a scan,
            // and they still need a name to be dispatched on.
            "clipboard" => Self::ClipboardEntry,
            "text" => Self::Text,
            "emoji" => Self::Emoji,
            "window" => Self::Window,
            // A tab a browser has open, which no scan produces and no index
            // holds: it is read from the running browser when somebody types.
            "browser-tab" => Self::BrowserTab,
            "audio-session" => Self::AudioSession,
            // What is playing, which is a row the search builds when somebody
            // asks for it and never anything the index holds.
            "media" => Self::NowPlaying,
            "process" => Self::Process,
            "workspace" => Self::Workspace,
            // Two modes, one kind. One is the conversation offered at the top
            // of the root list for a few minutes after you leave it, the
            // other is a row in the list of everything you have asked. They
            // are the same thing and the same actions apply.
            "conversation" | "past-conversation" => Self::Conversation,
            // A row in the extension store. Not in the index and never will be:
            // the catalogue is somebody else's list, fetched and parked, and
            // folding three thousand listings into what a keystroke weighs
            // would be paying for the store on every search.
            "store-listing" => Self::StoreListing,
            // A terminal profile, which is not in the index either: it is
            // read out of Terminal's settings and the WSL registry keys when
            // a query asks for one.
            "terminal-profile" => Self::TerminalProfile,
            // A control of a window somebody is looking at, read when they
            // open the view that lists them and never held between two
            // keystrokes. No index has one and none ever will.
            "control" => Self::ScreenControl,
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
                // A web search and a remembered page both end in a browser,
                // for the same reason a quicklink does. They were missing, so
                // the launcher restored whatever had been in front on top of
                // the browser it had just handed the question to.
                | Self::Url
                | Self::Search
                // Restoring an arrangement puts other people's windows in
                // front by definition.
                | Self::Workspace
                // Switching to a window is the clearest case of all: the whole
                // point is that something else ends up in front.
                | Self::Window
                // And switching to a tab is switching to the window it is in.
                | Self::BrowserTab
                // Opening a terminal puts a terminal in front, which is the
                // entire reason for pressing it.
                | Self::TerminalProfile
                // Pressing a control means watching what it did, and what it
                // did is behind the launcher. Staying up would be sitting on
                // top of the answer.
                | Self::ScreenControl
        )
    }
}

/// One thing, and enough to act on it.
///
/// Deliberately flat. Every kind above carries exactly one meaningful string
/// today: a path, a panel name, a snippet id, a row id, a result. A payload
/// enum would be inventing structure that nothing yet needs, and the shape of
/// the structure that *is* eventually needed is not knowable from here.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// The name a manifest writes and the name the wire carries are one name.
    ///
    /// Three spellings have to agree: what `name()` returns, what serde
    /// serialises an [`Object`]'s kind as, and what `named()` reads back. They
    /// are what an extension's manifest declares its action applies to, so a
    /// disagreement is an extension whose action is silently never offered,
    /// with nothing anywhere saying why.
    #[test]
    fn every_kind_is_spelled_one_way() {
        for kind in ObjectKind::ALL {
            let name = kind.name();

            assert_eq!(
                serde_json::to_value(kind).expect("a kind serialises"),
                serde_json::Value::String(name.to_string()),
                "{name} is not what serde calls this kind",
            );

            assert_eq!(
                ObjectKind::named(name),
                Some(*kind),
                "{name} does not read back as the kind it names",
            );
        }
    }

    /// A word nobody has heard of is refused rather than guessed at.
    #[test]
    fn a_kind_sill_has_never_heard_of_is_not_a_kind() {
        assert_eq!(ObjectKind::named("menuBarItem"), None);
        assert_eq!(ObjectKind::named(""), None);
        // The plain-English name, which is what somebody would try first and
        // is deliberately not accepted: one spelling, not two.
        assert_eq!(ObjectKind::named("extension command"), None);
    }

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
            "script",
            "script-arg",
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
            elsewhere: false,
            desktop: None,
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
            elsewhere: false,
            desktop: None,
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
            manifest: None,
            toggle: None,
        };

        let object = Object::from_record(&record).expect("a known mode");
        assert_eq!(object.kind, ObjectKind::ExtensionCommand);
        assert_eq!(object.mode, "no-view");
        assert_eq!(object.target, "C:/build/cmd.js");
    }

    /// `ALL` is what the invariant tests walk, so it has to hold every kind.
    ///
    /// The match below is exhaustive, which is the whole mechanism: adding a
    /// variant stops this compiling until somebody has decided where it goes.
    /// The doc on `ALL` used to claim that happened already. It did not, a
    /// list is not a match, and three kinds sat in the enum unchecked by any
    /// invariant for as long as they had existed.
    #[test]
    fn every_kind_is_in_the_list_the_invariants_walk() {
        fn place(kind: ObjectKind) -> usize {
            match kind {
                ObjectKind::Application => 0,
                ObjectKind::File => 1,
                ObjectKind::Folder => 2,
                ObjectKind::ExtensionCommand => 3,
                ObjectKind::SystemSetting => 4,
                ObjectKind::Setting => 5,
                ObjectKind::Builtin => 6,
                ObjectKind::SystemControl => 7,
                ObjectKind::Snippet => 8,
                ObjectKind::Quicklink => 9,
                ObjectKind::Answer => 10,
                ObjectKind::ClipboardEntry => 11,
                ObjectKind::Text => 12,
                ObjectKind::Emoji => 13,
                ObjectKind::Window => 14,
                ObjectKind::BrowserTab => 15,
                ObjectKind::Search => 16,
                ObjectKind::Url => 17,
                ObjectKind::AudioSession => 18,
                ObjectKind::NowPlaying => 19,
                ObjectKind::Process => 20,
                ObjectKind::Workspace => 21,
                ObjectKind::Script => 22,
                ObjectKind::Conversation => 23,
                ObjectKind::StoreListing => 24,
                ObjectKind::TerminalProfile => 25,
                ObjectKind::ScreenControl => 26,
            }
        }

        assert_eq!(
            ObjectKind::ALL.len(),
            27,
            "a kind was added or removed without `ALL` being told",
        );

        for (at, kind) in ObjectKind::ALL.iter().enumerate() {
            assert_eq!(place(*kind), at, "{kind:?} is not where `ALL` puts it");
        }
    }
}
