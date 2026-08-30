//! What icon each Windows settings page ends up wearing.
//!
//! Ignored by default: it reads this machine's System32. Run it deliberately.
//!
//!     cargo test --test probe_setting_icons -- --ignored --nocapture

use std::collections::BTreeMap;

#[test]
#[ignore = "reads this machine's System32"]
fn report_generic_pages() {
    for record in sill_lib::settings_catalog::load() {
        let icon = record.icon.clone().unwrap_or_default();
        if icon.ends_with("SystemSettings.exe") || icon.ends_with("control.exe") {
            println!("{:34} {}", record.title, record.entrypoint);
        }
    }
}

#[test]
#[ignore = "reads this machine's System32"]
fn report_section_icons() {
    let mut by_icon: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for record in sill_lib::settings_catalog::load() {
        let icon = record.icon.clone().unwrap_or_else(|| "(none)".into());
        let name = icon.rsplit('\\').next().unwrap_or(&icon).to_string();
        by_icon.entry(name).or_default().push(record.title);
    }

    println!("{} distinct icon sources", by_icon.len());
    println!();

    for (icon, pages) in &by_icon {
        // Does it actually produce a picture, and a different one each time?
        let path = sill_lib::settings_catalog::load()
            .into_iter()
            .find(|r| r.icon.as_deref().map(|i| i.ends_with(icon.as_str())) == Some(true))
            .and_then(|r| r.icon);

        let size = path
            .as_deref()
            .and_then(sill_lib::icons::icon_data_uri)
            .map(|uri| format!("{} bytes", uri.len()))
            .unwrap_or_else(|| "NO ICON".to_string());

        println!("{icon:22} {size:>12}  {} pages   e.g. {}", pages.len(), pages[0]);
    }
}

/// Two sections drawn with the same picture is the failure this is about.
#[test]
#[ignore = "reads this machine's System32"]
fn sections_do_not_all_look_the_same() {
    let mut pictures: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for record in sill_lib::settings_catalog::load() {
        let Some(icon) = record.icon.as_deref() else {
            continue;
        };
        let Some(uri) = sill_lib::icons::icon_data_uri(icon) else {
            continue;
        };

        pictures.entry(uri).or_default().push(icon.to_string());
    }

    println!("{} distinct pictures across the catalog", pictures.len());
    assert!(
        pictures.len() > 4,
        "the whole catalog is drawn with {} picture(s), which is the thing this was meant to fix",
        pictures.len(),
    );
}

/// Which system files are worth taking an icon from.
///
/// The tool for choosing one. Size is the signal: a source that comes back
/// under about two thousand bytes is drawing an empty picture rather than a
/// small one, and `bthprops.cpl` at 870 is what a blank looks like next to
/// `hdwwiz.cpl` at five thousand. Edit the list and run it before mapping a
/// section to anything.
#[test]
#[ignore = "reads this machine's System32"]
fn compare_candidates() {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());

    let candidates = [
        "SearchIndexer.exe", "SearchApp.exe", "SearchFilterHost.exe",
        "desk.cpl", "main.cpl", "PhoneExperienceHost.exe", "WFS.exe",
        "SecurityHealthSystray.exe", "wscui.cpl", "powercfg.cpl",
        "SystemPropertiesRemote.exe", "rstrui.exe", "OneDriveSetup.exe",
    ];

    for name in candidates {
        let path = format!(r"{root}\System32\{name}");
        if !std::path::Path::new(&path).exists() {
            println!("{name:28} missing");
            continue;
        }
        let size = sill_lib::icons::icon_data_uri(&path)
            .map(|u| u.len())
            .unwrap_or(0);
        println!("{name:28} {size:>7} bytes");
    }
}
