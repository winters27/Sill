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
    // One cache for this probe, rather than a process-wide one.
    let icons = sill_lib::icons::Icons::new(None);

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
            .and_then(|path| icons.data_uri(path))
            .map(|uri| format!("{} bytes", uri.len()))
            .unwrap_or_else(|| "NO ICON".to_string());

        println!(
            "{icon:22} {size:>12}  {} pages   e.g. {}",
            pages.len(),
            pages[0]
        );
    }
}

/// Two sections drawn with the same picture is the failure this is about.
#[test]
#[ignore = "reads this machine's System32"]
fn sections_do_not_all_look_the_same() {
    // One cache for this probe, rather than a process-wide one.
    let icons = sill_lib::icons::Icons::new(None);
    let mut pictures: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for record in sill_lib::settings_catalog::load() {
        let Some(icon) = record.icon.as_deref() else {
            continue;
        };
        let Some(uri) = icons.data_uri(icon) else {
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

/// No switch is drawn with an error sign.
///
/// `bthprops.cpl` with no index means index 0, and index 0 of that file is a
/// yellow warning triangle, so the Bluetooth switch shipped wearing one. It
/// was chosen by a rule that said a small icon is a blank icon, and the
/// triangle is the smallest of the three in there.
///
/// This asks the only question that catches it: is the picture Sill draws the
/// same picture as the triangle. Not ignored, because it needs no hardware and
/// this is exactly the kind of thing nobody looks at twice.
#[test]
fn the_bluetooth_switch_is_not_wearing_a_warning_triangle() {
    // One cache for this probe, rather than a process-wide one.
    let icons = sill_lib::icons::Icons::new(None);
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let triangle = icons.data_uri(&format!(r"{root}\System32\bthprops.cpl,0"));

    // Not `else { return }`. The first version of this test built the path
    // wrong, so this came back `None`, so the test returned before comparing
    // anything and passed while the triangle was still shipping. A test that
    // can quietly check nothing is worse than no test.
    let triangle = triangle.expect("the warning triangle to compare against");

    for row in sill_lib::registry::builtins() {
        let Some(icon) = row.icon.as_deref() else {
            continue;
        };
        let Some(drawn) = icons.data_uri(icon) else {
            continue;
        };

        assert_ne!(
            drawn, triangle,
            "{} is drawn with a warning triangle, from {icon}",
            row.title,
        );
    }
}

/// Which system files are worth taking an icon from.
///
/// The tool for choosing one. Size is a hint: a source under about two
/// thousand bytes is often drawing an empty picture rather than a small one,
/// next to `hdwwiz.cpl` at five thousand.
///
/// **A hint and not a test, and this file used to say otherwise.** It named
/// `bthprops.cpl` as what a blank looks like. It is not blank. Its three icons
/// come back at 386, 2887 and 426 bytes: the smallest is a yellow warning
/// triangle, and the 426-byte one is a perfectly good Bluetooth glyph that
/// compresses small because it is a flat two-colour shape. Sill shipped the
/// warning triangle on the Bluetooth switch on the strength of that rule.
///
/// So: use the size to order the candidates, then **look at them**. Run
/// `probe_icons`'s contact sheet and open the file. Edit the list and run this
/// before mapping a section to anything.
#[test]
#[ignore = "reads this machine's System32"]
fn compare_candidates() {
    // One cache for this probe, rather than a process-wide one.
    let icons = sill_lib::icons::Icons::new(None);
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());

    let candidates = [
        "SearchIndexer.exe",
        "SearchApp.exe",
        "SearchFilterHost.exe",
        "desk.cpl",
        "main.cpl",
        "PhoneExperienceHost.exe",
        "WFS.exe",
        "SecurityHealthSystray.exe",
        "wscui.cpl",
        "powercfg.cpl",
        "SystemPropertiesRemote.exe",
        "rstrui.exe",
        "OneDriveSetup.exe",
    ];

    for name in candidates {
        let path = format!(r"{root}\System32\{name}");
        if !std::path::Path::new(&path).exists() {
            println!("{name:28} missing");
            continue;
        }
        let size = icons.data_uri(&path).map(|u| u.len()).unwrap_or(0);
        println!("{name:28} {size:>7} bytes");
    }
}
