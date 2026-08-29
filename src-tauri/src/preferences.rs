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
    /// Opens the launcher straight into the window switcher.
    ///
    /// Its own key rather than a mode you type into: the value of a switcher
    /// is that one press puts the window you were last in under the cursor,
    /// and a prefix you have to type first spends that. Empty turns it off.
    #[serde(default = "default_switcher")]
    pub switcher: String,
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
}

/// Fields that are encrypted on their way to disk.
///
/// A path rather than a name, because "key" appears in plenty of places that
/// are not secret: `shortcutKey`, `finishKey`, `cancelKey`. Matching on the
/// word would have encrypted three keyboard settings and left the credential
/// alone the day someone renamed it.
const SEALED: &[&[&str]] = &[&["dictation", "provider", "apiKey"]];

/// Follows a path into a document, if every step of it exists.
fn at<'a>(root: &'a mut serde_json::Value, path: &[&str]) -> Option<&'a mut serde_json::Value> {
    path.iter().try_fold(root, |node, step| node.get_mut(step))
}

/// Encrypts every secret in a document about to be written.
///
/// A value that cannot be sealed is **removed rather than written through**.
/// Writing it would put the credential in the file in plain text, which is
/// the exact thing this exists to stop, and doing so silently is worse than
/// the user having to paste the key again.
fn seal_secrets(document: &mut serde_json::Value) {
    for path in SEALED {
        let Some(slot) = at(document, path) else {
            continue;
        };
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

/// Decrypts every secret in a document just read.
///
/// A value with no marker is one an older build wrote in plain text. It is
/// passed through so the user does not lose a working setup on upgrade, and
/// the next save seals it.
fn unseal_secrets(document: &mut serde_json::Value) {
    for path in SEALED {
        let Some(slot) = at(document, path) else {
            continue;
        };
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
    /// Global shortcuts that run an action without showing the launcher.
    #[serde(default)]
    pub bindings: Vec<crate::bindings::Binding>,
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
            .and_then(
                |text| match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(mut value) => {
                        unseal_secrets(&mut value);
                        match serde_json::from_value(value) {
                            Ok(prefs) => Some(prefs),
                            Err(err) => {
                                crate::say!("preferences could not be read, using defaults: {err}");
                                None
                            }
                        }
                    }
                    Err(err) => {
                        crate::say!("preferences could not be read, using defaults: {err}");
                        None
                    }
                },
            )
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

        let text = match serde_json::to_value(self) {
            Ok(mut value) => {
                seal_secrets(&mut value);
                serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".into())
            }
            Err(_) => "{}".into(),
        };

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
