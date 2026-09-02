//! Sill's own settings.
//!
//! Named `preferences` rather than `settings` because `settings_catalog`
//! already means Windows settings pages, and two things called settings in one
//! codebase is a reliable source of confusion.
//!
//! Every struct here carries `#[serde(default)]` on **both** the struct and its
//! fields. A nested struct without it fails to deserialise the moment a new
//! field is added, which silently resets a user's whole configuration to
//! defaults on upgrade. The catch-all on the outer type does not save the
//! inner ones.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Reads a list, keeping every entry that can be read.
///
/// The rule in the module note above holds for a struct that is a *section* of
/// this file, because the catch-all default fills in whatever is missing. It
/// does not hold for a struct that is an *element of a list*: `Binding` and
/// `Alias` have required fields, so one entry serde cannot read fails the
/// whole list, which fails the whole file, which resets every setting.
///
/// A list is the one place where dropping one thing is obviously better than
/// dropping everything. Defaulting the fields instead would be worse: a
/// binding with no accelerator and no action is not a binding, and keeping it
/// would put an empty row in the Shortcuts panel that does nothing.
///
/// Says what it dropped, because a shortcut quietly disappearing after an
/// update is exactly the kind of thing nobody can report usefully.
fn entries_that_can_be_read<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;

    Ok(raw
        .into_iter()
        .filter_map(|value| match serde_json::from_value::<T>(value) {
            Ok(one) => Some(one),
            Err(why) => {
                crate::say!(
                    "dropped one {} that could not be read: {why}",
                    std::any::type_name::<T>()
                );
                None
            }
        })
        .collect())
}

/// System integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    /// Start Sill when the user signs in.
    pub open_at_login: bool,
    /// Keep an icon in the notification area.
    pub show_in_tray: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            // Off by default: an application that adds itself to startup
            // without being asked is a bad neighbour.
            open_at_login: false,
            // On by default, because it is the only visible sign that a
            // launcher with no taskbar entry is running at all.
            show_in_tray: true,
        }
    }
}

/// How the launcher is summoned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Hotkey {
    /// An accelerator like "Alt+Space".
    pub summon: String,
    /// Opens the launcher straight into the window switcher.
    ///
    /// Its own key rather than a mode you type into: the value of a switcher
    /// is that one press puts the window you were last in under the cursor,
    /// and a prefix you have to type first spends that. Empty turns it off.
    #[serde(default = "default_switcher")]
    pub switcher: String,
    /// Picks an area of the screen without opening the launcher first.
    ///
    /// Its own key for the same reason the switcher has one: the whole value
    /// of a screenshot key is that it is one press. Empty turns it off, and it
    /// is empty by default because there is no obvious free combination and a
    /// default that collides is worse than none.
    #[serde(default)]
    pub capture: String,
    /// Copies every screen at once, likewise.
    #[serde(default)]
    pub capture_screen: String,
    /// Dismiss when the window loses focus.
    pub dismiss_on_blur: bool,
    /// Select the existing query on summon so typing replaces it.
    pub select_query_on_summon: bool,
    /// Return to the root list every time the launcher is summoned.
    pub reset_on_summon: bool,
}

/// W for window, and free where it was checked.
///
/// The obvious choices are not available. Alt+Tab and Ctrl+Alt+Tab belong to
/// Windows and cannot be registered at all, and Ctrl+Alt+Space was refused on
/// the first machine this was tried on: something already owned it, and
/// Windows does not say what.
///
/// So this is a default rather than a guarantee, which is the reason a refused
/// key is now reported in settings instead of failing into the log.
fn default_switcher() -> String {
    "Ctrl+Alt+W".to_string()
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            // Matches Raycast on Windows, which is the muscle memory being
            // replaced.
            summon: "Alt+Space".to_string(),
            switcher: default_switcher(),
            // Empty: there is no obviously free combination for these, and a
            // default that collides is worse than asking somebody to choose.
            capture: String::new(),
            capture_screen: String::new(),
            dismiss_on_blur: true,
            select_query_on_summon: true,
            reset_on_summon: false,
        }
    }
}

/// Which palette the interface is drawn in.
///
/// A theme changes the canvas and the accent, and nothing else. The neutral
/// ramp that owns text, hairlines and fills is white-alpha in every theme, so
/// contrast, legibility and the whole layering system are identical whichever
/// one is picked. That is what stops a theme becoming a second design system.
///
/// Rust only holds the choice. The palettes themselves are `[data-theme]`
/// blocks in `theme.css`, because a colour is presentation and a preference is
/// state, exactly as `InterfaceFont` already splits.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    /// Neutral near-black with a desaturated blue-grey accent. The default,
    /// and the palette Sill shares with StreamNook.
    WintersGlass,
    /// The same restraint with a faint iridescent wash across the window.
    /// The only theme that paints anything beyond a flat tint.
    Oilslick,
    /// No hue anywhere, accent included. The most restrained of the set.
    Graphite,
    /// Warm black and an amber accent.
    Ember,
    /// Cool green, slightly warmer canvas than the default.
    Moss,
}

/// Which face the interface is set in.
///
/// The window is transparent so the desktop can show through it, and that
/// costs subpixel text rendering for every glyph: Chromium falls back to flat
/// greyscale coverage, because blending against pixels it cannot see has no
/// correct answer. So the question is not which face is best hinted, it is
/// which one holds up once the display's subpixels are no longer available.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InterfaceFont {
    /// Satoshi, bundled. The default.
    ///
    /// Even stems and enough weight at 13px that it does not go anaemic
    /// under greyscale coverage, which is the condition it is actually
    /// rendered in here.
    Satoshi,
    /// Inter, bundled. Drawn for displays where hinting no longer decides
    /// anything, which is not this one.
    Inter,
    /// Segoe UI Variable, which Windows ships and hints for its own
    /// rendering at 96 DPI. The one with real optical sizes.
    System,
}

/// Which desktop backdrop the window uses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Backdrop {
    /// Windows acrylic. Adds a luminosity layer of its own, so it always
    /// lightens somewhat however dark the tint.
    Acrylic,
    /// The older composition blur. No luminosity layer, so the tint given is
    /// the tint that shows.
    Blur,
    /// No OS material at all; the page paints its own surface. Deepest.
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    pub backdrop: Backdrop,
    /// Which palette everything is drawn in.
    pub theme: Theme,
    /// How strongly a theme's chroma wash is painted, 0 to 2.
    ///
    /// Only Oilslick has one. A multiplier rather than a set of alphas so the
    /// relationship between the three washes is fixed and only their weight
    /// moves: tuning them independently is how an iridescent sheen turns into
    /// three coloured blobs.
    pub chroma_strength: f32,
    /// Which face everything is set in.
    pub font: InterfaceFont,
    /// 0 is fully solid, 1 is pure tint over the desktop blur.
    pub glass_strength: f32,
    /// How dark the backdrop tint sits, 0 to 255.
    pub tint_alpha: u8,
    /// Rows shown before the list scrolls; sets the window height.
    pub visible_rows: u32,
    /// Launcher width in pixels.
    pub window_width: u32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            backdrop: Backdrop::Acrylic,
            theme: Theme::WintersGlass,
            chroma_strength: 1.0,
            font: InterfaceFont::Satoshi,
            glass_strength: 1.0,
            tint_alpha: 232,
            visible_rows: 10,
            window_width: 750,
        }
    }
}

/// Snippets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snippets {
    /// Watch typing and expand a keyword wherever it is typed.
    ///
    /// Off by default. A launcher that installs a keyboard hook and starts
    /// rewriting what you type without being asked is not a launcher anyone
    /// asked for; snippets stay reachable from the launcher either way.
    pub expand_keywords: bool,
}

/// Double-tapping a modifier to reach the launcher.
///
/// The gesture every launcher on every platform eventually grows, because it
/// needs no chord and no key that anything else wants: the modifier keeps
/// doing its own job, and doing it twice quickly is a thing nothing else
/// listens for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Taps {
    /// Which modifier, or none for the gesture being off.
    ///
    /// Off by default, and for the same reason snippet expansion is: a
    /// launcher that installs a keyboard hook without being asked is not one
    /// anybody asked for.
    pub modifier: Option<crate::taps::Modifier>,
    /// How long the second tap has to arrive.
    pub window_ms: u64,
}

impl Default for Taps {
    fn default() -> Self {
        Self {
            modifier: None,
            window_ms: crate::taps::WINDOW_MS,
        }
    }
}

/// Who answers when you ask something.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Ai {
    /// The id of the provider that answers. Empty means none is set up, and
    /// asking says so rather than failing at the request.
    pub provider: String,
    /// The ones configured. Each key is sealed before this file is written.
    pub providers: Vec<crate::ai::provider::Provider>,
}

impl Default for Snippets {
    fn default() -> Self {
        Self {
            expand_keywords: false,
        }
    }
}

/// The widgets, and which of them ride along in the launcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Widgets {
    /// Ids kept in the launcher's chin, in the order they are shown.
    ///
    /// A list rather than a flag per widget, because the order is a choice
    /// too and a set of booleans cannot hold one.
    pub pinned: Vec<String>,
    /// Where the weather is for. Empty until somebody says.
    pub place: crate::weather::Place,
    /// Fahrenheit, or Celsius.
    pub fahrenheit: bool,
    /// A clock that counts seconds, or one that does not.
    ///
    /// Off by default, and that is an efficiency decision rather than a taste
    /// one: seconds mean a redraw every second for as long as the launcher is
    /// open, and a clock nobody is watching should cost a redraw a minute.
    pub seconds: bool,
}

impl Default for Widgets {
    fn default() -> Self {
        Self {
            // Nothing is pinned until it is asked for. A launcher that arrives
            // with a strip of other people's choices along the bottom has
            // decided something on somebody's behalf.
            pinned: Vec::new(),
            place: crate::weather::Place::default(),
            fahrenheit: true,
            seconds: false,
        }
    }
}

/// Clipboard history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClipboardHistory {
    /// Watch the clipboard at all.
    pub enabled: bool,
    /// Days an unpinned entry is kept. Zero keeps everything.
    ///
    /// A clipboard accumulates one-time codes and whatever was typed near a
    /// password field, so a history with no end date is a liability rather
    /// than a feature.
    pub retain_days: u32,
    /// Keep images as well as text. They are much the largest thing a
    /// clipboard carries, so this is worth being able to decline.
    pub keep_images: bool,
    /// Applications whose copies are never recorded.
    ///
    /// Matched against the source application's name. Password managers
    /// already exclude themselves through the Windows clipboard formats, so
    /// this is for everything else worth keeping out: a terminal, a banking
    /// site's browser profile, a note where the secrets live.
    pub ignored_apps: Vec<String>,
    /// What to do with something that looks like a credential.
    ///
    /// Defaults to not storing it. The clipboard database is a plain file that
    /// any process running as the user can read, and that whatever backs up
    /// `%APPDATA%` will copy, so a token that lands in it has already leaked.
    #[serde(default)]
    pub secrets: crate::clipboard::sensitive::Policy,
}

impl Default for ClipboardHistory {
    fn default() -> Self {
        Self {
            enabled: true,
            retain_days: 30,
            keep_images: true,
            ignored_apps: Vec::new(),
            secrets: crate::clipboard::sensitive::Policy::default(),
        }
    }
}

/// Which places are searched for launchable things.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sources {
    /// Start Menu, Desktop and taskbar pins.
    pub shortcuts: bool,
    /// Packaged apps, via the Apps folder.
    pub packaged_apps: bool,
    /// Executables registered under App Paths.
    pub app_paths: bool,
    /// Installed programs from the Uninstall hives.
    pub installed_programs: bool,
    /// Every executable on %PATH%. About 900 entries, mostly CLI tools.
    ///
    /// Off by default, and it is the only source that is. Measured on a real
    /// machine it was 912 of 1,443 indexed entries, so **63% of the index for
    /// the handful of them anyone launches by name**, and every one of those
    /// entries is ranked and weighed on every keystroke. The ranker already
    /// penalises them precisely because they were drowning real applications.
    ///
    /// Anyone who wants `ffmpeg` in the launcher turns it back on and gets it.
    /// The reverse, a first run where a search for "co" offers forty compiler
    /// front ends, is the impression that does not get a second chance.
    pub path_executables: bool,
    /// Windows settings pages and Control Panel applets.
    pub windows_settings: bool,
    /// Entries whose name or path contains any of these are never shown.
    pub excluded: Vec<String>,
    /// Individual entries switched off by id.
    ///
    /// Separate from `excluded`, which is a list of words matched against
    /// every title and path. That is the right tool for "never show me
    /// anything from this vendor" and the wrong one for "not this one":
    /// hiding a single entry by typing its name would take every other entry
    /// containing that word with it.
    #[serde(default)]
    pub hidden: Vec<String>,
}

/// Fields that are encrypted on their way to disk.
///
/// A path rather than a name, because "key" appears in plenty of places that
/// are not secret: `shortcutKey`, `finishKey`, `cancelKey`. Matching on the
/// word would have encrypted three keyboard settings and left the credential
/// alone the day someone renamed it.
const SEALED: &[&[&str]] = &[
    &["dictation", "provider", "apiKey"],
    // Every provider in the list, which is what the star is for: a person can
    // have a key for each of half a dozen services and there is no fixed path
    // that names them all.
    &["ai", "providers", "*", "apiKey"],
    &["tts", "provider", "apiKey"],
    // The extension store's GitHub token. Not a key to a service that costs
    // money, and still a credential that can read whatever the account it
    // belongs to can read.
    &["store", "githubToken"],
];

/// The step in a path that means "every element of this array".
const EACH: &str = "*";

/// Follows a path into a document, yielding every place it leads.
///
/// One place for an ordinary path, and one per element where the path crosses
/// an array. It used to return a single slot, which was enough while the only
/// secret lived at a fixed depth; a list of providers with a key each has no
/// fixed path, and missing one would write that key to the file in plain text.
fn at<'a>(root: &'a mut serde_json::Value, path: &[&str]) -> Vec<&'a mut serde_json::Value> {
    let Some((step, rest)) = path.split_first() else {
        return vec![root];
    };

    if *step == EACH {
        let Some(items) = root.as_array_mut() else {
            return Vec::new();
        };

        return items.iter_mut().flat_map(|item| at(item, rest)).collect();
    }

    match root.get_mut(step) {
        Some(node) => at(node, rest),
        None => Vec::new(),
    }
}

/// Encrypts every secret in a document about to be written.
///
/// A value that cannot be sealed is **removed rather than written through**.
/// Writing it would put the credential in the file in plain text, which is
/// the exact thing this exists to stop, and doing so silently is worse than
/// the user having to paste the key again.
fn seal_secrets(document: &mut serde_json::Value) {
    for path in SEALED {
        for slot in at(document, path) {
            let Some(plaintext) = slot.as_str() else {
                continue;
            };

            if plaintext.is_empty() || crate::secrets::is_sealed(plaintext) {
                continue;
            }

            match crate::secrets::seal(plaintext) {
                Some(sealed) => *slot = serde_json::Value::String(sealed),
                None => {
                    crate::say!(
                        "could not encrypt {}; it is left out of the file rather than                          written in plain text. You will have to enter it again",
                        path.join(".")
                    );
                    *slot = serde_json::Value::Null;
                }
            }
        }
    }
}

/// Decrypts every secret in a document just read.
///
/// A value with no marker is one an older build wrote in plain text. It is
/// passed through so the user does not lose a working setup on upgrade, and
/// the next save seals it.
fn unseal_secrets(document: &mut serde_json::Value) {
    for path in SEALED {
        for slot in at(document, path) {
            let Some(stored) = slot.as_str() else {
                continue;
            };

            if !crate::secrets::is_sealed(stored) {
                continue;
            }

            match crate::secrets::unseal(stored) {
                Some(plaintext) => *slot = serde_json::Value::String(plaintext),
                None => {
                    // Sealed by a different Windows account, or copied from
                    // another machine. Nothing can recover it, and leaving the
                    // blob in place would send it to the provider as a key.
                    crate::say!(
                        "{} could not be decrypted on this account and has been cleared",
                        path.join(".")
                    );
                    *slot = serde_json::Value::Null;
                }
            }
        }
    }
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            shortcuts: true,
            packaged_apps: true,
            app_paths: true,
            installed_programs: true,
            // The one source that starts off. See the field's own note.
            // Only new installs get this: a saved preferences file keeps
            // whatever it already said, because silently removing two thirds
            // of somebody's index on an update is not a default change, it is
            // a bug they cannot explain.
            path_executables: false,
            windows_settings: true,
            excluded: Vec::new(),
            hidden: Vec::new(),
        }
    }
}

/// File search.
///
/// Sill keeps its own index of the folders listed in `roots`, and additionally
/// asks a whole-volume indexer when one happens to be running. The two answer
/// different questions: ours knows the files somebody works on, and a
/// whole-volume index knows every file on the machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileSearch {
    pub enabled: bool,
    /// Results requested per query.
    pub max_results: u32,
    /// Match the whole path rather than just the file name.
    pub match_path: bool,
    /// Treat the query as case sensitive.
    pub match_case: bool,
    /// Treat the query as a regular expression.
    pub regex: bool,
    /// Only search these folders, when any are listed.
    ///
    /// A filter on what a whole-volume indexer returns, which is not the same
    /// thing as `roots`: this narrows results that already exist, where roots
    /// decide what gets indexed in the first place.
    pub only_in: Vec<String>,
    /// The folders Sill indexes itself.
    ///
    /// Empty means the home folder, resolved when the index is built rather
    /// than written into the file: a path baked into saved settings would be
    /// wrong for anybody who moved their profile, and would have to be
    /// migrated rather than simply meaning "wherever home is now".
    pub roots: Vec<String>,
    /// Whether to index anything at all.
    ///
    /// Separate from `enabled` because the two costs are different. Turning
    /// this off leaves file search working through a whole-volume indexer if
    /// one is running, and stops Sill holding an index of its own.
    pub index: bool,
}

impl FileSearch {
    /// The folders to index, with the default resolved.
    ///
    /// Home rather than a whole drive, which is what every launcher that
    /// builds its own index settles on. Measured here: a whole home folder is
    /// 2,272,143 files, and 42,976 once what `.gitignore` and the noise list
    /// rule out is left alone. The first number is not indexable at this
    /// budget and the second is.
    pub fn indexed_roots(&self) -> Vec<std::path::PathBuf> {
        if !self.index {
            return Vec::new();
        }

        let listed: Vec<std::path::PathBuf> = self
            .roots
            .iter()
            .map(|root| root.trim())
            .filter(|root| !root.is_empty())
            .map(std::path::PathBuf::from)
            .collect();

        if !listed.is_empty() {
            return listed;
        }

        home().into_iter().collect()
    }
}

/// Where the person using this keeps their own files.
fn home() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir())
}

/// Looking something up on the web.
///
/// On by default, unlike browser search. This reads nothing and knows nothing:
/// it is one row offering to open an address, and it only ever does anything
/// when it is deliberately chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WebSearch {
    pub enabled: bool,
    /// Which engine, by id.
    pub engine: String,
    /// An address of somebody's own, with `{query}` in it.
    ///
    /// Wins over `engine` when it holds anything, so an engine Sill has never
    /// heard of does not need a release.
    pub custom_url: String,
}

impl Default for WebSearch {
    fn default() -> Self {
        Self {
            enabled: true,
            // The one that does not build a profile of whoever is typing.
            // Every other engine here is two clicks away.
            engine: "duckduckgo".to_string(),
            custom_url: String::new(),
        }
    }
}

/// What a screenshot does once it has been taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AfterCapture {
    /// Straight to the clipboard, which is the fast path.
    Copy,
    /// Straight into the editor, for anybody who marks up most of what they
    /// take. It reaches the clipboard from there.
    Edit,
}

/// One key standing in for four modifiers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HyperKey {
    /// The virtual key code, or `None` for off.
    ///
    /// Off by default and never guessed at. This takes a key away from the
    /// system entirely, and choosing which one on somebody's behalf is
    /// choosing which key stops doing what it says on it.
    pub key: Option<u32>,
}

/// Script commands: files somebody keeps that the launcher can run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Scripts {
    /// Whether the folders below are scanned at all.
    ///
    /// Off until somebody names a folder, which is the same thing, but saying
    /// it separately means turning it off does not lose the folders.
    pub enabled: bool,
    /// The folders scanned for script commands, one level deep.
    ///
    /// Empty by default and never guessed at. A launcher that decided on its
    /// own to scan Documents for anything runnable would be finding commands
    /// nobody put there to be found.
    pub folders: Vec<String>,
    /// How long a script runs before it is stopped, in seconds.
    pub timeout_seconds: u64,
}

impl Default for Scripts {
    fn default() -> Self {
        Self {
            enabled: true,
            folders: Vec::new(),
            timeout_seconds: 60,
        }
    }
}

/// Taking pictures of the screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Screenshot {
    pub after: AfterCapture,
    /// Whether clicking a window in the picker captures that window.
    ///
    /// On by default: it is the difference between "drag a rectangle roughly
    /// around the window" and "click the window", and the second is what
    /// somebody meant nearly every time.
    pub click_a_window: bool,
    /// Which tool the editor opens with.
    pub tool: String,
    /// The colour it opens with.
    pub colour: String,
    /// The stroke width it opens with.
    pub weight: u32,
    /// The number the first badge shows.
    ///
    /// Somebody writing the second half of a walkthrough starts at seven, and
    /// the alternative is placing six badges and deleting them.
    pub step_from: u32,
}

impl Default for Screenshot {
    fn default() -> Self {
        Self {
            after: AfterCapture::Copy,
            click_a_window: true,
            tool: "box".to_string(),
            colour: "#ff3b30".to_string(),
            weight: 4,
            step_from: 1,
        }
    }
}

/// Reading what a browser remembers.
///
/// Off by default, and deliberately. Nothing else Sill reads is as personal as
/// a browsing history, and a launcher that quietly starts answering with it
/// because it was installed has helped itself to something nobody offered. It
/// is one switch away for anybody who wants it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Browsers {
    pub enabled: bool,
    /// Pages that were visited.
    pub history: bool,
    /// Pages that were saved, which is the smaller and more deliberate set.
    pub bookmarks: bool,
    /// Results requested per query.
    pub max_results: u32,
}

impl Default for Browsers {
    fn default() -> Self {
        Self {
            enabled: false,
            history: true,
            bookmarks: true,
            max_results: 6,
        }
    }
}

/// The extension store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Store {
    /// Only offer extensions that say they run on Windows.
    ///
    /// Raycast ships for macOS and for Windows, and its store is one index for
    /// both. The ones that name macOS and not Windows never reach the
    /// catalogue at all. This decides what happens to the third group: the
    /// 1,300 that name nothing because they were published before the field
    /// existed. On, they are hidden and counted; off, they are shown and
    /// marked.
    pub windows_only: bool,
    /// A GitHub token, to be allowed more requests per hour.
    ///
    /// Optional and empty by default. GitHub answers sixty requests an hour to
    /// an address that does not identify itself, and one install spends about
    /// three of them. That is enough for anybody installing extensions one at
    /// a time and not enough on a shared address where something else has
    /// already spent them.
    ///
    /// **Sealed**, so it is encrypted on its way to disk rather than sitting in
    /// the settings file in plain text. Its path is in `SEALED` and a test
    /// refuses a sealed path that names no real field.
    pub github_token: Option<String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            // On, because a store mostly full of things that will not run here
            // is a worse store than a smaller one that works.
            windows_only: true,
            github_token: None,
        }
    }
}

impl Default for FileSearch {
    fn default() -> Self {
        Self {
            enabled: true,
            max_results: 20,
            match_path: false,
            match_case: false,
            regex: false,
            only_in: Vec::new(),
            roots: Vec::new(),
            index: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub general: General,
    /// Dictation lives here rather than in its own file so one save writes
    /// everything, and so the hook can be armed from the same load that sets
    /// up the summon shortcut.
    pub dictation: crate::dictation::models::DictationSettings,
    /// How text is read aloud.
    #[serde(default)]
    pub tts: crate::tts::TtsSettings,
    /// The widgets and what is pinned.
    #[serde(default)]
    pub widgets: Widgets,
    pub hotkey: Hotkey,
    pub clipboard: ClipboardHistory,
    pub snippets: Snippets,
    /// Double-tapping a modifier to reach the launcher.
    #[serde(default)]
    pub taps: Taps,
    /// Who answers when you ask something.
    #[serde(default)]
    pub ai: Ai,
    pub appearance: Appearance,
    pub sources: Sources,
    pub files: FileSearch,
    pub browsers: Browsers,
    /// Browsing and installing extensions.
    #[serde(default)]
    pub store: Store,
    pub web_search: WebSearch,
    pub screenshot: Screenshot,
    pub scripts: Scripts,
    pub hyper: HyperKey,
    /// Global shortcuts that run an action without showing the launcher.
    #[serde(default, deserialize_with = "entries_that_can_be_read")]
    pub bindings: Vec<crate::bindings::Binding>,
    /// Names the user has chosen for things in the index.
    ///
    /// The one piece of ranking information that is not a guess, which is why
    /// an exact alias match outranks every other kind of match. A list rather
    /// than a map so the order shown in settings is the order they were made.
    #[serde(default, deserialize_with = "entries_that_can_be_read")]
    pub aliases: Vec<crate::registry::Alias>,
    /// Which keys move around the launcher.
    #[serde(default)]
    pub navigation: crate::navigation::Navigation,
    /// Skin tone and what Enter does, for the emoji picker.
    #[serde(default)]
    pub emoji: crate::emoji::Settings,
}

impl Appearance {
    /// Launcher height for the configured row count.
    ///
    /// Both constants are **measured from the rendered page**, not derived.
    /// Being a few pixels out shows as a partial row at the bottom, which
    /// reads as "there is more below" rather than as a mistake.
    ///
    /// Measured 2026-08-30, and it now adds up rather than being a number
    /// somebody landed on:
    ///
    /// | part                   | px | token             |
    /// | ---------------------- | -- | ----------------- |
    /// | search row             | 60 | `--search-height` |
    /// | its hairline           |  1 |                   |
    /// | the list's own padding |  8 | `--space-1` x 2   |
    /// | chin                   | 40 | `--chin-height`   |
    ///
    /// The chin's hairline is gone; it has its own recessed wash instead, and
    /// the action pill sitting on it carries that edge.
    pub fn window_height(&self) -> f64 {
        const CHROME: f64 = 109.0;
        /// Must equal `--row-height` in `src/lib/theme/theme.css`.
        ///
        /// Two sources of truth for one fact, because Rust sizes the window
        /// and cannot read CSS. `scripts/verify-source.mjs` reads both and
        /// fails if they drift, which is the only check that can: a Rust test
        /// asserting `CHROME + rows * ROW` only restates this formula and
        /// would pass for any pair of numbers.
        const ROW: f64 = 40.0;
        CHROME + f64::from(self.visible_rows.clamp(4, 16)) * ROW
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("preferences.json")
}

impl Preferences {
    /// Reads preferences, falling back to defaults.
    ///
    /// A malformed file is replaced by defaults rather than refused: a
    /// launcher that will not start because one setting is unparseable is
    /// worse than one that forgets a preference.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            // No file yet is the ordinary first run, not a failure.
            return Self::default();
        };

        /*
         * A byte order mark is not part of the JSON.
         *
         * Windows puts one on the front of any file written as "UTF-8" by
         * Notepad, by PowerShell's `Set-Content -Encoding UTF8`, and by a good
         * deal else. `serde_json` refuses the whole document over it, which
         * here means every setting and every sealed key is moved aside and the
         * defaults take over: a file somebody hand-edited to change one line
         * costs them all of the others.
         *
         * Skipped rather than rejected. It carries no information: the
         * encoding is already known, and nothing else in Sill writes one.
         */
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);

        let parsed = match serde_json::from_str::<serde_json::Value>(text) {
            Ok(mut value) => {
                unseal_secrets(&mut value);
                serde_json::from_value(value).map_err(|err| err.to_string())
            }
            Err(err) => Err(err.to_string()),
        };

        match parsed {
            Ok(prefs) => prefs,
            Err(why) => {
                /*
                 * Kept, rather than left where the next save will land on it.
                 *
                 * Falling back to defaults is right: a launcher that will not
                 * start because one file is malformed is worse than one that
                 * starts plain. What was wrong is that the defaults were then
                 * written straight over the file on the next settings change,
                 * so a single torn write, half-finished sync or hand edit
                 * silently destroyed every setting and every sealed key with
                 * no way back.
                 *
                 * Moving it aside costs nothing and makes it recoverable.
                 * `ai::chat` already does this; the file that holds the API
                 * keys deserves it at least as much.
                 */
                crate::say!("preferences could not be read, keeping them aside: {why}");
                let _ = std::fs::rename(path, path.with_extension("json.broken"));
                Self::default()
            }
        }
    }

    /// Writes preferences immediately.
    ///
    /// Not debounced. A debounced write has to be flushed on shutdown or the
    /// last change is lost, and that flush is the part that gets forgotten.
    /// These are written on a settings change, which is rare enough that the
    /// cost does not matter.
    ///
    /// Staged and renamed, the way `snippets::store::save` already does it. A
    /// plain write truncates first and fills afterwards, so losing power or
    /// being killed in that window leaves a half-written file that reads as
    /// corrupt on the next start. This is the file holding every setting and
    /// every sealed key, and a rename is atomic on NTFS, so there is no window
    /// in which it is neither the old file nor the new one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let text = match serde_json::to_value(self) {
            Ok(mut value) => {
                seal_secrets(&mut value);
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
            }
            Err(_) => "{}".into(),
        };

        let staging = path.with_extension("json.partial");
        std::fs::write(&staging, text)?;
        if let Err(err) = std::fs::rename(&staging, path) {
            let _ = std::fs::remove_file(&staging);
            return Err(err);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let p = Preferences::default();
        assert_eq!(p.hotkey.summon, "Alt+Space");
        assert_eq!(p.hotkey.switcher, "Ctrl+Alt+W");
        assert_ne!(
            p.hotkey.switcher, p.hotkey.summon,
            "two keys that mean different things cannot start out the same"
        );
        assert!(p.sources.shortcuts);
        assert!(p.files.enabled);
    }

    #[test]
    fn a_partial_file_keeps_defaults_for_everything_else() {
        // The upgrade case: a file written by an older version knows nothing
        // about fields added since, and must not reset them.
        // No switcher key in the stored file, which is every preferences
        // file written before the switcher existed.
        let json = r#"{"hotkey":{"summon":"Ctrl+Space"}}"#;
        let parsed: Preferences = serde_json::from_str(json).expect("partial input parses");

        assert_eq!(parsed.hotkey.summon, "Ctrl+Space", "the stated value wins");
        assert_eq!(
            parsed.hotkey.switcher, "Ctrl+Alt+W",
            "a field added later has to default rather than come back empty"
        );
        assert!(
            parsed.hotkey.dismiss_on_blur,
            "an omitted field inside a nested struct keeps its default"
        );
        assert!(
            parsed.sources.shortcuts,
            "an omitted section keeps its defaults"
        );
        // The same rule the other way up, which is the half that actually
        // bites: a default of `false` must survive too, or the check above
        // passes for any field that happens to default true.
        assert!(
            !parsed.sources.path_executables,
            "an omitted section keeps a default of false as well"
        );
        assert_eq!(parsed.files.max_results, 20);
    }

    #[test]
    fn an_unknown_field_does_not_fail_the_parse() {
        let json = r#"{"somethingRemoved":true,"files":{"enabled":false}}"#;
        let parsed: Preferences = serde_json::from_str(json).expect("unknown fields are ignored");
        assert!(!parsed.files.enabled);
    }

    #[test]
    fn a_round_trip_preserves_everything() {
        let mut original = Preferences::default();
        original.hotkey.summon = "Ctrl+Alt+K".to_string();
        original.appearance.backdrop = Backdrop::None;
        original.sources.path_executables = false;
        original.files.only_in = vec![r"C:\work".to_string()];

        let text = serde_json::to_string(&original).expect("serialises");
        let back: Preferences = serde_json::from_str(&text).expect("deserialises");

        assert_eq!(back.hotkey.summon, "Ctrl+Alt+K");
        assert_eq!(back.appearance.backdrop, Backdrop::None);
        assert!(!back.sources.path_executables);
        assert_eq!(back.files.only_in, vec![r"C:\work".to_string()]);
    }

    /// A key set in memory must not be readable in the file.
    ///
    /// This is the whole point of the exercise, so it is asserted against the
    /// bytes on disk rather than against a round trip, which would pass just
    /// as happily if nothing were encrypted at all.
    #[cfg(windows)]
    #[test]
    fn a_provider_key_never_reaches_the_file_in_plain_text() {
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("preferences.json");

        let secret = "sk-live-DO-NOT-LEAK-0123456789";
        let mut prefs = Preferences::default();
        prefs.dictation.provider.api_key = Some(secret.to_string());
        prefs.dictation.provider.base_url = Some("https://api.example.com".into());

        prefs.save(&path).expect("saved");

        let written = std::fs::read_to_string(&path).expect("readable");
        assert!(
            !written.contains(secret),
            "the key is sitting in the file:\n{written}"
        );
        assert!(
            written.contains("dpapi:v1:"),
            "nothing was sealed, so the field was simply dropped"
        );
        // Everything that is not a secret stays legible, because a settings
        // file nobody can read or edit by hand is its own problem.
        assert!(written.contains("https://api.example.com"));

        let back = Preferences::load(&path);
        assert_eq!(back.dictation.provider.api_key.as_deref(), Some(secret));
    }

    #[cfg(windows)]
    #[test]
    fn a_key_written_by_an_older_build_still_works_and_is_sealed_on_the_next_save() {
        // The upgrade path. Losing a working provider setup on update would
        // be a worse outcome than the plain text it is fixing.
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("preferences.json");

        let legacy = r#"{"dictation":{"provider":{"enabled":true,"apiKey":"legacy-plain-key"}}}"#;
        std::fs::write(&path, legacy).expect("wrote the old shape");

        let loaded = Preferences::load(&path);
        assert_eq!(
            loaded.dictation.provider.api_key.as_deref(),
            Some("legacy-plain-key"),
            "an unsealed key must be read as itself, not run through decrypt"
        );

        loaded.save(&path).expect("saved");
        let written = std::fs::read_to_string(&path).expect("readable");
        assert!(!written.contains("legacy-plain-key"), "still in plain text");
    }

    #[test]
    fn sealing_only_touches_the_credential() {
        // `shortcutKey`, `finishKey` and `cancelKey` all contain "key". A
        // name-based rule would have encrypted three keyboard settings and
        // left the actual secret alone.
        let mut document = serde_json::json!({
            "dictation": {
                "shortcutKey": "space",
                "finishKey": "enter",
                "provider": { "apiKey": "secret", "baseUrl": "https://x.test" }
            }
        });

        seal_secrets(&mut document);

        assert_eq!(document["dictation"]["shortcutKey"], "space");
        assert_eq!(document["dictation"]["finishKey"], "enter");
        assert_eq!(
            document["dictation"]["provider"]["baseUrl"],
            "https://x.test"
        );
    }

    #[test]
    fn documents_without_the_secret_are_left_alone() {
        // Every path step is optional: preferences written before the
        // dictation section existed must not panic their way through this.
        let mut bare = serde_json::json!({ "general": { "showInTray": true } });
        seal_secrets(&mut bare);
        unseal_secrets(&mut bare);
        assert_eq!(bare["general"]["showInTray"], true);

        let mut null_key = serde_json::json!({
            "dictation": { "provider": { "apiKey": serde_json::Value::Null } }
        });
        seal_secrets(&mut null_key);
        assert!(null_key["dictation"]["provider"]["apiKey"].is_null());
    }
}

#[cfg(test)]
mod sealed_paths {
    use super::*;

    /// Every API key in a saved document is sealed, wherever it lives.
    ///
    /// `SEALED` is a hand-written list of paths and nothing made it complete:
    /// a provider added at a new path is written to the file **in plain text**,
    /// silently, which is the exact failure the sealing exists to prevent. So
    /// this builds a document with a key at every place a key can be, saves
    /// it, and asserts none of them survived as plain text.
    ///
    /// It reads the real `Preferences` rather than a hand-made document, so a
    /// provider added to the type is covered without anybody remembering.
    #[test]
    fn no_api_key_is_written_in_plain_text() {
        const PLAIN: &str = "sk-plaintext-canary-0123456789";

        let mut prefs = Preferences::default();
        prefs.dictation.provider.api_key = Some(PLAIN.to_string());
        prefs.tts.provider.api_key = Some(PLAIN.to_string());
        prefs.store.github_token = Some(PLAIN.to_string());
        prefs.ai.providers.push(crate::ai::provider::Provider {
            api_key: PLAIN.to_string(),
            ..Default::default()
        });

        let mut document = serde_json::to_value(&prefs).expect("preferences serialise");
        seal_secrets(&mut document);

        let written = document.to_string();

        assert!(
            !written.contains(PLAIN),
            "an API key reached the file in plain text. Every place one can be \
             stored needs a path in SEALED"
        );
    }

    /// Every sealed path names a field that exists.
    ///
    /// The test above catches a credential with no path. This catches the
    /// opposite and quieter mistake: a path with a typo in it, or one left
    /// behind after the field it named was renamed. `at` walks a document and
    /// returns nothing for a path that leads nowhere, so a misspelled path
    /// seals nothing, reports nothing, and writes the credential in plain
    /// text, which is exactly the failure sealing exists to stop.
    ///
    /// Filled first, because `at` cannot walk into an absent field and an
    /// unset `Option` serialises to null.
    #[test]
    fn every_sealed_path_leads_somewhere_real() {
        let mut prefs = Preferences::default();
        prefs.dictation.provider.api_key = Some("x".to_string());
        prefs.tts.provider.api_key = Some("x".to_string());
        prefs.store.github_token = Some("x".to_string());
        prefs.ai.providers.push(crate::ai::provider::Provider {
            api_key: "x".to_string(),
            ..Default::default()
        });

        let mut document = serde_json::to_value(&prefs).expect("preferences serialise");

        for path in SEALED {
            assert!(
                !at(&mut document, path).is_empty(),
                "SEALED names {}, which is not a field in Preferences, so it seals \
                 nothing and says nothing",
                path.join(".")
            );
        }
    }

    /**
    Every section reads from an empty object.

    The module note says each struct carries `#[serde(default)]` on the struct
    as well as its fields, and until this existed nothing checked it. The way
    it fails is the reason it matters: a section that does not default cannot
    be read from a file written before that section existed, so the *whole*
    file fails, and every setting the user ever chose is replaced by defaults
    on the next save.

    Walking the serialised shape rather than naming the sections by hand, so a
    section added later is checked without anybody remembering to add it here.
    */
    #[test]
    fn every_section_can_be_read_from_an_empty_object() {
        let document = serde_json::to_value(Preferences::default()).expect("serialises");
        let serde_json::Value::Object(sections) = document else {
            panic!("preferences serialise as an object");
        };

        for (name, value) in sections {
            // Only the sections that are objects have this failure mode; a
            // bare string or number has nothing to omit.
            if !value.is_object() {
                continue;
            }

            let one = format!("{{\"{name}\":{{}}}}");
            let parsed = serde_json::from_str::<Preferences>(&one);

            assert!(
                parsed.is_ok(),
                "`{name}` cannot be read from an empty object, so a file written \
                 before one of its fields existed takes every other setting down \
                 with it. Add #[serde(default)] to the struct, not only its fields.\n\
                 {}",
                parsed.unwrap_err()
            );
        }
    }

    /// A file Notepad saved still reads.
    ///
    /// Windows writes a byte order mark on the front of anything it calls
    /// UTF-8: Notepad does, and so does PowerShell's `Set-Content -Encoding
    /// UTF8`. `serde_json` refuses the document over those three bytes, and
    /// refusing it here means the whole file is moved aside and the defaults
    /// take over, so somebody who hand-edited one line loses every other
    /// setting and every sealed key. This happened while testing something
    /// else, to a real preferences file.
    #[test]
    fn preferences_saved_with_a_byte_order_mark_still_read() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("preferences.json");

        let json = r#"{ "hotkey": { "summon": "Ctrl+Alt+F9" } }"#;
        std::fs::write(&path, format!("\u{feff}{json}")).expect("write");

        let prefs = Preferences::load(&path);

        assert_eq!(
            prefs.hotkey.summon, "Ctrl+Alt+F9",
            "a byte order mark threw away the whole file"
        );
        assert!(
            !path.with_extension("json.broken").exists(),
            "the file was moved aside over three bytes of encoding"
        );
    }

    /// One unreadable list entry costs that entry, not the file.
    #[test]
    fn a_binding_that_cannot_be_read_does_not_take_the_others_with_it() {
        // The middle one is missing `action`, which is required and has no
        // default. This is what a hand edit, or a field added to `Binding` in
        // a later version, looks like from an older file's point of view.
        let json = r#"{
            "hotkey": { "summon": "Ctrl+Space" },
            "bindings": [
                { "accelerator": "Ctrl+Alt+U", "action": "sill.text.upper", "source": { "from": "selection" } },
                { "accelerator": "Ctrl+Alt+B" },
                { "accelerator": "Ctrl+Alt+L", "action": "sill.text.lower", "source": { "from": "selection" } }
            ]
        }"#;

        let parsed: Preferences = serde_json::from_str(json).expect("the file still reads");

        assert_eq!(parsed.bindings.len(), 2, "the readable bindings survive");
        assert_eq!(parsed.bindings[0].accelerator, "Ctrl+Alt+U");
        assert_eq!(parsed.bindings[1].accelerator, "Ctrl+Alt+L");
        assert_eq!(
            parsed.hotkey.summon, "Ctrl+Space",
            "and the rest of the file is untouched, which is the whole point"
        );
    }
}
