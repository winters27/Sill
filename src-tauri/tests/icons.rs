//! Icon extraction against real files on this machine.

#[test]
#[cfg(windows)]
fn extracts_an_icon_from_a_system_executable() {
    // explorer.exe is present on every Windows install and always has an icon.
    let uri = sill_lib::icons::icon_data_uri(r"C:\Windows\explorer.exe")
        .expect("explorer.exe should have an icon");

    assert!(
        uri.starts_with("data:image/png;base64,"),
        "an icon is returned as a PNG data URI, got: {}",
        &uri[..uri.len().min(40)]
    );

    // The PNG signature proves real image bytes came back rather than an
    // empty buffer that merely encoded cleanly.
    let payload = uri.trim_start_matches("data:image/png;base64,");
    let bytes = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("the payload should be valid base64")
    };

    assert_eq!(
        &bytes[..8],
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        "the decoded bytes should start with the PNG signature"
    );
    assert!(
        bytes.len() > 200,
        "a real icon is more than a stub, got {} bytes",
        bytes.len()
    );
}

#[test]
#[cfg(windows)]
fn a_missing_file_yields_no_icon_rather_than_failing() {
    let uri = sill_lib::icons::icon_data_uri(r"C:\definitely\not\here\nope.lnk");
    assert!(uri.is_none(), "a path that does not exist has no icon");
}

#[test]
#[cfg(windows)]
fn results_are_cached() {
    let path = r"C:\Windows\explorer.exe";
    let first = sill_lib::icons::icon_data_uri(path);
    let second = sill_lib::icons::icon_data_uri(path);
    assert_eq!(first, second, "the cache must return the same icon");
}

/// Reports which extraction path each real shortcut takes. Diagnostic, not a
/// pass/fail gate: run with --nocapture to see it.
#[test]
#[cfg(windows)]
fn report_icon_paths_for_real_shortcuts() {
    let apps = sill_lib::apps::scan_shortcuts();
    let mut with_icon = 0usize;
    let mut without = Vec::new();
    let mut no_target = Vec::new();

    for app in &apps {
        let Some(source) = app.icon_source.as_ref() else {
            continue;
        };

        if source.to_ascii_lowercase().ends_with(".lnk")
            && sill_lib::lnk::target_of(std::path::Path::new(source)).is_none()
        {
            no_target.push(app.name.as_str());
        }

        match sill_lib::icons::icon_data_uri(source) {
            Some(_) => with_icon += 1,
            None => without.push(app.name.as_str()),
        }
    }

    eprintln!(
        "shortcuts {}, icons {}, none {}, unresolvable target {}",
        apps.len(),
        with_icon,
        without.len(),
        no_target.len()
    );
    eprintln!("no target: {:?}", &no_target[..no_target.len().min(12)]);
    eprintln!("no icon:   {:?}", &without[..without.len().min(12)]);
}

/// The badge rule, stated as a test so it cannot quietly regress.
#[test]
#[cfg(windows)]
fn a_plain_executable_still_gets_an_icon() {
    // Removing the composited fallback to kill shortcut badges once broke
    // every non-shortcut file, because those never carried a badge to begin
    // with. The fallback is only skipped for shortcuts.
    for exe in [
        r"C:\Windows\explorer.exe",
        r"C:\Windows\System32\notepad.exe",
    ] {
        assert!(
            sill_lib::icons::icon_data_uri(exe).is_some(),
            "{exe} should have an icon; the badge rule must not apply to non-shortcuts"
        );
    }
}

#[test]
#[cfg(windows)]
fn icons_arrive_with_more_pixels_than_they_are_drawn_at() {
    use base64::Engine;

    // Rows draw at 22 CSS pixels. The system "large" icon is 32 at 100%
    // scale, and 32 into 22 is a 0.69 downscale off an already small source,
    // which is soft at exactly the size the launcher shows it. Anyone
    // reverting to `ExtractIconExW`, which cannot be asked for a size, gets
    // caught here.
    let uri = sill_lib::icons::icon_data_uri(r"C:\Windows\explorer.exe")
        .expect("explorer.exe should have an icon");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(uri.trim_start_matches("data:image/png;base64,"))
        .expect("valid base64");

    // IHDR's width and height sit at a fixed offset in every PNG.
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());

    assert_eq!(width, height, "icons are square, got {width}x{height}");
    assert!(
        width >= 48,
        "an icon drawn at 22 needs more than {width} pixels to downscale from"
    );
}
