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
        return exists(format!(r"{system_root}\ImmersiveControlPanel\SystemSettings.exe"))
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
    let Ok(catalog) = serde_json::from_str::<Catalog>(CATALOG.trim_start_matches('\u{feff}')) else {
        eprintln!("[sill] the Windows settings catalog could not be parsed");
        return Vec::new();
    };

    catalog
        .settings
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
            let mut keywords: Vec<String> =
                entry.alt_names.iter().map(|a| humanize(a)).collect();
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
    use super::{humanize, icon_source};

    #[test]
    fn pascal_case_becomes_words() {
        assert_eq!(humanize("VideoPlayback"), "Video Playback");
        assert_eq!(humanize("PermissionsAndHistory"), "Permissions And History");
        assert_eq!(humanize("AreaNetworkAndInternet"), "Area Network And Internet");
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
