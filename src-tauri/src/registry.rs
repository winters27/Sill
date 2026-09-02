//! What the launcher can run, and how it decides what to show first.
//!
//! Two concerns that only make sense together: the set of installed commands,
//! and the ranking that turns a half-typed query into the one the user meant.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One runnable command, as written by `scripts/build-extension.mjs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    /// `<extension>:<command>`, stable across rebuilds and used as the
    /// frecency key.
    pub id: String,
    pub extension: String,
    pub extension_title: String,
    pub command: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub description: String,
    pub mode: String,
    pub entrypoint: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// A file to pull an icon from, when it differs from the launch target.
    ///
    /// An Apps folder entry launches by AppUserModelID but may still have a
    /// real executable behind it, and a packaged app has neither.
    #[serde(default)]
    pub icon: Option<String>,
    /// Whether this row is a switch, and which way it is set.
    ///
    /// `None` for everything that is not one, which is nearly everything. A
    /// row that carries this draws as a control rather than as a command:
    /// pressing it flips the thing and leaves the launcher where it is, so the
    /// state can be seen changing and changed again.
    ///
    /// Filled at search time rather than when the index is built. What a
    /// switch is set to is a fact about the moment it is looked at, and a
    /// value written into the index would be whatever it was at the last scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
    /// The settings panel this belongs to, for anything Sill owns.
    ///
    /// Carries the panel's drawn icon out to the launcher, so "Dictate" and
    /// "Dictation Vocabulary" arrive under the same mark they wear in
    /// settings. Set here rather than mapped in the frontend, because the
    /// answer is a fact about the command and would otherwise be maintained
    /// in two places that drift.
    #[serde(default)]
    pub panel: Option<String>,
    /// What `getPreferenceValues()` answers with, from the manifest.
    ///
    /// Only extension commands have any, so it is skipped when empty rather
    /// than writing `{}` beside every one of the thousand-odd applications in
    /// the index cache.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub preferences: serde_json::Value,
}

/// A command plus why it placed where it did.
///
/// Internal to ranking. What crosses the IPC boundary is [`SearchResult`],
/// which is a good deal smaller.
#[derive(Debug, Clone)]
pub struct RankedCommand {
    pub command: CommandRecord,
    pub score: i64,
    /// Indices into `title` that matched, so the UI can highlight them.
    pub matched: Vec<usize>,
    /// How this result was reached.
    ///
    /// Kept past the sort because two searches are merged in the window and
    /// the merge has to know which of them found something the user actually
    /// named. Never sent: only the answer to [`is_strong`] crosses the wire.
    pub class: MatchClass,
}

/// One result, as the window receives it.
///
/// Separate from [`CommandRecord`] because the two are answering different
/// questions. The record is what ranking needs, and it carries the fields that
/// make matching work: `description`, `keywords`, and the score itself. **None
/// of those are ever read by the frontend**, and flattening the whole record
/// onto the wire meant sending every one of them for every result.
///
/// This is rule 9's domain/DTO split, applied at the one place it pays for
/// itself: this type is serialised on every keystroke.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub extension: String,
    pub extension_title: String,
    pub title: String,
    pub subtitle: String,
    pub mode: String,
    pub entrypoint: String,
    /// Only when it differs from `entrypoint`.
    ///
    /// Most entries take their icon from the thing they launch, and the window
    /// already falls back to `entrypoint` when this is absent. Sending the
    /// same path twice per row was the single largest field in the payload,
    /// and paths are the longest strings a result carries.
    pub icon: Option<String>,
    pub panel: Option<String>,
    /// Whether this row is a switch, and which way it is set.
    ///
    /// Filled in when the results are built rather than carried by the index:
    /// what a switch is set to is a fact about the moment somebody looks at
    /// it, and a value from the last scan would be a guess.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
    /// Indices into `title` that matched, so the UI can highlight them.
    pub matched: Vec<usize>,
    /// The name the user gave this, when they gave it one.
    ///
    /// Shown on the row, which is the only way an alias is discoverable: a
    /// name nobody can see is one nobody remembers they set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    /// Whether the query named this rather than merely fitting it.
    ///
    /// The window runs more than one search and shows the answers in one list,
    /// so it needs to know which results were found by name. Without it, a
    /// query that nothing in the index really matches still buries an emoji
    /// somebody typed the name of under eighty near-misses.
    ///
    /// Absent rather than false on the wire. Most results in a long list are
    /// not strong, and this is serialised on every keystroke.
    #[serde(skip_serializing_if = "is_not_strong")]
    pub strong: bool,
}

/// Whether to leave `strong` out of the payload.
fn is_not_strong(strong: &bool) -> bool {
    !*strong
}

impl From<RankedCommand> for SearchResult {
    fn from(ranked: RankedCommand) -> Self {
        let RankedCommand {
            command,
            matched,
            class,
            // Ranking is finished by the time this conversion happens, and the
            // order results arrive in is the only thing the UI needs to know
            // about it.
            score: _,
        } = ranked;

        // `command.command` is not here on purpose. It is the manifest name a
        // command was declared under, which matching uses and nothing in the
        // window ever reads.
        let icon = command.icon.filter(|icon| *icon != command.entrypoint);

        Self {
            id: command.id,
            extension: command.extension,
            extension_title: command.extension_title,
            title: command.title,
            subtitle: command.subtitle,
            mode: command.mode,
            entrypoint: command.entrypoint,
            icon,
            // Set for a Windows switch and nothing else, and set by whoever
            // is about to draw the row rather than by the index, because how
            // a switch is set is a fact about right now.
            toggle: command.toggle,
            panel: command.panel,
            matched,
            // Filled by the caller, which is the only place that knows them.
            alias: None,
            strong: is_strong(class),
        }
    }
}

impl SearchResult {
    /// A result for something that was never ranked.
    ///
    /// The switcher's empty query, where enumeration order is already the
    /// answer. Going through `RankedCommand` would mean inventing a score to
    /// throw away and claiming a match that never happened.
    pub fn from_record(command: CommandRecord) -> Self {
        let icon = command.icon.filter(|icon| *icon != command.entrypoint);

        Self {
            id: command.id,
            extension: command.extension,
            extension_title: command.extension_title,
            title: command.title,
            subtitle: command.subtitle,
            mode: command.mode,
            entrypoint: command.entrypoint,
            icon,
            // Set for a Windows switch and nothing else, and set by whoever
            // is about to draw the row rather than by the index, because how
            // a switch is set is a fact about right now.
            toggle: command.toggle,
            panel: command.panel,
            // Nothing was typed, so nothing matched.
            matched: Vec::new(),
            alias: None,
            // Nothing was typed, so nothing was named.
            strong: false,
        }
    }
}

/// Turns a discovered application into a registry entry.
///
/// An app is just another thing you can launch, so it is expressed as a
/// `CommandRecord` with `mode: "app"` rather than as a parallel type. Search,
/// ranking and frecency then apply to apps and commands identically, with no
/// second code path to keep in step.
pub fn app_record(name: &str, path: &str, icon: Option<String>, kind: &str) -> CommandRecord {
    app_entry(name, path, icon, "app", kind)
}

/// A bare executable found on `%PATH%`.
///
/// Kept distinct from an application because there are an order of magnitude
/// more of them and they are mostly command-line tools. Same launch path, but
/// ranked below real applications so a CLI utility never displaces the app
/// someone was reaching for.
pub fn executable_record(name: &str, path: &str, kind: &str) -> CommandRecord {
    let mut record = app_entry(name, path, Some(path.to_string()), "exe", kind);

    // Where it is, because that is the only thing telling two of them apart.
    // A machine with three Pythons on PATH shows three identical rows without
    // it, and which one runs is decided by an order nobody can see.
    //
    // Applications are left blank on purpose: there is one Chrome, its
    // category already shows on the right, and a path under every row is a
    // second line of noise for a question nobody asked.
    record.subtitle = path.to_string();
    record
}

fn app_entry(
    name: &str,
    path: &str,
    icon: Option<String>,
    mode: &str,
    kind: &str,
) -> CommandRecord {
    CommandRecord {
        id: format!("app:{path}"),
        extension: "app".to_string(),
        extension_title: kind.to_string(),
        command: name.to_string(),
        title: name.to_string(),
        subtitle: String::new(),
        description: String::new(),
        mode: mode.to_string(),
        entrypoint: path.to_string(),
        keywords: Vec::new(),
        icon,
        // A Windows application, not one of Sill's panels.
        toggle: None,
        panel: None,
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
    }
}

/// Reads the previously scanned index.
///
/// Discovery costs a PowerShell round trip and a few thousand filesystem
/// calls, which is a second or so of the launcher being half-populated on
/// every start. The cache makes the full list available immediately; the scan
/// still runs behind it and replaces this with fresh results.
/// The index, with each id appearing once.
///
/// Every source above is already deduplicated in whatever terms suit it:
/// extension indexes by id, applications by name and by the binary they run.
/// Nothing looked at the finished list, and that gap is what let four Windows
/// settings pages into the index under one id.
///
/// An id is not a label. It is what an alias, a hotkey, a hidden entry and a
/// frecency score are all keyed on, so two rows sharing one id share all four:
/// running either promotes both, hiding either hides both. It is also the
/// identity the result list is drawn by, where a repeat is not a duplicated row
/// but a thrown error that costs the whole list.
///
/// So it is checked in the two places an index can arrive from, a fresh scan
/// and the cache the last one left, and nowhere else has to remember to. The
/// first of a repeated id wins, because sources are added in order of how much
/// they are trusted: Sill's own commands, then installed extensions, then the
/// catalog, then whatever was found on disk.
pub fn one_per_id(records: Vec<CommandRecord>) -> Vec<CommandRecord> {
    let before = records.len();
    let mut seen = std::collections::HashSet::with_capacity(before);

    let out: Vec<CommandRecord> = records
        .into_iter()
        .filter(|record| seen.insert(record.id.clone()))
        .collect();

    if out.len() != before {
        crate::say!(
            "{} entries were left out for sharing an id with an earlier one",
            before - out.len()
        );
    }

    out
}

pub fn load_cache(path: &Path) -> Vec<CommandRecord> {
    let cached: Vec<CommandRecord> = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();

    // Checked on the way in as well as on the way out. A cache written before
    // the scan enforced this is still on disk, and is read at every start until
    // the first rescan finishes behind it.
    one_per_id(cached)
}

/// Writes the index for the next start.
pub fn save_cache(path: &Path, commands: &[CommandRecord]) -> std::io::Result<()> {
    match cache_text(commands) {
        Some(text) => write_cache(path, &text),
        None => Ok(()),
    }
}

/// The index as text, ready to be written by somebody who is not holding a lock.
///
/// `None` when it will not serialise, which is not a thing that happens and is
/// still not worth writing an empty cache over: a missing cache costs a slower
/// next start, and an empty one would be read as an index with nothing in it.
pub fn cache_text(commands: &[CommandRecord]) -> Option<String> {
    serde_json::to_string(commands).ok()
}

/// Puts an already-serialised index on disk.
///
/// Staged and renamed. Half a megabyte written in place is half a megabyte of
/// window in which the file is neither the old index nor the new one, and what
/// is read back from a torn one is an empty root list on the next start.
pub fn write_cache(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let staging = path.with_extension("json.partial");
    std::fs::write(&staging, text)?;
    if let Err(err) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(err);
    }
    Ok(())
}

/// Where the scanned index is cached, given the app's data directory.
pub fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("index-cache.json")
}

/// Sill's own commands.
///
/// These are the things the launcher can do to itself. They belong in the root
/// list rather than behind a shortcut: a setting nobody can find is a setting
/// nobody has.
pub fn builtins() -> Vec<CommandRecord> {
    let mut rows = vec![
        builtin(
            "settings",
            "general",
            "Sill Settings",
            "Hotkey, appearance, sources and file search",
            &["preferences", "options", "configure", "hotkey"],
        ),
        builtin(
            "reload",
            "advanced",
            "Reload Sill Index",
            "Rescan applications, settings and shortcuts",
            &["refresh", "rescan", "reindex"],
        ),
        builtin(
            "snippets",
            "snippets",
            "Snippets",
            "Saved text, expanded by keyword or pasted from here",
            &["snippet", "template", "expand", "abbreviation", "text"],
        ),
        builtin(
            "save-workspace",
            "advanced",
            "Save Workspace",
            "Remember where every window is, to put them back later",
            &[
                "workspace",
                "layout",
                "arrange",
                "save",
                "windows",
                "session",
            ],
        ),
        builtin(
            "widgets",
            "widgets",
            "Widgets",
            "The clock, the weather, and what this machine is doing",
            &[
                "widget",
                "clock",
                "time",
                "weather",
                "temperature",
                "forecast",
                "process",
                "memory",
                "ram",
                "cpu",
                "monitor",
                "dashboard",
            ],
        ),
        builtin(
            "undo-last",
            "advanced",
            "Undo Last Action",
            "Take back the last thing Sill did that can be taken back",
            &["undo", "revert", "back", "mistake", "reverse"],
        ),
        builtin(
            "store",
            "extensions",
            "Extension Store",
            "Browse Raycast extensions and install them",
            &[
                "extension",
                "store",
                "raycast",
                "browse",
                "install",
                "add",
                "plugin",
                "marketplace",
                "catalog",
                "discover",
                "search",
            ],
        ),
        builtin(
            "store-updates",
            "extensions",
            "Update Extensions",
            "Check which installed extensions have a newer version",
            &[
                "extension",
                "update",
                "upgrade",
                "outdated",
                "newer",
                "version",
                "store",
                "refresh",
            ],
        ),
        builtin(
            "install-extension",
            "advanced",
            "Install Extension",
            "Build a Raycast extension from a folder and add its commands",
            &[
                "extension",
                "raycast",
                "add",
                "plugin",
                "import",
                "build",
                "folder",
            ],
        ),
        builtin(
            "quicklinks",
            "quicklinks",
            "Quicklinks",
            "Saved links that take what you type",
            &["link", "url", "bookmark", "search", "open", "web"],
        ),
        builtin(
            "clipboard",
            "clipboard",
            "Clipboard History",
            "Everything you have copied, searchable",
            &["paste", "copy", "history", "recent", "pasteboard"],
        ),
        builtin(
            "ask",
            "ai",
            "AI Chat",
            "Talk to a model that can see this machine, with room to think",
            &[
                "ai",
                "chat",
                "ask",
                "conversation",
                "model",
                "question",
                "talk",
                "assistant",
            ],
        ),
        builtin(
            "conversations",
            "ai",
            "Past Conversations",
            "Every AI chat you have had, to reopen or forget",
            &[
                "ask",
                "ai",
                "chat",
                "history",
                "conversation",
                "asked",
                "model",
                "answer",
            ],
        ),
        builtin(
            "capture-area",
            "clipboard",
            "Capture Area",
            "Drag a rectangle and copy it",
            &[
                "screenshot",
                "screen",
                "shot",
                "grab",
                "snip",
                "region",
                "area",
                "crop",
                "capture",
            ],
        ),
        builtin(
            "capture-screen",
            "clipboard",
            "Capture Whole Screen",
            "Copies everything on every display",
            &[
                "screenshot",
                "screen",
                "shot",
                "grab",
                "fullscreen",
                "display",
                "capture",
            ],
        ),
        builtin(
            "mark-up",
            "clipboard",
            "Mark Up Last Image",
            "Draw on the last picture you copied",
            &[
                "annotate",
                "markup",
                "draw",
                "arrow",
                "highlight",
                "redact",
                "blur",
                "screenshot",
                "edit image",
            ],
        ),
        builtin(
            "extract-text",
            // Filed with the clipboard, because that is where the picture it
            // reads comes from and where the words it finds go.
            "clipboard",
            "Extract Text from Image",
            "Reads the words out of the last picture you copied",
            &[
                "ocr",
                "text",
                "read",
                "recognise",
                "recognize",
                "image",
                "picture",
                "screenshot",
                "scan",
                "copy text",
            ],
        ),
        // Wearing the volume mixer's own mark rather than a panel's.
        //
        // What it opens is a list of Windows' audio sessions, not one of
        // Sill's settings pages, and a row wearing the settings gear would say
        // it belongs to Sill. Same rule the switches follow.
        builtin_wearing(
            "appVolume",
            &mixer_icon(),
            "App Volume",
            "Turn one program down without turning everything down",
            &[
                "volume", "mixer", "app", "program", "per app", "mute", "loud", "quiet", "sound",
                "audio",
            ],
        ),
        builtin(
            "emoji",
            "snippets",
            "Emoji",
            "Search every emoji by name and paste one",
            &[
                "emoji", "symbol", "smiley", "face", "icon", "unicode", "reaction",
            ],
        ),
        builtin(
            "dictate",
            "dictation",
            "Dictate",
            "Start a dictation without the hotkey",
            &[
                "voice",
                "speech",
                "talk",
                "transcribe",
                "microphone",
                "whisper",
            ],
        ),
        builtin(
            "dictation-history",
            "dictation",
            "Dictation History",
            "Every transcript, newest first",
            &["voice", "speech", "transcripts", "past", "log"],
        ),
        builtin(
            "last-transcription",
            "dictation",
            "Get Last Transcription",
            "Copy the most recent transcript",
            &["voice", "speech", "again", "repeat", "copy"],
        ),
        builtin(
            "vocabulary",
            "dictation",
            "Dictation Vocabulary",
            "Words and names dictation should always get right",
            &["voice", "speech", "terms", "jargon", "names"],
        ),
    ];

    // Folded in rather than kept apart, so a system switch ranks, takes an
    // alias, takes a hotkey and can be hidden exactly like every other row.
    rows.extend(system_commands());
    rows
}

/// The system switches, as rows.
///
/// Titles name the action rather than the state, so nothing here has to ask
/// the machine what it is currently doing. A row reading "Unmute" would need
/// the audio endpoint queried to know whether to say it, and the index is
/// built at startup and searched on every keystroke: neither is a place to put
/// a COM round trip for a word.
///
/// What actually happened is said afterwards instead. "Sound off" once it is
/// off is better than "Unmute" beforehand, because it is a fact rather than a
/// prediction, and it is right even when something else changed it in between.
///
/// Keywords carry the words people reach for. Somebody wanting silence types
/// "mute" or "quiet", somebody going away types "lock", and neither should
/// have to learn what Sill decided to call it.
fn system_commands() -> Vec<CommandRecord> {
    // Windows' own icons, from the programs that own each switch. A speaker
    // from the volume mixer, the personalisation applet's monitor, and the
    // shell's padlock. Nothing drawn here: the point of these rows is that
    // they change Windows rather than Sill, and wearing Sill's gear would say
    // the opposite.
    //
    // The padlock is `imageres.dll,54`, which is a resource reference of the
    // kind Windows writes everywhere and `icons` now reads. Chosen by
    // extracting the range and looking at it rather than by trusting a number
    // from somewhere.
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let audio = mixer_icon();
    let theme = format!(r"{root}\System32\themecpl.dll");
    let padlock = format!(r"{root}\System32\imageres.dll,54");
    let network = format!(r"{root}\System32\ncpa.cpl");
    /*
     * Index 2, and the index is the whole point.
     *
     * `bthprops.cpl` with no index means index 0, and index 0 of that file is
     * a **yellow warning triangle**. So the Bluetooth switch sat in the list
     * wearing an error sign on a machine where nothing was wrong.
     *
     * Index 2 is the Bluetooth glyph. It was found by extracting all three and
     * looking at them, which is the only way that works: the file's icons come
     * back at 386, 2887 and 426 bytes, so the smallest is the triangle and the
     * second smallest is the right one. Size does not say which is which.
     */
    let bluetooth = format!(r"{root}\System32\bthprops.cpl,2");

    let mut rows = vec![
        system_switch(
            "system.volume.up",
            &audio,
            "Volume Up",
            "Ten percent louder",
            &["louder", "sound", "audio", "increase", "system"],
        ),
        system_switch(
            "system.volume.down",
            &audio,
            "Volume Down",
            "Ten percent quieter",
            &["quieter", "sound", "audio", "decrease", "lower", "system"],
        ),
        system_switch(
            "system.volume.half",
            &audio,
            "Volume 50%",
            "Sets the volume to half",
            &["half", "sound", "audio", "system"],
        ),
        system_switch(
            "system.volume.max",
            &audio,
            "Volume 100%",
            "Sets the volume to full",
            &["full", "max", "loud", "sound", "audio", "system"],
        ),
        system_switch(
            "system.mute",
            &audio,
            "Toggle Mute",
            "Silences Windows, or brings the sound back",
            &[
                "mute", "unmute", "silence", "quiet", "sound", "audio", "system",
            ],
        ),
        system_switch(
            "system.theme",
            &theme,
            "Toggle Dark Mode",
            "Switches Windows between light and dark",
            &[
                "dark",
                "light",
                "theme",
                "appearance",
                "night",
                "mode",
                "system",
            ],
        ),
        system_switch(
            "system.lock",
            &padlock,
            "Lock Screen",
            "Locks Windows straight away",
            &["lock", "away", "screen", "afk", "system"],
        ),
    ];

    /*
     * One row per thing sound can come out of.
     *
     * Built here, when the index is, so they cost nothing per keystroke. The
     * price is that plugging in headphones does not add a row until the next
     * scan, which is the trade the whole index makes.
     *
     * The one in use is still listed. Choosing it is a no-op that says which
     * it is, and hiding it would mean the list never answers "what am I on".
     */
    for output in crate::audio::outputs() {
        let name = crate::audio::short_name(&output.name);
        let id = format!("{}{}", crate::actions::AUDIO_OUTPUT, output.id);

        rows.push(system_switch(
            &id,
            &audio,
            &name,
            // The name Windows gives it in full, which is the part the title
            // drops. "Speakers" and "Speakers" are two rows that look the
            // same until the card each one is on is written underneath.
            //
            // Not "sound is going here": the switch on the row says that, and
            // saying it twice is not saying it better.
            &if output.name.trim() == name {
                String::new()
            } else {
                output.name.trim().to_string()
            },
            &[
                "audio",
                "sound",
                "output",
                "speakers",
                "headphones",
                "device",
                "system",
            ],
        ));
    }

    /*
     * One row per radio, if the machine has any.
     *
     * Named for what pressing it does rather than for the radio, because the
     * row is a switch: "Wi-Fi" alone reads as somewhere to go, and the settings
     * catalog already has several of those.
     */
    for radio in crate::radios::radios() {
        let id = format!("{}{}", crate::actions::RADIO, radio.kind);

        rows.push(system_switch(
            &id,
            // The network applet for one and the Bluetooth applet for the
            // other, which is what Windows draws them with.
            if radio.kind == "wifi" {
                &network
            } else {
                &bluetooth
            },
            &format!(
                "Turn {} {}",
                radio.name,
                if radio.on { "Off" } else { "On" }
            ),
            // Nothing. The switch on the row says whether it is on, and a
            // radio has nothing else to tell you about itself.
            "",
            /*
             * The words for this radio only.
             *
             * Both rows shared one list at first, which put the Bluetooth
             * switch above the Wi-Fi one for the query "wifi": the word was an
             * exact keyword on both, and an exact keyword outranks the
             * subsequence match that "wifi" makes against "Turn Wi-Fi Off".
             *
             * The unhyphenated spelling was here because the hyphen used to
             * make "wifi" a scattered subsequence against "Turn Wi-Fi Off".
             * The matcher now reads through a mark that joins one word, so the
             * title matches on its own; the keyword is kept because it costs a
             * string and still answers for the other spellings.
             */
            &if radio.kind == "wifi" {
                [
                    "wifi", "wi-fi", "wireless", "wlan", "internet", "network", "radio", "system",
                ]
            } else {
                [
                    "bluetooth",
                    "bt",
                    "wireless",
                    "pair",
                    "headphones",
                    "radio",
                    "toggle",
                    "system",
                ]
            },
        ));
    }

    rows
}

/// One of Windows' switches, shaped as a row.
///
/// Its own mode rather than `builtin`, so it groups apart and so an action can
/// accept it without accepting everything Sill does to itself. Its icon comes
/// from whichever Windows program owns the switch, because a row that changes
/// the machine should not be wearing the launcher's badge.
fn system_switch(
    id: &str,
    icon: &str,
    title: &str,
    subtitle: &str,
    keywords: &[&str],
) -> CommandRecord {
    CommandRecord {
        id: format!("sill:{id}"),
        extension: "system".to_string(),
        extension_title: "System".to_string(),
        command: id.to_string(),
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        description: String::new(),
        mode: "system".to_string(),
        entrypoint: id.to_string(),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        icon: Some(icon.to_string()),
        // No panel. These do not appear in settings, and naming one would be
        // borrowing an icon that says the wrong thing anyway.
        toggle: None,
        panel: None,
        preferences: serde_json::Value::Null,
    }
}

/// Where the volume mixer keeps its icon.
///
/// Its own function because both this and the volume switches want it, and
/// they are built in different places.
fn mixer_icon() -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    format!(r"{root}\System32\SndVol.exe")
}

/// One of Sill's own commands, wearing a mark of its own rather than a panel's.
///
/// A row either names a settings panel and wears that panel's mark, or brings
/// its own. Anything that reaches outside Sill brings its own, because the
/// settings gear would say the thing belongs to Sill when it does not.
fn builtin_wearing(
    id: &str,
    icon: &str,
    title: &str,
    subtitle: &str,
    keywords: &[&str],
) -> CommandRecord {
    CommandRecord {
        panel: None,
        icon: Some(icon.to_string()),
        ..builtin(id, "general", title, subtitle, keywords)
    }
}

/// How a builtin's id is spelled, for anything that has to name one.
///
/// Shared rather than formatted again wherever it is needed. A second copy of
/// the format is a second thing to change, and the way it fails is silent: an
/// id that matches no row updates nothing and reports nothing.
pub fn builtin_id(id: &str) -> String {
    format!("sill:{id}")
}

fn builtin(id: &str, panel: &str, title: &str, subtitle: &str, keywords: &[&str]) -> CommandRecord {
    CommandRecord {
        id: builtin_id(id),
        extension: "sill".to_string(),
        extension_title: "Sill".to_string(),
        command: id.to_string(),
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        description: String::new(),
        mode: "builtin".to_string(),
        entrypoint: id.to_string(),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        icon: None,
        toggle: None,
        panel: Some(panel.to_string()),
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
    }
}

/// A folder offered as somewhere to move something, shaped as a row.
///
/// The title is the folder's own name and the subtitle is the path, which is
/// the pair that tells three folders called "src" apart. The same shape a file
/// result uses, because it is the same question with a different answer.
pub fn destination_record(folder: &str) -> CommandRecord {
    let path = std::path::Path::new(folder);
    let name = crate::files_ops::name_of(path);

    CommandRecord {
        id: format!("destination:{folder}"),
        extension: "destination".to_string(),
        extension_title: "Folders".to_string(),
        command: name.clone(),
        title: name,
        subtitle: folder.to_string(),
        description: String::new(),
        mode: "destination".to_string(),
        entrypoint: folder.to_string(),
        keywords: Vec::new(),
        // Nothing separate to point at. The row's own path is what the shell
        // is asked about, the way a file's is, and setting it here as well
        // would be dropped: a record's icon is thrown away when it is the same
        // string as its entrypoint, because that means "there is nothing extra
        // to look at".
        icon: None,
        toggle: None,
        panel: None,
        preferences: serde_json::Value::Null,
    }
}

/// One program's volume, shaped as a row.
///
/// The switch shows whether you can hear it, not whether it is muted, because
/// **the switch answers the row's title**. The system row is called "Toggle
/// Mute" so its switch says whether mute is on; this row is called by the
/// program's name, so its switch says whether the program is.
/// One running program, shaped as a row.
///
/// The subtitle carries what it costs, because that is the column somebody
/// came to read. Sizes in whole units: a process list is scanned, not
/// audited, and "412 MB" is read faster than "412.7 MB".
pub fn process_record(process: &crate::processes::Process) -> CommandRecord {
    let mb = process.bytes / 1_048_576;

    let subtitle = if process.visible {
        format!("{mb} MB, has a window")
    } else {
        format!("{mb} MB")
    };

    CommandRecord {
        id: format!("process:{}", process.pid),
        extension: "process".to_string(),
        extension_title: "Processes".to_string(),
        command: process.name.clone(),
        title: process.name.clone(),
        subtitle,
        description: String::new(),
        mode: "process".to_string(),
        // The pid, which is what every action here needs and what stops
        // meaning anything the moment it exits.
        entrypoint: process.pid.to_string(),
        keywords: vec![
            "process".to_string(),
            "memory".to_string(),
            "quit".to_string(),
        ],
        // The program's own mark, the rule every other row follows.
        icon: process.path.clone(),
        toggle: None,
        panel: None,
        preferences: serde_json::Value::Null,
    }
}

pub fn audio_session_record(session: &crate::app_volume::Session) -> CommandRecord {
    let percent = (session.volume * 100.0).round() as i32;

    CommandRecord {
        id: format!("audio-session:{}", session.id),
        extension: "audio-session".to_string(),
        extension_title: "App Volume".to_string(),
        command: session.name.clone(),
        title: session.name.clone(),
        subtitle: if session.muted {
            format!("Muted, was at {percent}%")
        } else {
            format!("{percent}%")
        },
        description: String::new(),
        mode: "audio-session".to_string(),
        // Windows' identifier for the session, which is what finds it again.
        entrypoint: session.id.clone(),
        keywords: vec!["volume".to_string(), "mute".to_string()],
        // The program itself, so the row wears its mark rather than Sill's.
        icon: (!session.path.is_empty()).then(|| session.path.clone()),
        toggle: Some(!session.muted),
        panel: None,
        preferences: serde_json::Value::Null,
    }
}

/// A script command, shaped as a command like everything else.
///
/// The package name is the subtitle when there is one, because a folder of
/// scripts from one place is how people organise them and it is what tells two
/// scripts called "Deploy" apart.
pub fn script_record(script: &crate::scripts::Script) -> CommandRecord {
    let id = script.path.to_string_lossy().to_string();

    CommandRecord {
        id: format!("script:{id}"),
        extension: "scripts".to_string(),
        extension_title: "Script".to_string(),
        command: id.clone(),
        title: script.title.clone(),
        subtitle: script
            .package
            .clone()
            .or_else(|| script.description.clone())
            .unwrap_or_else(|| {
                script
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            }),
        description: script.description.clone().unwrap_or_default(),
        // Same shape as a quicklink: whether it stops to ask rides in the mode,
        // because the launcher has to know that before it opens anything.
        mode: if script.needs_argument {
            "script-arg".to_string()
        } else {
            "script".to_string()
        },
        entrypoint: id,
        keywords: Vec::new(),
        // One or the other, never both. A row that carries an icon and a
        // panel leaves the launcher guessing which mark to wear, and a script
        // that named an emoji in its header meant that one.
        icon: script.icon.clone(),
        toggle: None,
        panel: script.icon.is_none().then(|| "scripts".to_string()),
        preferences: serde_json::Value::Null,
    }
}

/// A quicklink, shaped as a command so the ranker treats it like anything
/// else.
///
/// `needs_argument` rides along in the mode rather than as another field: the
/// launcher has to know before it opens anything whether to ask for a query
/// or go straight there, and that is the only thing it needs to know.
pub fn quicklink_record(
    id: &str,
    name: &str,
    keyword: &str,
    link: &str,
    needs_argument: bool,
) -> CommandRecord {
    CommandRecord {
        id: format!("quicklink:{id}"),
        extension: "quicklinks".to_string(),
        extension_title: "Quicklink".to_string(),
        command: id.to_string(),
        title: name.to_string(),
        // The target is the useful subtitle: two links called "Search" are
        // told apart by where they go, not by their names.
        subtitle: link.to_string(),
        description: String::new(),
        mode: if needs_argument {
            "quicklink-arg".to_string()
        } else {
            "quicklink".to_string()
        },
        entrypoint: id.to_string(),
        keywords: if keyword.is_empty() {
            Vec::new()
        } else {
            vec![keyword.to_string()]
        },
        icon: None,
        toggle: None,
        panel: Some("quicklinks".to_string()),
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
    }
}

/// A snippet, shaped as a command so the ranker treats it like anything else.
pub fn snippet_record(snippet: &crate::snippets::store::Snippet) -> CommandRecord {
    let id = &snippet.id;
    let keyword = snippet.keyword.trim();
    let preview = &snippet.content;
    let collection = snippet.collection.trim();

    CommandRecord {
        id: format!("snippet:{id}"),
        extension: "snippets".to_string(),
        // The group it is drawn under. A collection is a heading and nothing
        // else, so it goes where a heading goes rather than into a field of
        // its own that the window would have to be taught to read.
        extension_title: if collection.is_empty() {
            "Snippets".to_string()
        } else {
            collection.to_string()
        },
        command: id.to_string(),
        title: snippet.name.to_string(),
        // The keyword is worth showing: it is how the snippet is used when
        // the launcher is not open, and there is nowhere else to learn it.
        subtitle: if keyword.is_empty() {
            preview.to_string()
        } else {
            keyword.to_string()
        },
        description: String::new(),
        mode: "snippet".to_string(),
        entrypoint: id.to_string(),
        // Searchable by what is in it and by the group it is in, not only by
        // what it is called.
        keywords: vec![
            keyword.to_string(),
            preview.to_string(),
            collection.to_string(),
        ],
        icon: None,
        toggle: None,
        panel: None,
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
    }
}

/// A calculator answer, shaped as a result so the list needs no special case.
///
/// Scored far above anything the ranker can produce, because when a query is
/// a sum the answer is the only thing being asked for.
/// The conversation you left, as a row offering it back.
///
/// A record rather than a row spliced in at the top, so that typing finds it
/// the way typing finds everything else. That matters more than it sounds:
/// Escape out of a conversation puts your search back in the field, and the
/// search is usually the question you asked, so the row you want is found by
/// the words already there.
pub fn conversation_record(id: &str, title: &str, said: &str) -> CommandRecord {
    CommandRecord {
        id: format!("sill:{id}"),
        extension: "sill".to_string(),
        extension_title: "Continue".to_string(),
        command: "conversation".to_string(),
        title: title.to_string(),
        subtitle: said.to_string(),
        description: String::new(),
        mode: "conversation".to_string(),
        // Which conversation to reopen. The row's own id carries a prefix so
        // it cannot collide with anything scanned; this is the bare one.
        entrypoint: id.to_string(),
        // Nobody types "conversation" to find one. They type the question, and
        // the question is the title.
        keywords: Vec::new(),
        icon: None,
        toggle: None,
        // Wears the mark of the panel it belongs to, like everything else Sill
        // owns.
        panel: Some("ai".to_string()),
        preferences: serde_json::Value::Null,
    }
}

pub fn answer_record(text: &str, input: &str) -> RankedCommand {
    RankedCommand {
        // An answer is only ever produced because the query was a sum, so it
        // is always exactly what was asked for.
        class: MatchClass::ExactTitle,
        command: CommandRecord {
            id: "sill:answer".to_string(),
            extension: "sill".to_string(),
            extension_title: "Calculator".to_string(),
            command: "answer".to_string(),
            title: text.to_string(),
            subtitle: input.to_string(),
            description: String::new(),
            mode: "answer".to_string(),
            // What Enter copies.
            entrypoint: text.to_string(),
            keywords: Vec::new(),
            icon: None,
            toggle: None,
            panel: None,
            // Only extension commands carry any.
            preferences: serde_json::Value::Null,
        },
        score: i64::MAX,
        matched: Vec::new(),
    }
}

pub fn load_index(path: &Path) -> Vec<CommandRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

// ---------------------------------------------------------------- matching

/// Subsequence match with positional scoring, in the spirit of fzf.
///
/// Returns the score and the matched indices, or `None` if the query is not a
/// subsequence of the haystack at all.
///
/// The weighting is what makes a launcher feel like it reads your mind:
/// consecutive characters and word starts are worth far more than scattered
/// hits, so "vh" finds "View History" ahead of anything that merely contains a
/// v and an h.
/// The same, against a needle prepared once for the whole search.
///
/// Preparing it per candidate is what this exists to avoid. Lowercasing the
/// query and collecting it into a vector is three allocations, and the old
/// shape paid them **four times for every entry in the index**: once for the
/// title, once for the extension name, once per keyword, and once more in the
/// classifier. Across sixteen hundred entries that is thousands of
/// allocations for a single keystroke.
fn fuzzy_with(needle: &[char], haystack: &str) -> Option<(i64, Vec<usize>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }

    let hay: Vec<char> = haystack.chars().collect();
    let hay_lower: Vec<char> = haystack.to_lowercase().chars().collect();

    // Guard: lowercasing can change length for some scripts, and the indices
    // below are handed to the UI to slice the original string.
    if hay_lower.len() != hay.len() {
        return simple_subsequence(needle, &hay);
    }

    let mut score = 0i64;
    let mut matched = Vec::with_capacity(needle.len());
    let mut hay_i = 0usize;
    let mut previous_match: Option<usize> = None;

    for &want in needle {
        let found = (hay_i..hay_lower.len()).find(|&i| hay_lower[i] == want)?;

        let mut points = 1i64;

        // Start of the string, or of a word.
        let boundary = found == 0
            || matches!(hay[found - 1], ' ' | '-' | '_' | '.' | '/' | ':')
            || (hay[found].is_uppercase() && hay[found - 1].is_lowercase());
        if boundary {
            points += 8;
        }

        // Directly after the previous match.
        if previous_match == Some(found.saturating_sub(1)) && found > 0 {
            points += 6;
        }

        // Earlier is better, mildly.
        points += ((32 - found.min(32)) / 8) as i64;

        score += points;
        matched.push(found);
        previous_match = Some(found);
        hay_i = found + 1;
    }

    // A query covering most of the target beats one that covers a little.
    let coverage = (needle.len() * 10 / hay.len().max(1)) as i64;
    score += coverage;

    Some((score, matched))
}

/// Fallback for strings whose length changes when lowercased.
fn simple_subsequence(needle: &[char], hay: &[char]) -> Option<(i64, Vec<usize>)> {
    let mut matched = Vec::new();
    let mut hay_i = 0;

    for &want in needle {
        let found = (hay_i..hay.len()).find(|&i| hay[i].to_lowercase().next() == Some(want))?;
        matched.push(found);
        hay_i = found + 1;
    }

    Some((matched.len() as i64, matched))
}

// --------------------------------------------------------------- frecency

/// How often and how recently each command was launched.
///
/// Frequency alone entrenches whatever was popular last month; recency alone
/// forgets a daily habit after one detour. The combination is what makes the
/// top result usually correct without the user typing anything.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Frecency {
    /// id -> (launch count, last launch as unix seconds)
    #[serde(default)]
    entries: HashMap<String, (u32, i64)>,
    /// query -> id -> (times chosen for that query, last time)
    ///
    /// What the user meant, as opposed to what they opened. Typing `ggm` and
    /// choosing Gmail says something the id alone does not: not "Gmail is
    /// popular" but "`ggm` means Gmail". That is an alias nobody had to sit
    /// down and configure, and it is the thing people miss most when they
    /// move between launchers.
    ///
    /// Nested rather than keyed on a joined string, because the lookup is per
    /// query and happens once per search rather than once per candidate: the
    /// inner map is fetched once and then asked about each id.
    #[serde(default)]
    learned: HashMap<String, HashMap<String, (u32, i64)>>,
    /// Queries that reached something, most recent first.
    ///
    /// Separate from `learned`, which answers "what does this query mean".
    /// This answers "what did I type last", which is a different question and
    /// needs an order rather than a map.
    ///
    /// Only queries that led somewhere. A shell recalls everything typed
    /// including the typos; a launcher recalling the half-finished strings
    /// somebody abandoned would mostly offer back their mistakes.
    #[serde(default)]
    history: Vec<String>,
}

/// How many past queries are kept.
///
/// Enough to hold a working session and short enough that walking back
/// through it stays faster than retyping. Beyond that, searching is the answer
/// rather than pressing Up forty times.
const HISTORY: usize = 50;

/// How many times one query has to reach one command before it counts.
///
/// Two, not one. Once is a keystroke that could have been a mistake, and a
/// single stray Enter would silently reorder a list for good. Twice is a
/// habit, and it still arrives on the second use rather than the tenth.
///
/// The first use is not wasted either: it already counts towards ordinary
/// frecency, which moves the result up within its match class.
pub const LEARNED_AT: u32 = 2;

/// The longest query worth remembering.
///
/// An abbreviation is short by definition. Somebody who typed the whole name
/// has already found the thing, and remembering that teaches nothing while
/// growing the file with one entry per keystroke of every long search.
const LEARNED_MAX_LEN: usize = 24;

/// How many distinct queries are kept.
///
/// Bounded because it is written to disk and read on every launch, and an
/// unbounded map of everything ever typed is exactly the kind of quiet growth
/// rule 23 exists to stop. The oldest are dropped first.
const LEARNED_QUERIES: usize = 400;

/// How many launched entries are kept.
///
/// Bounded for the same reason `learned` is: this is written on every launch
/// and read at every start, and a map of everything ever opened only grows.
/// Two thousand is far more than anybody reaches for and small enough that the
/// file stays a few hundred kilobytes at worst.
const REMEMBERED: usize = 2_000;

/// How long a single launch is remembered.
///
/// Opening something once is not a habit, and after three months it is not
/// even a memory. Anything launched twice is kept regardless of age, because
/// twice is the same threshold `LEARNED_AT` uses to call something deliberate.
const ONE_OFF_FADES_AFTER: i64 = 60 * 60 * 24 * 90;

/// Recency buckets, in seconds, and what each is worth.
const RECENCY_TIERS: [(i64, i64); 5] = [
    (60 * 60, 100),          // within the hour
    (60 * 60 * 24, 70),      // today
    (60 * 60 * 24 * 7, 40),  // this week
    (60 * 60 * 24 * 30, 20), // this month
    (i64::MAX, 5),           // ever
];

impl Frecency {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /**
    Writes the ranking history.

    Compact rather than pretty, and staged rather than written in place.

    Pretty printing put a newline and an indent around every one of what can be
    thousands of entries, and this is written on **every launch**, on the
    registry lock, with the next keystroke waiting behind it. Nobody reads this
    file by hand; the one that people do read, `preferences.json`, is still
    printed properly.

    Staged and renamed for the same reason preferences are: an interrupted
    write left a truncated file, and a truncated file parses as nothing, which
    silently resets everybody's ranking to "never launched".
    */
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let text = serde_json::to_string(self).unwrap_or_else(|_| "{}".into());
        Self::write(path, &text)
    }

    /// Puts already-serialised history on disk.
    ///
    /// Split from `save` so a caller holding a lock can serialise under it and
    /// write outside it, which is what `launch_command` does: the write used to
    /// happen on the lock the next keystroke waits for.
    pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let staging = path.with_extension("json.partial");
        std::fs::write(&staging, text)?;
        if let Err(err) = std::fs::rename(&staging, path) {
            let _ = std::fs::remove_file(&staging);
            return Err(err);
        }
        Ok(())
    }

    /// How many times one entry has been launched.
    ///
    /// For a test that has to prove no launch was lost, which is the one
    /// question the count answers and the score does not: the score folds
    /// recency in, so two writers losing one launch each can still produce the
    /// same number.
    pub fn count(&self, id: &str) -> u32 {
        self.entries.get(id).map(|(count, _)| *count).unwrap_or(0)
    }

    /// How many distinct entries have ever been launched.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn record(&mut self, id: &str, now: i64) {
        let entry = self.entries.entry(id.to_string()).or_insert((0, now));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = now;

        self.forget_stale_entries(now, id);
    }

    /// Keeps the launch history bounded.
    ///
    /// `entries` was the one map here with no limit at all: every application,
    /// file, setting and window ever opened stayed for good, in a file written
    /// on every launch and parsed at every start. On a machine used for a year
    /// that is thousands of things somebody opened once and never again, most
    /// of which no longer exist.
    ///
    /// Two rules, in order. A single launch fades after three months, because
    /// once is not a habit. Then, if the map is still over its cap, the oldest
    /// go, whatever their count.
    ///
    /// `keep` is what was just recorded and is never dropped, for the reason
    /// written on `forget_oldest_queries`: times are whole seconds, so
    /// everything used in the same second ties, and a tie among `HashMap` keys
    /// is broken by hash order.
    fn forget_stale_entries(&mut self, now: i64, keep: &str) {
        // Cheap enough to skip entirely most of the time: below the cap, and
        // with nothing old enough to have faded, there is nothing to do. The
        // scan below is O(n) and runs on the registry lock with the next
        // keystroke waiting behind it.
        let faded = now.saturating_sub(ONE_OFF_FADES_AFTER);
        let anything_to_do = self.entries.len() > REMEMBERED
            || self
                .entries
                .iter()
                .any(|(_, (count, at))| *count <= 1 && *at < faded);

        if !anything_to_do {
            return;
        }

        self.entries
            .retain(|id, (count, at)| id == keep || *count > 1 || *at >= faded);

        if self.entries.len() <= REMEMBERED {
            return;
        }

        let mut ages: Vec<(String, i64)> = self
            .entries
            .iter()
            .filter(|(id, _)| id.as_str() != keep)
            .map(|(id, (_, at))| (id.clone(), *at))
            .collect();

        // Oldest first, and the id breaks a tie so the same file always loses.
        ages.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        let excess = self.entries.len() - REMEMBERED;
        for (id, _) in ages.into_iter().take(excess) {
            self.entries.remove(&id);
        }
    }

    /// Remembers that this query reached this command.
    ///
    /// Called with whatever was in the field when the user committed, which is
    /// what makes it an abbreviation rather than a name: the point is to learn
    /// the short thing they type, not the long thing they eventually matched.
    pub fn record_query(&mut self, query: &str, id: &str, now: i64) {
        let query = query.trim().to_lowercase();

        if query.is_empty() || query.chars().count() > LEARNED_MAX_LEN {
            return;
        }

        let seen = self.learned.entry(query.clone()).or_default();
        let entry = seen.entry(id.to_string()).or_insert((0, now));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = now;

        self.forget_oldest_queries(&query);
    }

    /// Keeps the map bounded, dropping the least recently used queries.
    ///
    /// `keep` is what was just recorded, and it is never dropped. Times are
    /// whole seconds, so everything used in the same second ties, and a tie
    /// among `HashMap` keys is broken by hash order. Without this the query
    /// the user just used can be the one forgotten by the very write that
    /// recorded it, **intermittently**: the same test passed and failed on
    /// consecutive runs of unchanged code, which is how this was found.
    fn forget_oldest_queries(&mut self, keep: &str) {
        if self.learned.len() <= LEARNED_QUERIES {
            return;
        }

        // The freshest time any command was chosen for that query.
        let mut ages: Vec<(String, i64)> = self
            .learned
            .iter()
            .filter(|(query, _)| query.as_str() != keep)
            .map(|(query, seen)| {
                let newest = seen.values().map(|(_, at)| *at).max().unwrap_or(0);
                (query.clone(), newest)
            })
            .collect();

        // By time, then by name. The name is not meaningful ordering, only
        // something stable: without it, which of a set of equally old queries
        // gets dropped changes between runs of identical code.
        ages.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

        for (query, _) in ages.into_iter().take(self.learned.len() - LEARNED_QUERIES) {
            self.learned.remove(&query);
        }
    }

    /// What this query has taught, if anything.
    ///
    /// Fetched once per search rather than once per candidate: the map is
    /// keyed by query, so the caller looks it up and then asks it about each
    /// id in turn.
    pub fn learned_for(&self, query: &str) -> Option<&HashMap<String, (u32, i64)>> {
        self.learned.get(query.trim().to_lowercase().as_str())
    }

    /// How many distinct queries have taught something. For diagnostics.
    pub fn learned_len(&self) -> usize {
        self.learned.len()
    }

    /// Puts a query at the front of the history.
    ///
    /// Deduplicated by moving rather than by refusing: searching for the same
    /// thing twice should not leave it buried under everything done since, and
    /// it should not appear twice either.
    pub fn remember(&mut self, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            return;
        }

        self.history.retain(|past| past != &query);
        self.history.insert(0, query);
        self.history.truncate(HISTORY);
    }

    /// What was typed before, most recent first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Higher is more likely to be what the user wants.
    ///
    /// Recency sets the magnitude and frequency multiplies within it, rather
    /// than the two being added. Addition lets a stale habit outrank something
    /// used minutes ago, which is exactly backwards for a launcher: what you
    /// did recently is the best predictor of what you are about to do.
    ///
    /// A command used constantly but not for a month lands near the bottom,
    /// while one used constantly *and* recently dominates, which is the
    /// behaviour that makes the root list usually right before you type.
    /// The ids under a prefix, the ones reached for most first.
    ///
    /// For corpora that are not the index: a folder somebody moves things to
    /// is worth offering again, and it has no command to be. The prefix keeps
    /// them out of the way of everything else stored here.
    ///
    /// Scored the same way commands are, so "most recently and most often"
    /// means the same thing here as everywhere else.
    pub fn recent_with_prefix(&self, prefix: &str, limit: usize) -> Vec<String> {
        let now = crate::state::now_seconds();

        let mut found: Vec<(&String, i64)> = self
            .entries
            .keys()
            .filter(|id| id.starts_with(prefix))
            .map(|id| (id, self.score(id, now)))
            .collect();

        found.sort_by(|(a_id, a), (b_id, b)| b.cmp(a).then_with(|| a_id.cmp(b_id)));

        found
            .into_iter()
            .take(limit)
            .map(|(id, _)| id[prefix.len()..].to_string())
            .collect()
    }

    pub fn score(&self, id: &str, now: i64) -> i64 {
        let Some(&(count, last)) = self.entries.get(id) else {
            return 0;
        };

        let age = (now - last).max(0);
        let recency = RECENCY_TIERS
            .iter()
            .find(|(window, _)| age <= *window)
            .map(|(_, points)| *points)
            .unwrap_or(5);

        // Capped so a command launched hundreds of times cannot permanently
        // outrank everything regardless of the query. The +10 keeps a
        // once-used command at its full recency value rather than a tenth.
        let frequency = 10 + count.min(20) as i64;

        recency * frequency / 10
    }
}

// ------------------------------------------------------------- aliases

/// One name the user chose for one thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alias {
    /// What they type. Stored lowercased, because that is how it is compared.
    pub alias: String,
    /// The command id it stands for.
    pub command: String,
}

/// The names the user has chosen, ready to be asked about.
///
/// Two lookups because both directions are needed and both are hot: ranking
/// asks "does this command have an alias" once per candidate, and the window
/// asks "what is the alias for this row" once per drawn row.
#[derive(Debug, Clone, Default)]
pub struct Aliases {
    by_command: std::collections::HashMap<String, String>,
}

impl Aliases {
    pub fn new(aliases: &[Alias]) -> Self {
        Self {
            by_command: aliases
                .iter()
                .filter(|a| !a.alias.trim().is_empty() && !a.command.is_empty())
                .map(|a| (a.command.clone(), a.alias.trim().to_lowercase()))
                .collect(),
        }
    }

    pub fn for_command(&self, id: &str) -> Option<&str> {
        self.by_command.get(id).map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.by_command.is_empty()
    }
}

// ------------------------------------------------------- match classes

/// How well a query matched, as a handful of discrete kinds.
///
/// This is what makes the list stable to type into. A fuzzy score changes by
/// a point or two on every keystroke, so ordering by it lets results trade
/// places constantly, and the position a person is reaching for moves out
/// from under them. Ordering by class means a result only moves when the
/// **kind** of match changes, which is a thing that happens rarely and for a
/// reason the user can feel.
///
/// Declaration order is preference order, best first: `Ord` is derived and
/// the sort relies on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchClass {
    /// The user said this is what those letters mean.
    ///
    /// Above everything, and it has to be: an alias is the one piece of
    /// ranking information that is not a guess. Somebody typed it in and said
    /// "when I type this, I mean that". A model that can overrule it has
    /// turned an instruction into a suggestion.
    Alias,
    /// This query has reached this command before, more than once.
    ///
    /// Below an alias, which was stated outright, and above everything the
    /// ranker infers from text. It is still evidence rather than instruction,
    /// but it is evidence about **this query** specifically, which no amount
    /// of reading the title can supply: nothing about "Obsidian" suggests
    /// somebody types "notes" to reach it.
    Learned,
    /// The title is exactly what was typed.
    ExactTitle,
    /// What was typed is a word of the title, or the start of one.
    ///
    /// **Where that word sits does not matter**, and that is the whole point
    /// of the class. "heart" in "heart suit" and "heart" in "red heart" are
    /// the same act: somebody typed a word this thing is called. Ranking the
    /// first above the second because it happens to come first in the string
    /// put ♥️ above ❤️, moon cake above the full moon, and a handbag above a
    /// raised hand. Ordering these by **title length** instead puts the plain
    /// thing first and the specific variants after it, which is what people
    /// mean.
    ///
    /// Contrast the substring below: "art" inside "heart" is a coincidence of
    /// spelling, not a name, and it stays down there.
    TitleWord,
    /// Every character landed on the start of a word: `vh` in View History.
    TitleWordStarts,
    /// The title contains what was typed, unbroken and mid-word.
    TitleSubstring,
    /// A keyword is exactly what was typed.
    ///
    /// Above a scattered subsequence, and that ordering was decided by a
    /// measurement rather than a preference. Searching emoji for `tada`
    /// returned the trade mark sign: `t`, `a`, `d`, `a` really are in "trade
    /// mark" in that order, so it matched as a subsequence, while the party
    /// popper only matched on its shortcode. A whole word somebody declared as
    /// another name for this thing is better evidence than four letters found
    /// scattered through a longer one.
    KeywordExact,
    /// The characters are all there in order, scattered.
    TitleSubsequence,
    /// Nothing in the title. Matched the extension's name or a keyword.
    Elsewhere,
    /// Nothing matched, but a word of the title is a near-miss for what was
    /// typed. Last on purpose: a guess, offered only when nothing else fits.
    TitleTypo,
}

/// Whether a character begins a word, by the same rule [`fuzzy`] scores.
/// Reads a query as the initials of the words in a name.
///
/// Only the positions where a word begins are considered, in order, so `vsc`
/// on "Visual Studio Code" matches V, S and C and never the s inside "Visual".
/// Returns nothing when the query cannot be read that way at all.
fn initials(needle: &[char], hay: &[char]) -> Option<Vec<usize>> {
    if needle.is_empty() {
        return None;
    }

    let mut matched = Vec::with_capacity(needle.len());
    let mut want = needle.iter().copied();
    let mut next = want.next()?;

    for at in 0..hay.len() {
        if !begins_a_word(hay, at) {
            continue;
        }

        if lower_one(hay[at]) == next {
            matched.push(at);
            match want.next() {
                Some(after) => next = after,
                // Every character placed, each at the start of a word.
                None => return Some(matched),
            }
        }
    }

    None
}

/// How many characters a scattered match may skip in one jump.
///
/// Skipping a letter is a near miss worth offering: `steam` reaches
/// StreamNook over a gap of one, and `disc` reaches Disk Cleanup over two.
/// Skipping fifty is not a match at all, it is a coincidence of a long name
/// containing common letters.
///
/// Three, measured. On real data the matches worth keeping had widest gaps of
/// 1, 2 and 2; the ones worth dropping had 7, 11 and 51. There is a wide empty
/// band between those, and this sits in it on the conservative side.
///
/// This is fzf's affine gap penalty as a limit rather than as a cost. A cost
/// would only reorder this class, and this class is already last.
const MAX_GAP: usize = 3;

/// The largest jump between two consecutive matched characters.
///
/// Not the span from first to last, which was tried and does not separate
/// anything: a short query over a short name and a long query over a long one
/// can span the same distance while being nothing alike. What tells them apart
/// is whether any single jump is implausible.
fn widest_gap(matched: &[usize]) -> usize {
    matched
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]).saturating_sub(1))
        .max()
        .unwrap_or(0)
}

fn begins_a_word(hay: &[char], at: usize) -> bool {
    at == 0
        || matches!(hay[at - 1], ' ' | '-' | '_' | '.' | '/' | ':')
        || (hay[at].is_uppercase() && hay[at - 1].is_lowercase())
}

/// Where `needle` appears unbroken in `hay`, in character positions.
/// The same characters with the marks that join one word taken out.
///
/// "Wi-Fi", "Node.js" and "don't" are each one word to the person typing them,
/// and the mark in the middle is the only reason a plain comparison fails.
/// Every kept character remembers where it came from, so a run found in the
/// joined form can still be highlighted in the text it was found in.
///
/// Spaces are **not** in this set. Removing them would make every pair of
/// words in a title one word, which is a much larger change to what counts as
/// a name and not one this is trying to make.
fn join_words(text: &[char]) -> (Vec<char>, Vec<usize>) {
    let mut kept = Vec::with_capacity(text.len());
    let mut from = Vec::with_capacity(text.len());

    for (at, c) in text.iter().enumerate() {
        if matches!(c, '-' | '.' | '\'' | '\u{2019}') {
            continue;
        }

        kept.push(*c);
        from.push(at);
    }

    (kept, from)
}

#[cfg(test)]
mod word_runs {
    use super::{match_class, CommandRecord, MatchClass};

    fn named(title: &str) -> CommandRecord {
        CommandRecord {
            id: title.to_lowercase(),
            extension: "test".into(),
            extension_title: "Test".into(),
            command: "run".into(),
            title: title.to_string(),
            subtitle: String::new(),
            description: String::new(),
            mode: "app".into(),
            entrypoint: String::new(),
            keywords: Vec::new(),
            icon: None,
            panel: None,
            preferences: serde_json::Value::Null,
            toggle: None,
        }
    }

    /// A name that holds the letters twice is judged by the better one.
    ///
    /// `CoreRenderer.cs` has `re` in the middle of "Core" and again where
    /// "Renderer" begins. Asking only about the first occurrence called this
    /// a mid-word substring, which is the weakest evidence the ranker has,
    /// while the name plainly starts a word with it.
    #[test]
    fn a_run_that_begins_a_word_later_is_still_a_word_match() {
        assert_eq!(
            match_class("re", &named("CoreRenderer.cs")),
            Some(MatchClass::TitleWord)
        );
    }

    /// And one that never begins a word is still only a substring.
    #[test]
    fn a_run_that_begins_no_word_is_still_a_substring() {
        assert_eq!(
            match_class("ignore", &named(".gitignore")),
            Some(MatchClass::TitleSubstring)
        );
    }

    /// The ordinary case has not moved.
    #[test]
    fn a_run_at_a_word_start_is_where_it_always_was() {
        assert_eq!(
            match_class("render", &named("CoreRenderer.cs")),
            Some(MatchClass::TitleWord)
        );
    }
}

/// The first place a run of these letters begins a word.
///
/// Separate from [`find_run`], which answers with the first occurrence
/// wherever it is. A name can hold the letters twice, once in the middle of a
/// word and once at the start of another, and only the second is evidence
/// that this is what the thing is called.
fn run_beginning_a_word(hay: &[char], lower: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > lower.len() {
        return None;
    }

    (0..=lower.len() - needle.len())
        .find(|&at| lower[at..at + needle.len()] == *needle && begins_a_word(hay, at))
}

fn find_run(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&at| hay[at..at + needle.len()] == *needle)
}

/// How this command matched, and which characters of its title to highlight.
///
/// `None` when it did not match at all, which is the answer for almost every
/// entry on almost every query.
fn classify(
    needle: &[char],
    command: &CommandRecord,
    alias: Option<&str>,
    learned: Option<&HashMap<String, (u32, i64)>>,
) -> Option<(MatchClass, Vec<usize>)> {
    // Checked before the title, because it outranks it. The alias is already
    // lowercased when it is stored, and the needle is lowercased by the
    // caller, so this is a comparison rather than a normalisation.
    if let Some(alias) = alias {
        if alias.chars().eq(needle.iter().copied()) {
            // No matched indices: the letters that matched are not in the
            // title, so highlighting positions in it would underline the
            // wrong characters.
            return Some((MatchClass::Alias, Vec::new()));
        }
    }

    // What this exact query has reached before. Checked after an alias, which
    // was stated, and before the title, which is inference.
    //
    // **This does not make a non-match into a match.** The command still has
    // to be something the query would have found anyway; learning promotes it
    // past the things that outranked it, rather than conjuring it out of a
    // corpus of fifteen hundred entries because the letters were typed once.
    // Without that rule a stray Enter puts an unrelated result at the top of a
    // query it has nothing to do with, which is unexplainable from the screen.
    let taught = learned
        .and_then(|seen| seen.get(&command.id))
        .is_some_and(|(count, _)| *count >= LEARNED_AT);

    // The text match still has to hold. Promotion is what learning does; it
    // does not create a match that was never there.
    let (class, matched) = classify_text(needle, command)?;

    Some(if taught {
        (MatchClass::Learned, matched)
    } else {
        (class, matched)
    })
}

/// One character, lowercased, for comparing against an already-lowered needle.
///
/// Keywords are written by hand in the index and by extension manifests, so
/// they are not reliably lowercase even though almost all of them are.
fn lower_one(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// How this command matched on its own text, with nothing learned or stated.
/// How a query matches one piece of text.
///
/// Split out of [`classify_text`] because the file index ranks bare names: a
/// file has a name and nothing else, no keywords and no extension it belongs
/// to. Two implementations of "does this text match" would have drifted the
/// first time either one learned something, and the point of a file behaving
/// like every other row is that the same code decides.
pub fn match_name(needle: &[char], text: &str) -> Option<(MatchClass, Vec<usize>)> {
    let hay: Vec<char> = text.chars().collect();
    let hay_lower: Vec<char> = text.to_lowercase().chars().collect();

    // Lowercasing can change length in some scripts, and these indices are
    // handed to the window to slice the *original* title.
    let aligned = hay_lower.len() == hay.len();

    if aligned {
        if hay_lower == needle {
            return Some((MatchClass::ExactTitle, (0..needle.len()).collect()));
        }
        /*
         * Whether the run starts the title is not asked. Only whether it
         * starts a *word*, which is what makes it a name rather than an
         * accident of spelling. Position zero always begins a word, so this
         * answers both cases at once.
         *
         * Every occurrence, not just the first. `find_run` answers with the
         * first, and a name can contain the letters twice: `CoreRenderer.cs`
         * has `re` at index two, in the middle of "Core", and again at index
         * four where "Renderer" begins. Asking only about the first said this
         * was a mid-word substring, which is the weakest evidence the ranker
         * has, when the same name plainly starts a word with it.
         */
        if let Some(at) = run_beginning_a_word(&hay, &hay_lower, needle) {
            return Some((MatchClass::TitleWord, (at..at + needle.len()).collect()));
        }

        /*
         * The same again, with the marks that split one word taken out.
         *
         * "Wi-Fi" is the word people type as "wifi" and the hyphen is the only
         * thing between them, so `Turn Wi-Fi Off` matched only as a scattered
         * subsequence and lost to every settings page with the word in it.
         * Both sides are joined, so it works in either direction: typing
         * "e-mail" finds "Email" too.
         *
         * Second rather than first, so a title that matches outright is never
         * beaten by one that needed the marks removed.
         */
        let (joined_hay, from) = join_words(&hay_lower);
        let (joined_needle, _) = join_words(needle);

        if joined_hay.len() != hay_lower.len() || joined_needle.len() != needle.len() {
            if joined_hay == joined_needle {
                return Some((MatchClass::ExactTitle, from));
            }

            if let Some(at) = find_run(&joined_hay, &joined_needle) {
                // Asked of the original, because that is where the words are.
                if begins_a_word(&hay, from[at]) {
                    // The indices the run covers in the title it was found in,
                    // which skip the mark rather than running through it.
                    return Some((
                        MatchClass::TitleWord,
                        from[at..at + joined_needle.len()].to_vec(),
                    ));
                }
            }
        }
    }

    // Asked directly rather than hoped for. `fuzzy_with` takes the first
    // occurrence of each character, so on "Visual Studio Code" it matches the
    // s inside "Visual" and then has to reach eleven characters for the c.
    // That reads as a scattered near miss when it is really an acronym, and
    // the gap limit below then throws it away.
    //
    // fzf avoids this with a dynamic program that finds the best alignment
    // rather than the first one. The whole of the difference here is whether
    // initials are read as initials, so this asks that one question instead.
    if let Some(matched) = initials(needle, &hay) {
        return Some((MatchClass::TitleWordStarts, matched));
    }

    let scattered = fuzzy_with(needle, &text);

    // Checked before the substring case on purpose. Typing initials is a
    // deliberate act and `sc` should find Screen Capture ahead of Discord,
    // which merely contains those two letters in a row.
    if let Some((_, matched)) = &scattered {
        if !matched.is_empty() && matched.iter().all(|&at| begins_a_word(&hay, at)) {
            return Some((MatchClass::TitleWordStarts, matched.clone()));
        }
    }

    if aligned {
        if let Some(at) = find_run(&hay_lower, needle) {
            return Some((
                MatchClass::TitleSubstring,
                (at..at + needle.len()).collect(),
            ));
        }
    }

    // The first typed character has to land where a word begins.
    //
    // fzf doubles the bonus on the first character for exactly this reason:
    // "the first character in the typed pattern usually has more significance
    // than the rest, so it's important that it appears at special positions
    // where bonus points are given". Here it is a requirement rather than a
    // bonus, because this class is already last and a bonus inside it would
    // only reorder noise.
    //
    // What it costs and what it buys, measured against a real index of 1,444
    // entries: `tada` drops from fifty-seven results to a handful, `term`
    // stops offering Character Map, and `steam` still finds StreamNook, which
    // is the one scattered match on that machine anybody would have wanted.
    if let Some((_, matched)) = scattered {
        if matched.first().is_some_and(|&at| begins_a_word(&hay, at))
            && widest_gap(&matched) <= MAX_GAP
        {
            return Some((MatchClass::TitleSubsequence, matched));
        }
    }

    None
}

/// Whether every word of a phrase lands somewhere on this command.
///
/// A match otherwise has to happen inside one field, and `audio output`
/// reached nothing at all: both words are keywords of the audio switches, and
/// no single field holds the phrase. The row was not ranked badly, it was not
/// there.
///
/// Strict on purpose. Each word has to be a whole word of the title or a
/// keyword of its own, never a subsequence, because a phrase matched loosely
/// would find something for almost anything typed. Two words minimum: a single
/// word has already been asked about everywhere this could ask.
fn every_word_lands(needle: &[char], command: &CommandRecord) -> bool {
    let typed: String = needle.iter().collect();
    let words: Vec<&str> = typed.split_whitespace().collect();

    if words.len() < 2 {
        return false;
    }

    words.iter().all(|word| {
        let word: Vec<char> = word.chars().collect();

        let in_title = match_name(&word, &command.title).is_some_and(|(class, _)| {
            matches!(class, MatchClass::ExactTitle | MatchClass::TitleWord)
        });

        in_title
            || command
                .keywords
                .iter()
                .any(|keyword| keyword.chars().map(lower_one).eq(word.iter().copied()))
    })
}

/// The longest a field can be and still be read as a name.
///
/// Beyond this it is prose, and the two clever rules below stop meaning
/// anything: the initials of a paragraph spell almost any short word, and a
/// scattered match has hundreds of places to find each letter. A snippet body
/// is prose. A keyword is a name; the longest Sill ships is under thirty
/// characters.
const NAME_LIMIT: usize = 48;

/// Whether the query matches a field that is not the title.
///
/// `fuzzy_with` on its own is an unbounded subsequence: over a long enough
/// string, almost any query matches. That is tolerable on a title, which is
/// short and which the caller already gates by word start and widest gap. It
/// was not tolerable here, because **a snippet's entire body is one of its
/// keywords**. A three hundred character snippet contains the letters of very
/// nearly anything in order, so every snippet matched every query and sat in
/// the results under whatever was actually being looked for.
///
/// Two rules for prose, three for a name.
///
/// The letters together always count, wherever they sit: that is what "that
/// phrase is in there" means, it is the only useful way to search a body of
/// text, and `mail` finding the keyword "email" is a contract this ranker
/// already had. On something short enough to be a name, the initials
/// of its words count too, so `vm` still finds the keyword "volume mixer", and
/// so does a scattered match held to the discipline a title gets: starting at
/// a word, never jumping more than [`MAX_GAP`].
///
/// Both of the extra rules are refused on prose, and each was refused for a
/// reason found by a test rather than guessed at. The initials of an ordinary
/// two-sentence snippet spell `figma`. A gap of three has hundreds of chances
/// to be met over that many characters.
fn matches_another_field(needle: &[char], text: &str) -> bool {
    if needle.is_empty() {
        return false;
    }

    let hay: Vec<char> = text.chars().collect();
    // `lower_one` is one character in and one out, so indices stay usable.
    let lower: Vec<char> = hay.iter().copied().map(lower_one).collect();

    // A contiguous run, anywhere, including inside a word: `mail` finds the
    // keyword "email", which is a contract this ranker already had a test for.
    // Position matters for a scattered match, where it is the only thing
    // separating intent from coincidence, and not for letters found together.
    if find_run(&lower, needle).is_some() {
        return true;
    }

    if hay.len() > NAME_LIMIT {
        return false;
    }

    if initials(needle, &hay).is_some() {
        return true;
    }

    let Some((_, matched)) = fuzzy_with(needle, text) else {
        return false;
    };

    matched.first().is_some_and(|&at| begins_a_word(&hay, at)) && widest_gap(&matched) <= MAX_GAP
}

fn classify_text(needle: &[char], command: &CommandRecord) -> Option<(MatchClass, Vec<usize>)> {
    if let Some(found) = match_name(needle, &command.title) {
        return Some(found);
    }

    // Nothing in the title. The other sources are searched but never
    // highlighted, since their indices would point into the wrong string.
    // No highlight for either of these: what matched is not in the title, so
    // marking positions in it would underline the wrong characters.
    if command
        .keywords
        .iter()
        .any(|keyword| keyword.chars().map(lower_one).eq(needle.iter().copied()))
    {
        return Some((MatchClass::KeywordExact, Vec::new()));
    }

    if matches_another_field(needle, &command.extension_title)
        || command
            .keywords
            .iter()
            .any(|keyword| matches_another_field(needle, keyword))
    {
        return Some((MatchClass::Elsewhere, Vec::new()));
    }

    // A phrase whose words are spread across the fields rather than sitting
    // together in one of them.
    if every_word_lands(needle, command) {
        return Some((MatchClass::Elsewhere, Vec::new()));
    }

    // Last resort. Nothing matched, so ask whether this was a slip of the
    // fingers. No highlight comes back: the characters that were typed are
    // not the characters in the title, and pointing at them would be a lie.
    let budget = typo_budget(needle.len());
    looks_like_a_typo_of(needle, &command.title, budget)
        .then(|| (MatchClass::TitleTypo, Vec::new()))
}

/// How many single-character mistakes are forgiven at this query length.
///
/// Nothing under four characters, because at three a budget of one matches an
/// enormous share of any index and the list fills with things the user did not
/// ask for. The allowance grows with length because a longer word gives the
/// distance more to work with before a match becomes a coincidence.
fn typo_budget(length: usize) -> usize {
    match length {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

/// Edit distance counting an adjacent swap as one mistake, not two.
///
/// Optimal string alignment rather than plain Levenshtein, and the difference
/// is the whole point: **transposition is the typo people actually make.**
/// `chorme` for `chrome` is one slip of the fingers and Levenshtein calls it
/// two edits, which puts it outside any budget tight enough to be useful.
///
/// Returns `None` as soon as the distance cannot come in under `budget`, so
/// the common case (two unrelated words) stops after a row or two.
fn near_miss(a: &[char], b: &[char], budget: usize) -> Option<usize> {
    // A length difference is already that many edits.
    if a.len().abs_diff(b.len()) > budget {
        return None;
    }

    // Three rows, rotated rather than reallocated. All three start at full
    // width: the rotation below hands the oldest row back as scratch, and an
    // empty one arrives at `current[0] = i` as an out-of-bounds write.
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut current: Vec<usize> = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        current[0] = i;
        let mut best = current[0];

        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (current[j - 1] + 1)
                .min(prev[j] + 1)
                .min(prev[j - 1] + cost);

            // The transposition case: the two characters are swapped.
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(prev2[j - 2] + 1);
            }

            current[j] = value;
            best = best.min(value);
        }

        // Every alignment through this row already costs more than allowed.
        if best > budget {
            return None;
        }

        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut current);
    }

    (prev[b.len()] <= budget).then_some(prev[b.len()])
}

/// Whether some word of the title is a near-miss for what was typed.
///
/// Word by word rather than against the whole title, because a title is
/// usually several words and the typo is in one of them: `chorme` should find
/// Google Chrome, and comparing it against the whole name never will.
fn looks_like_a_typo_of(needle: &[char], title: &str, budget: usize) -> bool {
    if budget == 0 {
        return false;
    }

    /*
     * One buffer, filled a word at a time.
     *
     * This is the last thing asked of a candidate that matched nothing, which
     * means it is asked of nearly every entry in the index on nearly every
     * keystroke. `to_lowercase()` allocated a String for each of them, and
     * collecting each word allocated again. Neither survived the call.
     *
     * The rest of `fuzzy_with`'s doc comment makes the same point about the
     * same kind of waste, and this was the one place still doing it.
     */
    let mut word: Vec<char> = Vec::with_capacity(24);

    for c in title.chars() {
        if c.is_alphanumeric() {
            word.push(lower_one(c));
            continue;
        }

        if !word.is_empty() {
            if near_miss(needle, &word, budget).is_some() {
                return true;
            }
            word.clear();
        }
    }

    !word.is_empty() && near_miss(needle, &word, budget).is_some()
}

/// How a query matched this command, if it did.
///
/// Public because the class is a fact about the result worth having outside
/// ranking: the tests assert stability against it, and grouping the list by
/// it is the obvious next use.
pub fn match_class(query: &str, command: &CommandRecord) -> Option<MatchClass> {
    let needle: Vec<char> = query.trim().to_lowercase().chars().collect();
    classify(&needle, command, None, None).map(|(class, _)| class)
}

/// Whether a match is good enough to volunteer beside things that were asked
/// for.
///
/// A separate corpus appended to the root list has to earn its place. Emoji
/// matched loosely would put a smiley in the middle of every search: there are
/// nearly two thousand of them and their names are ordinary words, so a
/// scattered subsequence finds a dozen for almost anything typed.
///
/// So only the classes that mean the user named the thing: the whole title,
/// the start of it, a keyword exactly, or something they set or taught
/// themselves. Not a substring, not a subsequence, not a near miss.
pub fn is_strong(class: MatchClass) -> bool {
    matches!(
        class,
        MatchClass::Alias
            | MatchClass::Learned
            | MatchClass::ExactTitle
            | MatchClass::TitleWord
            | MatchClass::TitleWordStarts
            | MatchClass::KeywordExact
    )
}

/// The same, for a command the user has given a name of their own.
pub fn match_class_with_alias(
    query: &str,
    command: &CommandRecord,
    alias: &str,
) -> Option<MatchClass> {
    let needle: Vec<char> = query.trim().to_lowercase().chars().collect();
    classify(&needle, command, Some(alias), None).map(|(class, _)| class)
}

// ----------------------------------------------------------------- search

/// How many results a search may return.
///
/// History worth keeping, because both mistakes are easy to repeat. It was
/// first 50, which silently capped the root list at 50 of about 1,400 indexed
/// entries: every source added past that point was invisible, and the symptom
/// looked exactly like the indexing not working at all. It was then raised to
/// 2,000, above the corpus, which is not a limit at all and meant **every
/// search serialised the entire index across the IPC boundary**, measured at
/// 534 KB for an empty query against a list that draws about fifteen rows.
///
/// 120 is the number that is a limit without being a wall. Ranking still
/// considers every entry; this is only how many survive to be sent. Anything
/// past the first hundred was never going to be found by scrolling, only by
/// typing more, and typing more re-runs the search over the whole corpus.
pub const SEARCH_LIMIT: usize = 120;

/// What a Windows switch is worth before anybody has ever pressed it.
///
/// Set just above a page opened **once earlier today**, which scores 77, and
/// below one opened several times in the last hour. So a switch beats a page
/// somebody happened to visit, and loses to one they actually rely on.
const SWITCH_FLOOR: i64 = 80;

/// Where the conversation you left ranks.
///
/// Above everything, including the empty query, which is the one place the
/// switch floor above deliberately does not apply. The reasoning differs
/// because the rows differ: there are twelve switches and they would bury a
/// list ordered by what you reach for, whereas there is only ever one of
/// these and it expires by itself within ten minutes.
///
/// The number is arithmetic rather than taste. Frecency tops out at 300: the
/// best recency tier is 100 and the capped frequency multiplier is 30, over
/// ten. So anything above 300 is first, and 400 says so without being
/// `i64::MAX`, which would also outrank a calculator answer.
const CONVERSATION_FLOOR: i64 = 400;

/// Ranks commands for a query.
///
/// An empty query is the root list, ordered purely by frecency, which is what
/// a user sees every time they summon without typing.
pub fn search(
    commands: &[CommandRecord],
    query: &str,
    frecency: &Frecency,
    now: i64,
    limit: usize,
) -> Vec<RankedCommand> {
    search_excluding(
        commands,
        query,
        frecency,
        &Aliases::default(),
        now,
        limit,
        Excluded::none(),
    )
}

/// The corpus is borrowed, never collected.
///
/// This runs on **every keystroke** against an index of well over a thousand
/// entries, so taking an iterator rather than a slice is what lets a caller
/// chain two sources together without deep-copying either. An earlier version
/// cloned the whole index per character typed, which is thousands of string
/// allocations for one keypress.

/// The same search, with the user's exclusion terms applied.
///
/// Mirrors `files::search` and `files::search_with`: the plain name is the
/// common case, the long one carries the settings.
pub fn search_excluding<'a>(
    commands: impl IntoIterator<Item = &'a CommandRecord>,
    query: &str,
    frecency: &Frecency,
    aliases: &Aliases,
    now: i64,
    limit: usize,
    off: Excluded<'_>,
) -> Vec<RankedCommand> {
    let query = query.trim();

    // Lowercased and collected once for the whole search rather than once per
    // candidate. See `fuzzy_with` for what that was costing.
    let needle: Vec<char> = query.to_lowercase().chars().collect();

    // Looked up once for the whole search. The map is keyed by query, so
    // fetching it per candidate would be a hash of the same string fifteen
    // hundred times per keystroke.
    let learned = frecency.learned_for(query);

    // Scored by reference, cloned afterwards.
    //
    // The previous version cloned a whole `CommandRecord`, keywords vector and
    // all, for every candidate that matched, and then threw all but the first
    // `limit` of them away. On an empty query that is fourteen hundred deep
    // clones to produce a hundred results. Borrowing until the truncation is
    // done means only the survivors are ever copied.
    //
    // (class, weight, command, matched). See the sort below for why the
    // ordering is these four things and not a single score.
    let mut scored: Vec<(MatchClass, i64, &CommandRecord, Vec<usize>)> = Vec::new();

    // Filtered here rather than at scan time so removing a term brings its
    // entries straight back, with no reindex.
    let excluded: Vec<String> = off
        .terms
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    for command in commands {
        if !excluded.is_empty() && is_excluded(command, &excluded) {
            continue;
        }

        if !off.ids.is_empty() && is_hidden(command, off.ids) {
            continue;
        }

        // An empty query is the root list, where everything matched equally
        // and the order is purely what you reach for most.
        let (class, matched) = if query.is_empty() {
            (MatchClass::ExactTitle, Vec::new())
        } else {
            match classify(&needle, command, aliases.for_command(&command.id), learned) {
                Some(found) => found,
                None => continue,
            }
        };

        let mut weight = frecency.score(&command.id, now);

        // A bare PATH executable is usually not what someone means. There are
        // roughly a thousand of them against a couple of hundred real
        // applications, so without this a CLI tool wins on any short query.
        // A penalty rather than exclusion: they are still reachable by name.
        if command.mode == "exe" {
            weight -= 12;
        }

        /*
         * A switch you have never pressed still ranks as a familiar one.
         *
         * `bluetooth` put three settings pages above "Turn Bluetooth Off". One
         * of them opens a window where the thing can be done and the other
         * does it, and somebody who typed the name of a switch asked for the
         * switch.
         *
         * A floor rather than a bonus, and the difference is the whole reason
         * this works. A bonus of a dozen points was swamped: a settings page
         * opened **once, earlier today** scores 77, because recency dominates
         * the frecency curve. The floor says a switch is never ranked as
         * though it were unknown, which is the honest claim: these are twelve
         * things Sill ships, not one of fifteen hundred scanned entries it
         * knows nothing about.
         *
         * `max`, so a switch that really is used keeps its own larger score,
         * and a page somebody opens *repeatedly* still wins. That is a
         * preference worth honouring; one visit is not.
         *
         * Not applied to the empty query, where every row matches and this
         * would push twelve switches to the top of a list that is supposed to
         * be ordered by what you reach for.
         */
        if !query.is_empty() && command.mode == "system" {
            weight = weight.max(SWITCH_FLOOR);
        }

        // Where you were, above what exists. On the empty query as well, for
        // the reason written on the constant.
        if command.mode == "conversation" {
            weight = weight.max(CONVERSATION_FLOOR);
        }

        scored.push((class, weight, command, matched));
    }

    /*
     * Four keys, and every one of them is stable while you type.
     *
     * Class first, so a result only changes position when the *kind* of match
     * changes. Then how much you reach for it. Then the shorter title, which
     * is the honest reading of two equally good matches: the query covers more
     * of it. Then the name, so ties are alphabetical rather than arbitrary.
     *
     * What is deliberately absent is the fuzzy score itself. It moves by a
     * point or two on every keystroke, so ordering by it lets results trade
     * places constantly and the row someone is reaching for slides out from
     * under their finger. It still decides the class; it just no longer
     * decides the position within one.
     *
     * ## Except with nothing typed
     *
     * The shorter title is only meaningful *against a query*: it says the
     * query covers more of that title. With nothing typed there is no query to
     * cover anything, so the rule stops being a reading of the match and turns
     * into "short names first", which is an order nobody chose and nobody can
     * predict. The opening list read Ai, Cmd, Edge, Gmail down to the longest
     * name on the machine.
     *
     * So with an empty query it is frecency, then alphabetical: what you reach
     * for, and then a list somebody can actually find their way down.
     */
    let typed = !query.is_empty();

    scored.sort_by(|(a_class, a_weight, a, _), (b_class, b_weight, b, _)| {
        a_class
            .cmp(b_class)
            .then_with(|| b_weight.cmp(a_weight))
            .then_with(|| {
                if typed {
                    a.title.chars().count().cmp(&b.title.chars().count())
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| a.title.cmp(&b.title))
    });
    scored.truncate(limit);

    scored
        .into_iter()
        .map(|(class, weight, command, matched)| RankedCommand {
            class,
            command: command.clone(),
            score: weight,
            matched,
        })
        .collect()
}

/// Whether an entry matches any exclusion term, by title or by path.
///
/// Path as well as title, because the useful exclusions are usually a folder:
/// hiding one vendor's whole Start Menu directory is a single term, where
/// hiding it by name is a dozen.
fn is_excluded(command: &CommandRecord, excluded: &[String]) -> bool {
    let title = command.title.to_lowercase();
    let target = command.entrypoint.to_lowercase();
    excluded
        .iter()
        .any(|term| title.contains(term.as_str()) || target.contains(term.as_str()))
}

/// What the user has switched off.
///
/// A type rather than two more arguments: the two are always passed together
/// and always mean the same pair, and the signature was already at the point
/// where the next `&[String]` would be indistinguishable from the last.
#[derive(Debug, Clone, Copy, Default)]
pub struct Excluded<'a> {
    /// Words matched against every title and path.
    pub terms: &'a [String],
    /// Individual entries switched off by id.
    pub ids: &'a [String],
}

impl Excluded<'_> {
    pub const fn none() -> Self {
        Self {
            terms: &[],
            ids: &[],
        }
    }
}

/// Whether this entry was switched off individually.
///
/// Checked by id and by exact match, unlike the term list, because "not this
/// one" has to mean this one and nothing that happens to share a word with it.
pub fn is_hidden(command: &CommandRecord, hidden: &[String]) -> bool {
    hidden.iter().any(|id| id == &command.id)
}

/// Where the frecency file lives, given the app's data directory.
pub fn frecency_path(data_dir: &Path) -> PathBuf {
    data_dir.join("frecency.json")
}
