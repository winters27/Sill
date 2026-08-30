//! Windows settings pages, as launchable entries.
//!
//! Windows exposes several hundred settings pages behind `ms-settings:` URIs,
//! plus the older Control Panel applets and management consoles. None of them
//! are files, so no amount of scanning finds them; they only exist as a
//! catalog.
//!
//! The catalog is Microsoft's own, taken from PowerToys Run's Windows Settings
//! plugin (`WindowsSettings.json`, MIT licensed, Copyright (c) Microsoft
//! Corporation). It is embedded rather than read at runtime so the binary has
//! no data file to lose.

use std::collections::HashMap;

use serde::Deserialize;

use crate::registry::CommandRecord;

/// The catalog, embedded at build time.
const CATALOG: &str = include_str!("../../resources/WindowsSettings.json");

#[derive(Deserialize)]
struct Catalog {
    #[serde(rename = "Settings")]
    settings: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Command")]
    command: String,
    #[serde(rename = "Areas", default)]
    areas: Vec<String>,
    #[serde(rename = "AltNames", default)]
    alt_names: Vec<String>,
    /// Microsoft's own caveat about the row, where it has one.
    #[serde(rename = "Note", default)]
    note: Option<String>,
}

/// The catalog's rows, one per page that can actually be opened.
///
/// Two things stop the raw catalog being a usable list of results.
///
/// Three rows carry `NoteNoMscFileExist`, Microsoft's note that the console
/// file is not present. Their command is a bare `mmc.exe`, so choosing
/// "IP Security Monitor" opens an empty management console. A row that does
/// not do what its title says is worse than no row.
///
/// The rest name the same page more than once. "Shared Experiences", "Nearby
/// Share Settings" and "Share Across Devices" are all
/// `ms-settings:crossdevice`; three rows for one destination is three times
/// the list and no more reach. They fold into one row that still answers to
/// every name, which is shorter to read and no harder to find.
///
/// Folding them also makes the id unique, and that part is not cosmetic. The
/// id is what an alias, a hotkey, a hidden entry and a frecency score are all
/// keyed on, so four pages sharing one id means using any one of them promotes
/// all four, and hiding one hides four.
fn one_per_page(rows: Vec<Entry>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::with_capacity(rows.len());
    let mut at: HashMap<String, usize> = HashMap::new();

    for row in rows {
        if row.note.as_deref() == Some("NoteNoMscFileExist") {
            continue;
        }

        match at.get(&row.command) {
            // The first row names the page. Later ones only add ways to reach it.
            Some(&i) => {
                let held = &mut out[i];
                if held.name != row.name && !held.alt_names.contains(&row.name) {
                    held.alt_names.push(row.name);
                }
            }
            None => {
                at.insert(row.command.clone(), out.len());
                out.push(row);
            }
        }
    }

    out
}

/// Splits a PascalCase identifier into words.
///
/// The catalog stores localisation keys, not display text: "VideoPlayback",
/// "PermissionsAndHistory". PowerToys resolves those against resource files
/// per language. Splitting on case boundaries gets readable English without
/// carrying a translation table, and leaves runs of capitals alone so "VPN"
/// and "USB" survive intact.
fn humanize(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    let mut out = String::with_capacity(key.len() + 8);

    for (i, &c) in chars.iter().enumerate() {
        let starts_word = i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_ascii_digit()
                // The end of an acronym: the "S" in "USBSettings".
                || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));

        if starts_word {
            out.push(' ');
        }
        out.push(c);
    }

    out
}

/// A file whose icon represents this command.
///
/// None of these commands is itself a file, so the icon has to come from
/// whatever program services them. That gives settings pages the Settings app
/// icon, consoles the MMC icon, and each Control Panel applet its own, which
/// is exactly what Explorer shows for the same items.
fn icon_source(command: &str) -> Option<String> {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let system32 = format!(r"{system_root}\System32");

    let exists = |path: String| std::path::Path::new(&path).is_file().then_some(path);

    if command.starts_with("ms-settings:") {
        // The modern Settings app, which is what every ms-settings URI opens.
        return exists(format!(
            r"{system_root}\ImmersiveControlPanel\SystemSettings.exe"
        ))
        .or_else(|| exists(format!(r"{system32}\control.exe")));
    }

    // A `.cpl` is a DLL and carries its own icon, which is nicer than a
    // generic one: appwiz.cpl shows the Programs and Features icon.
    if let Some(applet) = command.split_whitespace().find(|t| t.ends_with(".cpl")) {
        if let Some(found) = exists(format!(r"{system32}\{applet}")) {
            return Some(found);
        }
    }

    // A `.msc` is XML, not a PE file, so it has no icon of its own. The
    // console that opens it does.
    if command.contains(".msc") {
        return exists(format!(r"{system32}\mmc.exe"));
    }

    // Everything else names its own program. Most are written without the
    // extension, e.g. "control /name Microsoft.ActionCenter", so `.exe` has to
    // be tried as well: looking only for a file literally called "control"
    // finds nothing and leaves a third of the catalog with no icon.
    let program = command.split_whitespace().next()?;
    exists(format!(r"{system32}\{program}"))
        .or_else(|| exists(format!(r"{system32}\{program}.exe")))
        .or_else(|| exists(program.to_string()))
        .or_else(|| exists(format!("{program}.exe")))
}

/// Every settings page, as registry entries.
pub fn load() -> Vec<CommandRecord> {
    let Ok(catalog) = serde_json::from_str::<Catalog>(CATALOG.trim_start_matches('\u{feff}'))
    else {
        eprintln!("[sill] the Windows settings catalog could not be parsed");
        return Vec::new();
    };

    one_per_page(catalog.settings)
        .into_iter()
        .map(|entry| {
            let title = humanize(&entry.name);

            // "AreaNetworkAndInternet" is the section the page sits under.
            let area = entry
                .areas
                .first()
                .map(|a| humanize(a.trim_start_matches("Area")))
                .unwrap_or_default();

            let kind = match entry.kind.as_str() {
                "AppControlPanel" => "Control Panel",
                "AppMMC" => "System",
                _ => "Setting",
            };

            // Searchable by section and by whatever aliases the catalog knows,
            // so "network proxy" finds the proxy page even though its own name
            // is only "Proxy".
            let mut keywords: Vec<String> = entry.alt_names.iter().map(|a| humanize(a)).collect();
            if !area.is_empty() {
                keywords.push(area.clone());
            }

            let icon = icon_source(&entry.command);

            CommandRecord {
                id: format!("setting:{}", entry.command),
                extension: "setting".to_string(),
                extension_title: kind.to_string(),
                command: entry.command.clone(),
                title,
                subtitle: area,
                description: String::new(),
                mode: "setting".to_string(),
                entrypoint: entry.command,
                keywords,
                icon,
                // A Windows settings page, which draws the shell's own icon.
                panel: None,
                // Only extension commands carry any.
                preferences: serde_json::Value::Null,
            }
        })
        .collect()
}

/// Runs a settings entry.
///
/// Three shapes live in one catalog: an `ms-settings:` URI the shell opens, a
/// `control.exe` invocation with arguments, and a bare management console.
/// Only the first is a URL, so the others are spawned as processes.
pub fn launch(command: &str) -> Result<(), String> {
    if command.starts_with("ms-settings:") {
        return tauri_plugin_opener::open_url(command, None::<&str>).map_err(|e| e.to_string());
    }

    let mut parts = command.split_whitespace();
    let program = parts.next().ok_or_else(|| "empty command".to_string())?;
    let args: Vec<&str> = parts.collect();

    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not run {command}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::{humanize, icon_source, load};
    use std::collections::HashMap;

    /// The failure this guards against blanks the entire result list.
    ///
    /// The frontend draws results with a keyed loop, and a repeated key is a
    /// hard error there rather than a duplicated row: nothing renders at all.
    /// Four settings shared the id `setting:mmc.exe`, so the launcher opened on
    /// an empty list until this was found.
    ///
    /// The id also carries meaning of its own. Aliases, hotkeys, hidden entries
    /// and frecency scores are all keyed on it, so two rows sharing one id
    /// silently share all four.
    #[test]
    fn no_two_pages_share_an_id() {
        let mut seen: HashMap<String, String> = HashMap::new();

        for row in load() {
            if let Some(first) = seen.insert(row.id.clone(), row.title.clone()) {
                panic!("{} is the id of both {first:?} and {:?}", row.id, row.title);
            }
        }
    }

    /// Microsoft ships three rows whose console file does not exist.
    ///
    /// Their command is a bare `mmc.exe`, so they open an empty management
    /// console rather than the thing they are named after.
    #[test]
    fn pages_that_cannot_open_are_not_offered() {
        let rows = load();

        assert!(
            !rows.iter().any(|r| r.entrypoint == "mmc.exe"
                && r.title.starts_with("Ip Security")),
            "a page with no console file is still being offered",
        );
    }

    /// Folding rows must not cost the names they were found by.
    ///
    /// The cases here are ones the catalog does not already cover: the second
    /// row's name is not among the first row's alternates, so the only way it
    /// still finds the page is if folding carried it across.
    #[test]
    fn a_folded_page_answers_to_every_name_it_had() {
        let rows = load();

        for (command, name) in [
            ("ms-settings:bluetooth", "Devices"),
            ("ms-settings:windowsupdate", "Windows Update Check For Updates"),
        ] {
            let page = rows
                .iter()
                .find(|r| r.entrypoint == command)
                .unwrap_or_else(|| panic!("{command} is missing entirely"));

            assert!(
                page.keywords.iter().any(|k| k == name),
                "{name:?} no longer finds {command}, keywords: {:?}",
                page.keywords,
            );
        }
    }


    #[test]
    fn pascal_case_becomes_words() {
        assert_eq!(humanize("VideoPlayback"), "Video Playback");
        assert_eq!(humanize("PermissionsAndHistory"), "Permissions And History");
        assert_eq!(
            humanize("AreaNetworkAndInternet"),
            "Area Network And Internet"
        );
    }

    #[test]
    #[cfg(windows)]
    fn every_command_shape_resolves_to_an_icon() {
        // The four shapes in the catalog, each of which needs a different
        // file to borrow an icon from.
        for command in [
            "ms-settings:bluetooth",
            "control appwiz.cpl",
            "compmgmt.msc",
            "control.exe",
        ] {
            let found = icon_source(command);
            assert!(found.is_some(), "no icon source for {command:?}");
            assert!(
                std::path::Path::new(&found.unwrap()).is_file(),
                "the icon source for {command:?} does not exist"
            );
        }
    }

    #[test]
    fn acronyms_are_left_alone() {
        // A run of capitals is one word; only the last capital before a
        // lowercase letter starts the next one.
        assert_eq!(humanize("VPN"), "VPN");
        assert_eq!(humanize("USBSettings"), "USB Settings");
    }
}
