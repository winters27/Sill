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

/// The icon for the section a settings page sits in.
///
/// Windows draws every section of its own Settings app with a mark of its own:
/// a clock for Time and Language, a speaker for Sound, a globe for Network. A
/// list where all three hundred pages wear the same Settings icon throws that
/// away, and the icon is most of how somebody finds the right row at a glance.
///
/// Those marks live inside the Settings app's own resources and are not
/// something a program can ask for. What is available is the Control Panel
/// applet behind each section, which is where Windows drew the same idea
/// before, and which every one of these pages still resolves to underneath.
/// A clock is a clock.
///
/// Each of these was extracted and looked at rather than taken from a list, and
/// the ones with nothing convincing behind them are deliberately absent: a
/// wrong icon is worse than the section's parent program, which is at least
/// honest about being generic.
/// A page's own mark, where Windows has one for that exact page.
///
/// The most specific answer wins, so this is asked before the section is. A
/// page that Windows itself draws with a monitor, a speaker or a clock should
/// wear that rather than the section it happens to sit under: Sound is filed
/// under System, and a speaker says more about it than a computer does.
///
/// Only pages with a real applet of their own are here. A page with nothing
/// specific behind it falls through to its section, which is the point: a
/// setting inside a page inherits the page it is inside.
fn page_icon(command: &str) -> Option<String> {
    let page = command.strip_prefix("ms-settings:")?;

    let file = match page {
        "display" | "display-advanced" | "display-advancedgraphics" | "screenrotation" => {
            "desk.cpl"
        }
        "sound" | "sound-devices" | "apps-volume" => "mmsys.cpl",
        "mousetouchpad" | "mouse" | "devices-touchpad" => "main.cpl",
        "printers" | "printers-scanners" => "printui.dll",
        "dateandtime" => "timedate.cpl",
        "regionformatting" | "regionlanguage" | "regionlanguage-languageoptions" => "intl.cpl",
        "powersleep" | "batterysaver" | "batterysaver-settings" | "batterysaver-usagedetails" => {
            "powercfg.cpl"
        }
        "recovery" => "rstrui.exe",
        "storagesense" | "storagepolicies" => "cleanmgr.exe",
        "optionalfeatures" => "OptionalFeatures.exe",
        "appsfeatures" | "appsfeatures-app" | "defaultapps" => "appwiz.cpl",
        "windowsdefender" => "SecurityHealthSystray.exe",
        "about" => "sysdm.cpl",
        "easeofaccess-magnifier" => "Magnify.exe",
        "easeofaccess-narrator" => "Narrator.exe",
        "easeofaccess-keyboard" => "osk.exe",
        "cortana-windowssearch" | "search" | "search-permissions" => "SearchIndexer.exe",
        _ => return None,
    };

    in_system32(file)
}

/// The section a page belongs to, read from the address that opens it.
///
/// The catalog files most pages under an area, and where it does that is the
/// better answer. It does not always: thirty entries carry no area at all, and
/// a few carry one for a section that no longer has a mark of its own. Those
/// pages were the ones still wearing the plain Settings icon, and several of
/// them are sub-pages of a section that does have one, which is the case this
/// is for. `privacy-holographic-environment` is a Privacy page whichever
/// headset it is about.
///
/// `ms-settings:` addresses are named after the section they sit in, and every
/// sub-page of a section begins with the same word: `privacy-webcam`,
/// `privacy-microphone`, `privacy-location`. So the first word of the address
/// says which section a page belongs to, and a page nobody has filed still
/// knows where it lives.
fn command_icon(command: &str) -> Option<String> {
    // A Control Panel command usually names its applet outright, and the
    // applet is a better icon than the program that launches it:
    // `control desk.cpl,,@screensaver` is a display page and should look like
    // one rather than like the Control Panel itself.
    if let Some(applet) = command
        .split(|c: char| c.is_whitespace() || c == ',')
        .find(|part| part.to_lowercase().ends_with(".cpl"))
    {
        if let Some(found) = in_system32(applet) {
            return Some(found);
        }
    }

    let page = command.strip_prefix("ms-settings:")?;
    // The section is the first word. Everything after the first dash is which
    // page of that section it is.
    let section = page.split('-').next()?;

    let file = match section {
        "privacy" => "SecurityHealthSystray.exe",

        "personalization" | "colors" | "themes" | "lockscreen" | "fonts" | "backgrounds"
        | "taskbar" | "startmenu" => "themecpl.dll",

        "display" | "sound" | "notifications" | "powersleep" | "batterysaver" | "battery"
        | "storagesense" | "multitasking" | "project" | "remotedesktop" | "about"
        | "nightlight" | "clipboard" | "quiethours" | "focusassist" | "tabletmode"
        | "screenrotation" | "deviceencryption" | "crossdevice" => "sysdm.cpl",

        "network" | "proxy" | "wifi" | "mobilehotspot" | "vpn" | "ethernet" | "dialup"
        | "datausage" | "airplanemode" | "nfctransactions" | "cellular" | "netswitcher" => {
            "ncpa.cpl"
        }

        "bluetooth" | "devices" | "printers" | "mousetouchpad" | "typing" | "pen" | "autoplay"
        | "usb" | "mobile" | "wheel" | "camera" => "hdwwiz.cpl",

        "apps" | "appsfeatures" | "defaultapps" | "optionalfeatures" | "appsforwebsites"
        | "maps" | "startupapps" | "videoplayback" | "appvolume" => "appwiz.cpl",

        "emailandaccounts" | "otherusers" | "signinoptions" | "sync" | "workplace" | "yourinfo"
        | "assignedaccess" | "family" => "netplwiz.exe",

        "dateandtime" | "regionlanguage" | "regionformatting" | "keyboard" | "speech"
        | "language" | "region" | "typingsettings" => "timedate.cpl",

        "gaming" | "gamebar" | "gamedvr" | "gamemode" | "broadcasting" | "trueplay"
        | "quietmomentsgame" => "joy.cpl",

        "easeofaccess" => "Magnify.exe",

        "windowsupdate" | "windowsdefender" | "backup" | "recovery" | "troubleshoot"
        | "activation" | "findmydevice" | "developers" | "windowsinsider" | "delivery"
        | "storagerecommendations" => "wscui.cpl",

        // Cortana and Windows Search are one section, and the indexer is the
        // only program on the machine wearing that section's mark.
        "cortana" | "search" => "SearchIndexer.exe",

        // Mixed reality, Surface Hub and the odds and ends have nothing
        // convincing behind them. They keep the plain Settings icon, which is
        // at least honest about being generic.
        _ => return None,
    };

    in_system32(file)
}

/// A system file, when this machine has it.
fn in_system32(file: &str) -> Option<String> {
    let path = format!(
        r"{}\System32\{file}",
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
    );

    std::path::Path::new(&path).exists().then_some(path)
}

/// The sections that have a mark of their own.
///
/// Listed so a test can check every one of them still resolves. `access.cpl`
/// was the natural choice for Ease of Access and Windows 11 does not ship it,
/// which nothing would have noticed: a missing file falls back to the page's
/// own program and simply looks like the feature was never built.
#[cfg(test)]
const MAPPED_AREAS: &[&str] = &[
    "AreaTimeAndLanguage",
    "AreaClockAndRegion",
    "AreaNetworkAndInternet",
    "AreaHardwareAndSound",
    "AreaEaseOfAccess",
    "AreaAppearanceAndPersonalization",
    "AreaPersonalization",
    "AreaAccounts",
    "AreaUserAccounts",
    "AreaApps",
    "AreaPrograms",
    "AreaSystem",
    "AreaSystemPropertiesAdvanced",
    "AreaDevices",
    "AreaBluetoothAndDevices11",
    "AreaSystemAndSecurity",
    "AreaSecurityAndMaintenance",
    "AreaUpdateAndSecurity",
    "AreaPrivacy",
    "AreaGaming",
];

fn area_icon(area: &str) -> Option<String> {
    let file = match area {
        "AreaTimeAndLanguage" | "AreaClockAndRegion" => "timedate.cpl",
        "AreaNetworkAndInternet" => "ncpa.cpl",
        "AreaHardwareAndSound" => "mmsys.cpl",
        "AreaAppearanceAndPersonalization" | "AreaPersonalization" => "themecpl.dll",
        "AreaAccounts" | "AreaUserAccounts" => "netplwiz.exe",
        "AreaApps" | "AreaPrograms" => "appwiz.cpl",
        "AreaSystem" | "AreaSystemPropertiesAdvanced" => "sysdm.cpl",
        // Not `bthprops.cpl`, which is the obvious choice and draws almost
        // nothing: 870 bytes against five thousand for this one. Every icon
        // here was extracted and measured, and a source that comes back that
        // small is an empty picture rather than a small one.
        "AreaDevices" | "AreaBluetoothAndDevices11" => "hdwwiz.cpl",
        "AreaSystemAndSecurity" | "AreaSecurityAndMaintenance" | "AreaUpdateAndSecurity" => {
            "wscui.cpl"
        }
        // Kept apart from security on purpose. They are neighbouring ideas and
        // seventy pages under one mark is most of the catalog looking alike.
        "AreaPrivacy" => "SecurityHealthSystray.exe",
        // A magnifier, which is the mark accessibility is drawn with
        // everywhere. `access.cpl` would be the natural answer and Windows 11
        // does not ship it at all.
        "AreaEaseOfAccess" => "Magnify.exe",
        // Game controllers. There is no Settings-era gaming icon to be had:
        // both game bar programs draw an empty picture.
        "AreaGaming" => "joy.cpl",
        // The catalog carries a couple of raw file names in this field rather
        // than an area, and they are already the answer.
        other if other.ends_with(".cpl") => other,
        _ => return None,
    };

    in_system32(file)
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

            // Named the way Windows names them, because that is what tells
            // one of these apart from one of Sill's own settings. "Setting" on
            // its own says which program it belongs to only if you already
            // know.
            let kind = match entry.kind.as_str() {
                "AppControlPanel" => "Control Panel",
                // What Windows now calls the folder these live in.
                "AppMMC" => "Windows Tools",
                _ => "Windows Settings",
            };

            // Searchable by section and by whatever aliases the catalog knows,
            // so "network proxy" finds the proxy page even though its own name
            // is only "Proxy".
            let mut keywords: Vec<String> = entry.alt_names.iter().map(|a| humanize(a)).collect();
            if !area.is_empty() {
                keywords.push(area.clone());
            }

            // The section it lives in, then the program that opens it. A page
            // wearing its section's mark says where it came from, which is how
            // somebody picks the right row out of three hundred.
            // The section it is filed under, then the section its address
            // says it belongs to, then the program that opens it. A page
            // wearing its section's mark says where it came from, which is how
            // somebody picks the right row out of three hundred, and a
            // sub-page of a section is still that section.
            /*
             * Most specific first.
             *
             * A page Windows draws with a mark of its own wears that mark. A
             * page with nothing of its own belongs to a section, and wears the
             * section's: a setting inside a page is still that page.
             *
             * The section is known two ways. The catalog files most pages under
             * an area, which is the better answer where it has one. Where it
             * has none, the address says it anyway, because every sub-page of a
             * section begins with the section's name.
             *
             * Only then the program that opens it, which says nothing about
             * what the page is but is at least honest about that.
             */
            let icon = page_icon(&entry.command)
                .or_else(|| entry.areas.first().and_then(|area| area_icon(area)))
                .or_else(|| command_icon(&entry.command))
                .or_else(|| icon_source(&entry.command));

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
    use super::{area_icon, command_icon, humanize, icon_source, load, page_icon, MAPPED_AREAS};
    use std::collections::HashMap;

    /// What a page wears, in order of how specific it is.
    mod which_mark_a_page_wears {
        use super::*;

        fn ends_with(icon: Option<String>, file: &str) -> bool {
            icon.map(|i| i.to_lowercase().ends_with(&file.to_lowercase()))
                .unwrap_or(false)
        }

        fn icon_of(command: &str) -> Option<String> {
            load()
                .into_iter()
                .find(|record| record.entrypoint == command)
                .and_then(|record| record.icon)
        }

        /// A page with a mark of its own wears it, over its section's.
        ///
        /// Sound is filed under System. A speaker says more about that page
        /// than a computer does, so the more specific answer wins.
        #[test]
        fn a_page_with_its_own_mark_keeps_it() {
            assert!(
                ends_with(icon_of("ms-settings:sound"), "mmsys.cpl"),
                "Sound wore its section's mark instead of its own: {:?}",
                icon_of("ms-settings:sound"),
            );

            assert!(
                ends_with(icon_of("ms-settings:display"), "desk.cpl"),
                "Display wore its section's mark instead of its own: {:?}",
                icon_of("ms-settings:display"),
            );
        }

        /// A page inside a section wears the section's mark.
        #[test]
        fn a_page_inside_a_section_inherits_it() {
            let webcam = icon_of("ms-settings:privacy-webcam");
            let microphone = icon_of("ms-settings:privacy-microphone");

            assert!(webcam.is_some(), "the webcam privacy page has no mark at all");
            assert_eq!(
                webcam, microphone,
                "two pages of the same section were drawn differently",
            );
        }

        /// Even when the catalog files it somewhere with no mark of its own.
        ///
        /// `privacy-holographic-environment` is filed under mixed reality,
        /// which has nothing behind it, and it is a Privacy page whichever
        /// headset it is about. It used to be one of the plain ones.
        #[test]
        fn a_page_nobody_filed_still_knows_its_section() {
            assert_eq!(
                icon_of("ms-settings:privacy-holographic-environment"),
                icon_of("ms-settings:privacy-webcam"),
                "an unfiled privacy page did not land with the other privacy pages",
            );
        }

        /// A Control Panel command names its applet, which is its own mark.
        #[test]
        fn a_control_panel_command_wears_the_applet_it_names() {
            assert!(
                ends_with(command_icon("control desk.cpl,,@screensaver"), "desk.cpl"),
                "the screen saver page did not wear the display applet",
            );
        }

        /// A section with nothing convincing behind it keeps the plain mark
        /// rather than borrowing one that would say the wrong thing.
        #[test]
        fn a_section_with_no_mark_does_not_borrow_one() {
            assert_eq!(page_icon("ms-settings:surfacehub-welcome"), None);
            assert_eq!(command_icon("ms-settings:surfacehub-welcome"), None);
        }

        /// The whole point: most of the catalog should not look alike.
        #[test]
        fn the_catalog_is_not_all_one_picture() {
            let marks: std::collections::HashSet<String> =
                load().into_iter().filter_map(|record| record.icon).collect();

            assert!(
                marks.len() >= 20,
                "three hundred pages are drawn with only {} mark(s)",
                marks.len(),
            );
        }
    }

    /// Every section that claims a mark of its own must still have one.
    ///
    /// A source that is not on this machine falls back silently to the page's
    /// own program, so the section simply looks like it was never given an
    /// icon. That is exactly what `access.cpl` did: it is the obvious answer
    /// for Ease of Access and Windows 11 removed it.
    #[test]
    fn every_section_with_a_mark_can_still_find_it() {
        for area in MAPPED_AREAS {
            assert!(
                area_icon(area).is_some(),
                "{area} names an icon source this machine does not have",
            );
        }
    }

    /// A section nothing convincing was found for keeps the page's own program.
    #[test]
    fn an_unmapped_section_falls_back_rather_than_guessing() {
        assert_eq!(area_icon("AreaCortana"), None);
        assert_eq!(area_icon("AreaSomethingFromTheFuture"), None);
    }

    /// The catalog carries a couple of raw file names where an area should be.
    #[test]
    fn a_raw_file_name_in_the_area_field_is_used_as_it_stands() {
        let found = area_icon("bthprops.cpl").expect("the file is there");

        assert!(found.ends_with("bthprops.cpl"), "{found}");
    }

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
