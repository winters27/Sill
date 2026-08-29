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
    /// Dismiss when the window loses focus.
    pub dismiss_on_blur: bool,
    /// Select the existing query on summon so typing replaces it.
    pub select_query_on_summon: bool,
    /// Return to the root list every time the launcher is summoned.
    pub reset_on_summon: bool,
}

impl Default for Hotkey {
    fn default() -> Self {
        Self {
            // Matches Raycast on Windows, which is the muscle memory being
            // replaced.
            summon: "Alt+Space".to_string(),
            dismiss_on_blur: true,
            select_query_on_summon: true,
            reset_on_summon: false,
        }
    }
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

impl Default for Snippets {
    fn default() -> Self {
        Self {
            expand_keywords: false,
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
}

impl Default for ClipboardHistory {
    fn default() -> Self {
        Self {
            enabled: true,
            retain_days: 30,
            keep_images: true,
            ignored_apps: Vec::new(),
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
        }
    }
}

/// File search, which is delegated to Everything.
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
    pub only_in: Vec<String>,
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
    pub hotkey: Hotkey,
    pub clipboard: ClipboardHistory,
    pub snippets: Snippets,
    pub appearance: Appearance,
    pub sources: Sources,
    pub files: FileSearch,
}

impl Appearance {
    /// Launcher height for the configured row count.
    ///
    /// `CHROME` is the search bar, both hairlines and the footer, measured
    /// from the rendered page rather than derived: the search bar's height
    /// comes from its font's line box, which no constant here can predict.
    /// Being a few pixels out shows as a partial row at the bottom, which
    /// reads as "there is more below" rather than as a mistake.
    pub fn window_height(&self) -> f64 {
        const CHROME: f64 = 87.0;
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
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| match serde_json::from_str(&text) {
                Ok(value) => Some(value),
                Err(err) => {
                    crate::say!("preferences could not be read, using defaults: {err}");
                    None
                }
            })
            .unwrap_or_default()
    }

    /// Writes preferences immediately.
    ///
    /// Not debounced. A debounced write has to be flushed on shutdown or the
    /// last change is lost, and that flush is the part that gets forgotten.
    /// These are written on a settings change, which is rare enough that the
    /// cost does not matter.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let p = Preferences::default();
        assert_eq!(p.hotkey.summon, "Alt+Space");
        assert!(p.sources.shortcuts);
        assert!(p.files.enabled);
    }

    #[test]
    fn a_partial_file_keeps_defaults_for_everything_else() {
        // The upgrade case: a file written by an older version knows nothing
        // about fields added since, and must not reset them.
        let json = r#"{"hotkey":{"summon":"Ctrl+Space"}}"#;
        let parsed: Preferences = serde_json::from_str(json).expect("partial input parses");

        assert_eq!(parsed.hotkey.summon, "Ctrl+Space", "the stated value wins");
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
}
