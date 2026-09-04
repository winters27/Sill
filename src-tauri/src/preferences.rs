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

/// System integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct General {
    /// Start Sill when the user signs in.
    pub open_at_login: bool,
    /// Keep an icon in the notification area.
    pub show_in_tray: bool,
    /// Write the per-keystroke and per-summon lines to the log as well.
    ///
    /// For somebody chasing a fault, and off the rest of the time: those lines
    /// run on every search and every extension load, and a log full of them
    /// rotates past the hour that was actually wanted. **It only ever adds**;
    /// see `log.rs` on why there is no setting in the other direction.
    #[serde(default)]
    pub detailed_log: bool,
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
            // Off, because it is the setting somebody turns on for an
            // afternoon and forgets, and what it costs is the log.
            detailed_log: false,
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
    /// Neutral canvas with a warm fringe down one edge and a cool one down
    /// the other, the way a lens that cannot focus every wavelength at one
    /// point renders a high-contrast edge. The second theme with a chroma
    /// wash, and the only one whose colour lives at the edges rather than
    /// the middle.
    Aberration,
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

/// Which screen the launcher appears on.
///
/// A choice rather than a rule, because the right answer depends on how
/// somebody works. The window used to be centred once at startup and never
/// moved again, so it always came up on the primary monitor however far away
/// that was, and a display change could leave it off every screen entirely.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SummonOn {
    /// The screen the mouse is on. The default, and what most launchers do:
    /// the pointer is the cheapest available guess at where somebody is
    /// looking, and it is right whenever they reached for the keyboard from
    /// whatever they were just clicking.
    Cursor,
    /// The screen holding the window that had focus.
    ///
    /// Better than the cursor for anybody who leaves the mouse parked on one
    /// screen and works on another, which is the ordinary arrangement with a
    /// portrait second monitor.
    ActiveWindow,
    /// Always the primary screen, which is what it did before.
    Primary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    pub backdrop: Backdrop,
    /// Which palette everything is drawn in.
    pub theme: Theme,
    /// How strongly a theme's chroma wash is painted, 0 to 2.
    ///
    /// Only Oilslick and Aberration have one. A multiplier rather than a set of alphas so the
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
    /// Which screen it comes up on.
    pub summon_on: SummonOn,
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
            summon_on: SummonOn::Cursor,
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
    /// How many unpinned entries are kept. Zero keeps as many as arrive.
    ///
    /// Beside the retention rather than instead of it. An age bounds how long
    /// something survives and says nothing about how much arrives in that
    /// time; a count is the other way round. Ten thousand is generous enough
    /// that nobody meets it by using the launcher normally, and small enough
    /// that a runaway script copying in a loop does not fill the disk.
    pub max_entries: u32,
    /// Keep images as well as text. They are much the largest thing a
    /// clipboard carries, so this is worth being able to decline.
    pub keep_images: bool,
    /// Lock stored pictures to this Windows account.
    ///
    /// Off by default, because it is a promise with edges and the person
    /// should be the one to accept them. What it buys: another account on this
    /// machine cannot open them, and neither can anyone holding a copy of the
    /// file, from a backup, a synced folder or a drive taken out of the
    /// machine. What it does not buy: anything running as this user can unlock
    /// them exactly the way Sill does.
    ///
    /// Pictures only. The text of an entry is what full-text search reads, so
    /// encrypting it would mean either losing search or keeping a plaintext
    /// index beside the ciphertext, and the second is a promise that is not
    /// true.
    pub encrypt_images: bool,
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
            max_entries: 10_000,
            keep_images: true,
            encrypt_images: false,
            ignored_apps: Vec::new(),
            secrets: crate::clipboard::sensitive::Policy::default(),
        }
    }
}

/// Which places are searched for launchable things.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    /// Entries kept at the top of the root list, in the order shown.
    ///
    /// The opposite of `hidden`, and stored the same way: ids rather than
    /// titles, because a title changes when an application updates and an id
    /// does not.
    ///
    /// A list rather than a set, because the order is the point. Somebody who
    /// pins five things has arranged five things, and re-sorting them by how
    /// often each is opened would undo the arranging on the first launch.
    #[serde(default)]
    pub pinned: Vec<String>,
}

/// Fields that are encrypted on their way to disk.
///
/// A path rather than a name, because "key" appears in plenty of places that
/// are not secret: `shortcutKey`, `finishKey`, `cancelKey`. Matching on the
/// word would have encrypted three keyboard settings and left the credential
/// alone the day someone renamed it.
pub(crate) const SEALED: &[&[&str]] = &[
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
pub(crate) const EACH: &str = "*";

/// Follows a path into a document, yielding every place it leads.
///
/// One place for an ordinary path, and one per element where the path crosses
/// an array. It used to return a single slot, which was enough while the only
/// secret lived at a fixed depth; a list of providers with a key each has no
/// fixed path, and missing one would write that key to the file in plain text.
///
/// Reachable from `preferences_transfer` rather than private here, so that
/// taking credentials out of an export and putting them back after an import
/// walk to exactly the same places as sealing and unsealing. A second walker
/// there would be a second list of what counts as a secret, and the day the
/// two disagreed the export would be the one carrying a key.
pub(crate) fn at<'a>(
    root: &'a mut serde_json::Value,
    path: &[&str],
) -> Vec<&'a mut serde_json::Value> {
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
                        "could not encrypt {}; it is left out of the file rather than \
                         written in plain text. You will have to enter it again",
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
pub(crate) fn unseal_secrets(document: &mut serde_json::Value) {
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
            pinned: Vec::new(),
        }
    }
}

/// File search.
///
/// Sill keeps its own index of the folders listed in `roots`, and additionally
/// asks a whole-volume indexer when one happens to be running. The two answer
/// different questions: ours knows the files somebody works on, and a
/// whole-volume index knows every file on the machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// The scripts allowed to ask Windows for administrator rights, by path.
    ///
    /// **Named one file at a time, and only from here.** A script's own header
    /// can ask for administrator rights and never grant them: a header is
    /// somebody else's writing, and the UAC prompt Windows shows names
    /// `powershell.exe` rather than the file, so there is nothing on that
    /// dialog to decide with. This list is the deciding, made in advance, by
    /// the person, about one script. Nothing writes it but the settings
    /// window: not a script, not an extension, not the model.
    ///
    /// Empty by default, and a folder is never a member. Allowing a folder
    /// would mean allowing every file dropped into it afterwards, which is
    /// the grant this list exists to avoid.
    pub elevated: Vec<String>,
}

impl Default for Scripts {
    fn default() -> Self {
        Self {
            enabled: true,
            folders: Vec::new(),
            timeout_seconds: 60,
            elevated: Vec::new(),
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
    #[serde(
        default,
        deserialize_with = "crate::json_store::entries_that_can_be_read"
    )]
    pub bindings: Vec<crate::bindings::Binding>,
    /// Names the user has chosen for things in the index.
    ///
    /// The one piece of ranking information that is not a guess, which is why
    /// an exact alias match outranks every other kind of match. A list rather
    /// than a map so the order shown in settings is the order they were made.
    #[serde(
        default,
        deserialize_with = "crate::json_store::entries_that_can_be_read"
    )]
    pub aliases: Vec<crate::registry::Alias>,
    /// Which keys move around the launcher.
    #[serde(default)]
    pub navigation: crate::navigation::Navigation,
    /// Which chord runs which action, where it differs from the default.
    ///
    /// The sibling of `navigation`: that one is movement, this one is doing
    /// something to what is selected. Both hold only what was changed, so an
    /// action whose default is later reconsidered gets the new one.
    #[serde(default)]
    pub action_keys: crate::action_keys::Settings,
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

/// How the file is kept. See `json_store` for what each part buys.
///
/// `Beside` rather than a wrapper, because this is the one Sill file people
/// genuinely open and edit, and pushing every section a level deeper under
/// `items` would cost them more than the version is worth here.
///
/// The two hooks are why preferences call `json_store` through its `_with`
/// variants: the credentials in the document are sealed on the way out and
/// unsealed on the way in, and both happen to the `Value` rather than to the
/// struct so that a key never exists in plain text on disk.
pub(crate) const SCHEMA: crate::json_store::Schema = crate::json_store::Schema {
    version: 1,
    shape: crate::json_store::Shape::Beside,
    layout: crate::json_store::Layout::Readable,
    unreadable: crate::json_store::Unreadable::KeepAside,
    what: "preferences",
};

impl Preferences {
    /// Reads preferences, falling back to defaults.
    ///
    /// A malformed file is replaced by defaults rather than refused: a
    /// launcher that will not start because one setting is unparseable is
    /// worse than one that forgets a preference. It is kept aside first, so
    /// the next settings change cannot write the defaults over every setting
    /// and every sealed key with no way back.
    pub fn load(path: &Path) -> Self {
        crate::json_store::load_with(path, &SCHEMA, unseal_secrets)
    }

    /// Writes preferences immediately.
    ///
    /// Not debounced. A debounced write has to be flushed on shutdown or the
    /// last change is lost, and that flush is the part that gets forgotten.
    /// These are written on a settings change, which is rare enough that the
    /// cost does not matter.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::json_store::save_atomic_with(path, self, &SCHEMA, seal_secrets)
    }
}

impl Sources {
    /// The switches that decide what a scan goes and looks at.
    ///
    /// `excluded` and `hidden` are deliberately not here. They are read on
    /// every query, so a word added to either takes effect on the next
    /// keystroke and asking the machine to scan itself again for them would
    /// be a minute of work to change nothing.
    fn scanned(&self) -> [bool; 6] {
        [
            self.shortcuts,
            self.packaged_apps,
            self.app_paths,
            self.installed_programs,
            self.path_executables,
            self.windows_settings,
        ]
    }
}

/// What a save changed that somebody has to go and redo.
///
/// Three settings used to be read once and then never again: the source
/// switches gate a scan, the script folders are walked when they change, and
/// the indexed roots decide what the file index holds. Turning one on left the
/// panel saying it was on and the index saying it was not, with nothing but a
/// restart or the Rebuild button in between, and the Sources panel said in so
/// many words that the change had already happened.
///
/// A plain comparison, apart from the command that acts on it, so the rule
/// "this setting is one the index has to be told about" can be read in one
/// place and tested without an application.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Redo {
    /// The switches that gate the scan changed, so the sources are read again.
    pub sources: bool,
    /// Script scanning changed, so the folders are walked again.
    pub scripts: bool,
    /// The folders the file index covers changed. `Some` carries the new set,
    /// because the rebuild and the watcher both need it and resolving the
    /// default twice could disagree.
    pub file_roots: Option<Vec<std::path::PathBuf>>,
}

impl Redo {
    /// What has to happen for the new preferences to be true.
    pub fn between(previous: &Preferences, next: &Preferences) -> Self {
        let roots = next.files.indexed_roots();

        Self {
            sources: previous.sources.scanned() != next.sources.scanned(),
            // The timeout is read when a script is run, so changing it is not
            // a reason to walk the folders again.
            scripts: previous.scripts.enabled != next.scripts.enabled
                || previous.scripts.folders != next.scripts.folders,
            file_roots: (previous.files.indexed_roots() != roots).then_some(roots),
        }
    }

    /// Whether anything at all has to be redone.
    pub fn is_empty(&self) -> bool {
        !self.sources && !self.scripts && self.file_roots.is_none()
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

    /// Both clipboard bounds survive a write and a read.
    ///
    /// A setting a person can choose has to outlive a restart, and the two
    /// ways that quietly fails are a name the window spells differently and a
    /// field nothing serialises. Both would look like the setting reverting
    /// itself with no message anywhere.
    #[test]
    fn the_clipboard_limit_and_the_picture_lock_are_written_and_read_back() {
        let mut prefs = Preferences::default();
        prefs.clipboard.max_entries = 500;
        prefs.clipboard.encrypt_images = true;

        let json = serde_json::to_string(&prefs).expect("serialises");
        assert!(json.contains("\"maxEntries\":500"), "{json}");
        assert!(json.contains("\"encryptImages\":true"));

        let back: Preferences = serde_json::from_str(&json).expect("parses");
        assert_eq!(back.clipboard.max_entries, 500);
        assert!(back.clipboard.encrypt_images);
    }

    /// And a history file written before either existed gets the defaults.
    #[test]
    fn a_clipboard_section_from_an_older_build_keeps_the_new_defaults() {
        let json = r#"{"clipboard":{"retainDays":7}}"#;
        let parsed: Preferences = serde_json::from_str(json).expect("parses");

        assert_eq!(parsed.clipboard.retain_days, 7, "the stated value wins");
        assert_eq!(
            parsed.clipboard.max_entries, 10_000,
            "a bound added later must not arrive as zero or as one"
        );
        assert!(
            !parsed.clipboard.encrypt_images,
            "and a promise about encryption is never made by default"
        );
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

    #[test]
    fn an_action_key_survives_being_written_and_read_back() {
        // The bug this whole item exists to prevent, asserted against the file
        // rather than against the object in hand. A settings screen that
        // updates what it is holding and never reaches disk looks correct from
        // every angle except the one that matters, which is opening Sill
        // again tomorrow. `P0-01` was exactly that on this panel.
        let dir = tempfile::tempdir().expect("a temp dir");
        let path = dir.path().join("preferences.json");

        let mut prefs = Preferences::default();
        prefs
            .action_keys
            .overrides
            .insert("sill.copyPath".to_string(), "Ctrl+Alt+P".to_string());
        // Turning one off is a stored value too, and an empty string is the
        // one most likely to be dropped on the way through.
        prefs
            .action_keys
            .overrides
            .insert("sill.copyName".to_string(), String::new());

        prefs.save(&path).expect("saved");

        let written = std::fs::read_to_string(&path).expect("readable");
        assert!(
            written.contains("actionKeys"),
            "the section never reached the file:\n{written}"
        );

        let back = Preferences::load(&path);
        assert_eq!(
            back.action_keys
                .overrides
                .get("sill.copyPath")
                .map(String::as_str),
            Some("Ctrl+Alt+P")
        );
        assert_eq!(
            back.action_keys
                .overrides
                .get("sill.copyName")
                .map(String::as_str),
            Some(""),
            "an action turned off came back as one that was never touched"
        );

        // And the chord means the same thing after the trip, which is the
        // half a string comparison alone would not prove.
        let chosen = crate::action_keys::effective(&back.action_keys, "sill.copyPath", None)
            .expect("a chord");
        assert_eq!(chosen.chord(), "Ctrl+Alt+P");
        assert_eq!(chosen.key, "p");
        assert_eq!(
            crate::action_keys::effective(&back.action_keys, "sill.copyName", None),
            None
        );
    }

    #[test]
    fn a_file_from_before_action_keys_existed_still_reads() {
        // Every preferences file on a machine today. Adding a section must
        // not be the upgrade that resets somebody's settings.
        let json = r#"{"hotkey":{"summon":"Ctrl+Space"}}"#;
        let parsed: Preferences = serde_json::from_str(json).expect("it parses");

        assert!(parsed.action_keys.overrides.is_empty());
        assert_eq!(parsed.hotkey.summon, "Ctrl+Space");
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
    Every object reachable from `Preferences` reads from an empty one.

    The module note says each struct carries `#[serde(default)]` on the struct
    as well as its fields, and until this existed nothing checked it. The way
    it fails is the reason it matters: a struct that does not default cannot be
    read from a file written before one of its fields existed, so the *whole*
    file fails, and every setting the user ever chose is replaced by defaults
    on the next save.

    Every object, not only the top-level sections. The three structs `P0-03`
    found without the attribute were nested ones, and a check that stops at the
    first level would have passed for all three: `dictation` reads from `{}`
    perfectly well while `dictation.provider` does not.

    Walking the serialised shape rather than naming anything by hand, so a
    struct added later is checked without anybody remembering to add it here.
    Each object is emptied on its own, with the rest of the document left
    intact, because emptying all of them at once is the one case that was
    already covered.
    */
    #[test]
    fn every_object_reachable_from_preferences_reads_from_an_empty_one() {
        let document = serde_json::to_value(Preferences::default()).expect("serialises");

        for place in objects_within(&document, String::new()) {
            let mut emptied = document.clone();

            *emptied
                .pointer_mut(&place)
                .expect("a path taken from this document") = serde_json::json!({});

            let parsed = serde_json::from_value::<Preferences>(emptied);

            assert!(
                parsed.is_ok(),
                "`{}` cannot be read from an empty object, so a file written before \
                 one of its fields existed takes every other setting down with it. \
                 Add #[serde(default)] to the struct, not only to its fields.\n{}",
                if place.is_empty() {
                    "the whole file"
                } else {
                    place.trim_start_matches('/')
                },
                parsed.unwrap_err()
            );
        }
    }

    /// Every object in a document, as JSON pointers, itself included.
    ///
    /// Fields only. An element of a list cannot be omitted the way a field can,
    /// so emptying one asks a different question, and the one it asks is
    /// already answered by `entries_that_can_be_read`.
    fn objects_within(value: &serde_json::Value, at: String) -> Vec<String> {
        let serde_json::Value::Object(fields) = value else {
            return Vec::new();
        };

        let mut found = vec![at.clone()];

        for (name, held) in fields {
            // `~` and `/` are the two characters a JSON pointer escapes. No
            // field here contains either, and a silently wrong path would make
            // this test pass by looking at nothing.
            assert!(
                !name.contains('~') && !name.contains('/'),
                "`{name}` needs escaping before it can be a JSON pointer"
            );

            found.extend(objects_within(held, format!("{at}/{name}")));
        }

        found
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
