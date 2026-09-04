//! Sill's own settings, as things that can be searched for.
//!
//! One catalogue, two readers: the settings window's own filter box, and the
//! launcher's root list. Keeping the list here rather than in the Svelte page
//! is what stops those two drifting apart, which they would the first time a
//! setting was added to one and not the other.
//!
//! Every entry names the panel it lives in. That is what lets a result carry
//! its parent's icon and say where it came from, and what makes opening one
//! land on the right panel rather than wherever settings was last left.

use serde::Serialize;

use crate::registry::CommandRecord;

/// One setting, as it is searched for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting {
    /// The panel it lives in, which is also its icon and its deep link.
    pub panel: &'static str,
    /// What the panel calls that panel.
    pub panel_name: &'static str,
    pub title: &'static str,
    /// Words someone might search for that are not in the title.
    pub keywords: &'static str,
}

/// Every panel the settings window has.
///
/// The single list, so a panel name is checked rather than trusted. A typo
/// anywhere that names one costs an icon and a working deep link, and neither
/// failure says anything at the time.
pub const PANELS: &[&str] = &[
    "general",
    "appearance",
    "ai",
    "dictation",
    "tts",
    "widgets",
    "snippets",
    "emoji",
    "shortcuts",
    "quicklinks",
    "clipboard",
    "sources",
    "files",
    "screenshot",
    "scripts",
    "extensions",
    "advanced",
    "about",
];

/// Every setting Sill has, in the order its panel shows it.
///
/// Sub-sections are **not** separate panels. Dictation's transcripts live in
/// Dictation, because a history of what dictation produced is part of
/// dictation and not a peer of it. The same reasoning would fold any future
/// sub-view into its parent rather than adding a row to the sidebar.
pub const SETTINGS: &[Setting] = &[
    s(
        "shortcuts",
        "Shortcuts",
        "Hyper key",
        "hyper caps lock modifier chord shortcut everything",
    ),
    // ------------------------------------------------------------- scripts
    s(
        "scripts",
        "Scripts",
        "Run script commands",
        "script shell powershell bash python command run",
    ),
    s(
        "scripts",
        "Scripts",
        "Folders scanned for scripts",
        "script folder directory scan where",
    ),
    s(
        "scripts",
        "Scripts",
        "Stop a script after",
        "script timeout stop long running seconds",
    ),
    s(
        "scripts",
        "Scripts",
        "Scripts allowed to run as administrator",
        "script administrator admin elevated elevate uac privilege rights",
    ),
    // ------------------------------------------------------------- general
    s(
        "general",
        "General",
        "Open at login",
        "startup boot autostart windows",
    ),
    s(
        "general",
        "General",
        "Show in the system tray",
        "tray notification area icon",
    ),
    s(
        "general",
        "General",
        "Hide when it loses focus",
        "blur dismiss escape close",
    ),
    s(
        "general",
        "General",
        "Select the search text",
        "query summon replace typing",
    ),
    s(
        "general",
        "General",
        "Return to the root list",
        "reset summon root back",
    ),
    // ---------------------------------------------------------- appearance
    s(
        "appearance",
        "Appearance",
        "Theme",
        "colour color palette accent oilslick graphite ember moss aberration \n         chromatic prism fringe chroma gradient dark",
    ),
    s(
        "appearance",
        "Appearance",
        "Chroma",
        "iridescent wash colour strength prism spectral",
    ),
    s(
        "appearance",
        "Appearance",
        "Interface font",
        "typeface inter segoe type crisp",
    ),
    s(
        "appearance",
        "Appearance",
        "Backdrop",
        "acrylic blur glass material transparency",
    ),
    s(
        "appearance",
        "Appearance",
        "Backdrop depth",
        "tint alpha dark opacity",
    ),
    s(
        "appearance",
        "Appearance",
        "Glass strength",
        "solid opaque transparency",
    ),
    s(
        "appearance",
        "Appearance",
        "Rows before scrolling",
        "height size window rows",
    ),
    s(
        "appearance",
        "Appearance",
        "Window width",
        "width size window",
    ),
    s(
        "appearance",
        "Appearance",
        "Where it appears",
        "monitor screen display second multiple cursor mouse primary",
    ),
    // ----------------------------------------------------------------- ask
    s(
        "ai",
        "AI Chat",
        "Who answers",
        "ai model chat provider claude ollama openai gpt tab ask llm chat",
    ),
    s(
        "ai",
        "AI Chat",
        "Model",
        "ai which model change switch sonnet opus haiku gpt llama qwen",
    ),
    s(
        "ai",
        "AI Chat",
        "Address",
        "ai endpoint base url ollama local server http https lm studio",
    ),
    s(
        "ai",
        "AI Chat",
        "Key",
        "ai api key token secret credential paste anthropic openrouter",
    ),
    s(
        "advanced",
        "Advanced",
        "What Sill has done",
        "activity history undo revert log recent took back reverse",
    ),
    // ------------------------------------------------------------- widgets
    s(
        "widgets",
        "Widgets",
        "Pin a widget to the launcher",
        "widget pin chin clock weather glance dashboard",
    ),
    s(
        "widgets",
        "Widgets",
        "Where the weather is for",
        "weather location city place temperature forecast fahrenheit celsius",
    ),
    // ----------------------------------------------------------------- tts
    s(
        "tts",
        "Text to Speech",
        "Read aloud",
        "voice speak talk text to speech tts narrate out loud say",
    ),
    s(
        "tts",
        "Text to Speech",
        "Voice",
        "who reads speaker aloud engine provider openai piper system sapi \n         natural neural elevenlabs kokoro",
    ),
    s(
        "tts",
        "Text to Speech",
        "Address",
        "endpoint url server speech host",
    ),
    s(
        "tts",
        "Text to Speech",
        "Key",
        "api key token speech credential",
    ),
    s(
        "tts",
        "Text to Speech",
        "Model",
        "speech model voice engine",
    ),
    s(
        "tts",
        "Text to Speech",
        "Download an offline voice",
        "piper neural offline local model amy ryan alba lessac free no key",
    ),
    // ----------------------------------------------------------- dictation
    s(
        "dictation",
        "Dictation",
        "Dictation",
        "voice speech whisper microphone talk",
    ),
    s(
        "dictation",
        "Dictation",
        "Start dictating",
        "shortcut hotkey trigger push talk",
    ),
    s(
        "dictation",
        "Dictation",
        "What happens to the transcript",
        "paste clipboard output",
    ),
    s(
        "dictation",
        "Dictation",
        "Use the system default",
        "microphone input device audio mic default",
    ),
    s(
        "dictation",
        "Dictation",
        "Priority order",
        "microphone input device headset rank order",
    ),
    s(
        "dictation",
        "Dictation",
        "Mute everything else while recording",
        "audio speakers music silence mute",
    ),
    s(
        "dictation",
        "Dictation",
        "Language",
        "english auto detect locale",
    ),
    s(
        "dictation",
        "Dictation",
        "Finish and cancel keys",
        "enter escape space discard",
    ),
    s(
        "dictation",
        "Dictation",
        "Ask before discarding",
        "confirm cancel warning discard",
    ),
    s(
        "dictation",
        "Dictation",
        "Start and stop cues",
        "sound effects audio feedback tone",
    ),
    s(
        "dictation",
        "Dictation",
        "Keep a history",
        "transcripts record statistics",
    ),
    s(
        "dictation",
        "Dictation",
        "Keep transcripts for",
        "history retention days delete transcripts",
    ),
    s(
        "dictation",
        "Dictation",
        "Backend",
        "transcription local whisper openai groq api engine",
    ),
    s(
        "dictation",
        "Dictation",
        "API key",
        "token secret credential openai groq",
    ),
    s(
        "dictation",
        "Dictation",
        "Custom endpoint",
        "url server remote host base",
    ),
    s(
        "dictation",
        "Dictation",
        "Custom instructions",
        "prompt guidance style spelling",
    ),
    s(
        "dictation",
        "Dictation",
        "Use the frontmost application",
        "app context window prompt",
    ),
    s(
        "dictation",
        "Dictation",
        "Vocabulary",
        "prompt names jargon bias terms",
    ),
    s(
        "dictation",
        "Dictation",
        "Speech model",
        "whisper tiny base small medium download",
    ),
    // Dictation's own history, inside dictation where it belongs.
    s(
        "dictation",
        "Dictation",
        "Dictation transcripts",
        "history past search transcript",
    ),
    s(
        "dictation",
        "Dictation",
        "Clear dictation history",
        "delete forget wipe transcripts",
    ),
    s(
        "dictation",
        "Dictation",
        "Dictation statistics",
        "words per minute time saved wpm",
    ),
    // ------------------------------------------------------------ snippets
    s(
        "snippets",
        "Snippets",
        "Expand keywords as I type",
        "snippet expansion abbreviation",
    ),
    s(
        "snippets",
        "Snippets",
        "Snippets",
        "template saved text signature placeholder",
    ),
    s(
        "snippets",
        "Snippets",
        "Snippets as a file",
        "import export backup json file",
    ),
    // --------------------------------------------------------------- emoji
    s(
        "emoji",
        "Emoji",
        "Skin tone",
        "emoji hand colour color diverse tone people",
    ),
    s(
        "emoji",
        "Emoji",
        "What Enter does",
        "emoji paste copy primary action",
    ),
    s(
        "emoji",
        "Emoji",
        "Learning what you call things",
        "learned names nicknames emoji search remember",
    ),
    // ----------------------------------------------------------- shortcuts
    s(
        "shortcuts",
        "Shortcuts",
        "Summon hotkey",
        "shortcut keybind alt space keyboard hotkey",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Window switcher hotkey",
        "alt tab window switch cycle shortcut keybind",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Put the result back",
        "paste replace selection result back",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Open with a double-tap",
        "double tap modifier open shortcut",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Extra keys",
        "extra modifier keys action shortcut",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Jump to a row by number",
        "number row jump select digit",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Action keys",
        "action panel key chord copy path name terminal reveal read aloud",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Moving around",
        "vim emacs arrows navigation keys preset page section jump number",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Shortcuts",
        "hotkey key global selection transform text case",
    ),
    // ---------------------------------------------------------- quicklinks
    s(
        "quicklinks",
        "Quicklinks",
        "Quicklinks",
        "link url bookmark search open",
    ),
    s(
        "quicklinks",
        "Quicklinks",
        "Open with",
        "browser application default chrome",
    ),
    s(
        "quicklinks",
        "Quicklinks",
        "Quicklinks as a file",
        "import export backup json file",
    ),
    // ----------------------------------------------------------- clipboard
    s(
        "clipboard",
        "Clipboard History",
        "Record what I copy",
        "clipboard history paste enable",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Keep history for",
        "retention days delete old expire",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Keep at most",
        "limit count cap entries maximum number trim oldest",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Things that look like passwords",
        "secret token api key credential password redact skip private",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Keep images",
        "screenshots pictures clipboard",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Lock stored pictures",
        "encrypt encryption secure protect screenshots pictures at rest account",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Never record from",
        "excluded applications ignore private password exclude",
    ),
    s(
        "clipboard",
        "Clipboard History",
        "Clear clipboard history",
        "delete wipe entries",
    ),
    // ------------------------------------------------------------- sources
    s(
        "sources",
        "Sources",
        "Start Menu, Desktop and taskbar",
        "shortcuts lnk pinned",
    ),
    s(
        "sources",
        "Sources",
        "Store and packaged applications",
        "appx uwp store msix",
    ),
    s(
        "sources",
        "Sources",
        "Registered executables",
        "app paths registry run",
    ),
    s(
        "sources",
        "Sources",
        "Installed programs",
        "uninstall registry programs",
    ),
    s(
        "sources",
        "Sources",
        "Executables on PATH",
        "cli command line exe path",
    ),
    s(
        "sources",
        "Sources",
        "Windows settings pages",
        "control panel applets",
    ),
    s(
        "sources",
        "Sources",
        "Installed games",
        "steam epic game library play",
    ),
    s(
        "sources",
        "Sources",
        "Folders of your own",
        "custom folder directory portable tools scan add",
    ),
    s(
        "sources",
        "Sources",
        "What Sill found",
        "alias hotkey nickname shortcut name list index everything hide",
    ),
    s(
        "sources",
        "Sources",
        "Hidden entries",
        "exclude filter block ignore hide",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Screenshot hotkey",
        "screenshot capture key bind area region",
    ),
    s(
        "shortcuts",
        "Shortcuts",
        "Whole screen hotkey",
        "screenshot capture key bind fullscreen display",
    ),
    // ---------------------------------------------------------- screenshot
    s(
        "screenshot",
        "Screenshots",
        "After taking one",
        "screenshot capture editor markup open copy",
    ),
    s(
        "screenshot",
        "Screenshots",
        "Click a window to take it",
        "screenshot window app capture click",
    ),
    s(
        "screenshot",
        "Screenshots",
        "Tool",
        "screenshot editor markup default tool",
    ),
    s(
        "screenshot",
        "Screenshots",
        "Colour",
        "screenshot editor markup default colour color",
    ),
    s(
        "screenshot",
        "Screenshots",
        "Badges start at",
        "screenshot editor markup numbered badge step number start walkthrough",
    ),
    s(
        "screenshot",
        "Screenshots",
        "Stroke width",
        "screenshot editor markup default weight size",
    ),
    // ------------------------------------------------- sources, the web
    s(
        "sources",
        "Sources",
        "Offer to search the web",
        "web internet lookup enable google duckduckgo bing brave engine",
    ),
    s(
        "sources",
        "Sources",
        "Engine",
        "web search google duckduckgo bing brave startpage provider",
    ),
    s(
        "sources",
        "Sources",
        "Your own address",
        "custom web search engine url query template",
    ),
    // -------------------------------------------- sources, browser pages
    s(
        "sources",
        "Sources",
        "Search browser pages",
        "browser history bookmarks pages chrome edge firefox zen web enable",
    ),
    s(
        "sources",
        "Sources",
        "Bookmarks",
        "browser saved favourites favorites starred pages",
    ),
    s(
        "sources",
        "Sources",
        "History",
        "browser visited pages recently",
    ),
    s(
        "sources",
        "Sources",
        "Maximum browser results",
        "browser limit count results pages",
    ),
    // --------------------------------------------------------------- files
    s(
        "files",
        "File Search",
        "Search files",
        "everything voidtools enable",
    ),
    s(
        "files",
        "File Search",
        "Maximum file results",
        "limit count results",
    ),
    s(
        "files",
        "File Search",
        "Match the whole path",
        "path folder match",
    ),
    s("files", "File Search", "Match case", "case sensitive"),
    s(
        "files",
        "File Search",
        "Regular expression",
        "regex pattern",
    ),
    s(
        "files",
        "File Search",
        "Folders",
        "folders to search scope restrict directories roots indexed",
    ),
    s(
        "files",
        "File Search",
        "Drives",
        "whole drive volume index read",
    ),
    s(
        "files",
        "File Search",
        "Only show results in",
        "narrow filter folders results limit",
    ),
    // ---------------------------------------------------------- extensions
    s(
        "extensions",
        "Extensions",
        "Installed extensions",
        "raycast host node commands version revision update outdated remove uninstall",
    ),
    s(
        "extensions",
        "Extensions",
        "What an extension is allowed to reach",
        "permission grant revoke allow deny files clipboard network programs \
         selection not allowed unavailable capability",
    ),
    s(
        "extensions",
        "Extensions",
        "Only Windows extensions",
        "store platform macos compatible filter browse hide",
    ),
    s(
        "extensions",
        "Extensions",
        "GitHub token",
        "store rate limit api requests hour credential browse install",
    ),
    // ------------------------------------------------------------ advanced
    s(
        "advanced",
        "Advanced",
        "Rebuild the index",
        "reload rescan reindex refresh",
    ),
    s(
        "advanced",
        "Advanced",
        "Usage history",
        "frecency ranking forget clear reset",
    ),
    s(
        "advanced",
        "Advanced",
        "Export settings",
        "backup save copy migrate move machine transfer share",
    ),
    s(
        "advanced",
        "Advanced",
        "Import settings",
        "restore backup migrate powertoys run raycast rayconfig transfer",
    ),
    s(
        "advanced",
        "Advanced",
        "Data folder",
        "appdata preferences cache open",
    ),
    s(
        "advanced",
        "Advanced",
        "Log",
        "diagnostics debug error output",
    ),
    s(
        "advanced",
        "Advanced",
        "Detailed logging",
        "verbose debug level trace timings chase fault",
    ),
    s(
        "advanced",
        "Advanced",
        "Export diagnostics",
        "bundle report support send crash log troubleshoot",
    ),
    // --------------------------------------------------------------- about
    s("about", "About", "Version", "build release licence credits"),
    s(
        "about",
        "About",
        "Licence",
        "mit open source licence licensing",
    ),
    s(
        "about",
        "About",
        "Indexed entries",
        "how many entries index count",
    ),
];

const fn s(
    panel: &'static str,
    panel_name: &'static str,
    title: &'static str,
    keywords: &'static str,
) -> Setting {
    Setting {
        panel,
        panel_name,
        title,
        keywords,
    }
}

/// The catalogue as launcher results.
///
/// `mode` is `"sill-setting"` so the list can draw the parent panel's glyph
/// rather than asking the shell for an icon of a file that does not exist,
/// and `entrypoint` is the panel so launching one opens settings there.
pub fn records() -> Vec<CommandRecord> {
    SETTINGS
        .iter()
        .map(|setting| CommandRecord {
            id: format!("sill-setting:{}:{}", setting.panel, setting.title),
            extension: "sill".to_string(),
            extension_title: setting.panel_name.to_string(),
            command: setting.title.to_string(),
            title: setting.title.to_string(),
            // Where it came from, which is what the row shows beside the name.
            subtitle: format!("Sill Settings, {}", setting.panel_name),
            description: String::new(),
            mode: "sill-setting".to_string(),
            entrypoint: setting.panel.to_string(),
            keywords: setting
                .keywords
                .split_whitespace()
                .map(str::to_string)
                .collect(),
            icon: None,
            toggle: None,
            panel: Some(setting.panel.to_string()),
            // Only extension commands carry any.
            preferences: serde_json::Value::Null,
            manifest: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_setting_names_a_panel_that_exists() {
        // The panel is the deep link and the icon; a typo here opens settings
        // at whatever was last shown and draws no glyph.
        let panels: HashSet<&str> = PANELS.iter().copied().collect();

        for setting in SETTINGS {
            assert!(
                panels.contains(setting.panel),
                "{} names an unknown panel {:?}",
                setting.title,
                setting.panel
            );
        }
    }

    /// The other direction, which is the one that fails quietly.
    ///
    /// A panel with no settings still draws in the sidebar, so it looks
    /// finished; it is only unreachable from the launcher, where most people
    /// look for a setting first. The Ask panel shipped that way and was found
    /// by hand rather than by a test.
    #[test]
    fn every_panel_has_at_least_one_setting_somebody_can_search_for() {
        let named: HashSet<&str> = SETTINGS.iter().map(|setting| setting.panel).collect();

        for panel in PANELS {
            assert!(
                named.contains(panel),
                "the {panel:?} panel has no settings, so nothing in it can be \
                 found from the launcher",
            );
        }
    }

    #[test]
    fn every_builtin_names_a_panel_that_exists() {
        // Sill's own commands carry their panel out to the launcher so they
        // arrive under the same mark they wear in settings. An unset or
        // misspelt panel silently falls back to a lettered tile, which looks
        // like the icons were never wired up rather than like a typo.
        let panels: HashSet<&str> = PANELS.iter().copied().collect();
        for record in crate::registry::builtins() {
            // A row either names a panel, and wears that panel's mark, or
            // brings an icon of its own. Windows' switches do the second: a
            // volume control wearing the settings gear would say it belongs
            // to Sill, which is the one thing it must not say.
            if record.icon.is_some() {
                assert!(
                    record.panel.is_none(),
                    "{} carries an icon and a panel, so which one wins is a guess",
                    record.title
                );
                continue;
            }

            let panel = record
                .panel
                .as_deref()
                .unwrap_or_else(|| panic!("{} has neither a panel nor an icon", record.title));
            assert!(
                panels.contains(panel),
                "{} names an unknown panel {panel:?}",
                record.title
            );
        }
    }

    #[test]
    fn there_is_no_history_panel_any_more() {
        // Dictation's transcripts belong inside dictation, not beside it. A
        // history of what a feature produced is part of that feature.
        assert!(
            SETTINGS.iter().all(|s| s.panel != "history"),
            "transcripts should live under dictation"
        );
        assert!(
            SETTINGS
                .iter()
                .any(|s| s.title == "Dictation transcripts" && s.panel == "dictation"),
            "and they should still be findable"
        );
    }

    #[test]
    fn a_panel_always_carries_the_same_name() {
        // The name is shown on the row, so two spellings of one panel would
        // look like two different places.
        let mut names: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for setting in SETTINGS {
            let seen = names.entry(setting.panel).or_insert(setting.panel_name);
            assert_eq!(
                *seen, setting.panel_name,
                "{} is called two different things",
                setting.panel
            );
        }
    }

    #[test]
    fn no_panel_holds_the_same_title_twice() {
        // The panel and the title together are the id, because the title on
        // its own is what the row is called on screen and two panels are
        // allowed to call a row the same thing: the address of an AI provider
        // and the address of a speech server are both "Address", and
        // `verify:source` requires each entry to spell its row exactly. Twice
        // in one panel is still a duplicate, and would make one of the two
        // unreachable from the launcher.
        let mut seen = HashSet::new();
        for setting in SETTINGS {
            assert!(
                seen.insert((setting.panel, setting.title)),
                "{:?} appears twice in {}",
                setting.title,
                setting.panel
            );
        }
    }

    #[test]
    fn records_carry_the_panel_as_their_entrypoint() {
        let records = records();
        assert_eq!(records.len(), SETTINGS.len());

        let vocabulary = records
            .iter()
            .find(|r| r.title == "Vocabulary")
            .expect("vocabulary is in the catalogue");

        assert_eq!(vocabulary.entrypoint, "dictation");
        assert_eq!(vocabulary.mode, "sill-setting");
        assert!(vocabulary.subtitle.contains("Dictation"));
        assert!(vocabulary.keywords.contains(&"jargon".to_string()));
    }
}
