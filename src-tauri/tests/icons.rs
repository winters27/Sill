//! Icon extraction against real files on this machine.
//!
//! Kept as a binary of its own while the rest of the suite moved into the
//! library, because it is not a function over values and it is not cheap:
//! **271 seconds** for these six on this machine, against 3.5 for the library's
//! entire test binary. Extracting an icon means opening somebody's executable
//! through GDI and shell APIs, and what that costs is a property of the disk
//! rather than of the code.

/// A cache of its own for each test.
///
/// The point of the change this follows: the cache was a `static`, so every
/// test in this file shared one and none could be given a fresh one. Without a
/// file, so nothing here reads or writes what a real run left behind.
fn icons() -> sill_lib::icons::Icons {
    sill_lib::icons::Icons::new(None)
}

#[test]
#[cfg(windows)]
fn extracts_an_icon_from_a_system_executable() {
    // explorer.exe is present on every Windows install and always has an icon.
    let uri = icons()
        .data_uri(r"C:\Windows\explorer.exe")
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
    let uri = icons().data_uri(r"C:\definitely\not\here\nope.lnk");
    assert!(uri.is_none(), "a path that does not exist has no icon");
}

#[test]
#[cfg(windows)]
fn results_are_cached() {
    // One cache, asked twice. Two caches would only show that extraction is
    // deterministic, which is a different and much weaker claim.
    let icons = icons();
    let path = r"C:\Windows\explorer.exe";

    let began = std::time::Instant::now();
    let first = icons.data_uri(path);
    let extracting = began.elapsed();

    let again = std::time::Instant::now();
    let second = icons.data_uri(path);
    let remembering = again.elapsed();

    assert_eq!(first, second, "the cache must return the same icon");
    assert!(
        remembering < extracting / 4 || remembering < std::time::Duration::from_micros(200),
        "the second answer took {remembering:?} against {extracting:?} for the first,          which is not a remembered answer"
    );
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

        match icons().data_uri(source) {
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
            icons().data_uri(exe).is_some(),
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
    let uri = icons()
        .data_uri(r"C:\Windows\explorer.exe")
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
