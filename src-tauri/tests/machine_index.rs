//! What the index looks like on the machine this is running on.
//!
//! Six tests that walk the real Start Menu, ask Windows for its packaged
//! applications and extract an icon per entry. They were in `registry.rs`
//! beside a thousand tests that are arithmetic over a fixture, and when that
//! file moved into the library they took `cargo test --lib` from **3.5 seconds
//! to 460**. Measured individually, on this machine:
//!
//! | Test | Alone |
//! | --- | --- |
//! | `report_icon_coverage` | 437.9 s |
//! | `report_category_distribution` | 101.3 s |
//! | `nothing_past_the_root_list_cap_becomes_unreachable` | 42.7 s |
//! | `app_paths_entries_do_not_duplicate_shortcuts` | 41.1 s |
//! | `the_apps_folder_finds_what_the_start_menu_walk_misses` | a few seconds |
//! | `the_start_menu_scan_finds_something` | 0.1 s |
//!
//! So they are a binary of their own, which is what a test that reads the
//! whole machine should have been all along. `scan_apps_folder` runs
//! `Get-StartApps` in a real PowerShell, the icon pass opens every executable
//! on the disk through GDI, and none of it is a function over values. What
//! they answer is real and worth answering; it is simply not the thing
//! somebody wants to wait for between two edits.
//!
//! `report_` is a diagnostic rather than an assertion: run it with
//! `--nocapture` when the numbers are the question.

use sill_lib::registry::{search, Frecency};

const NOW: i64 = 1_756_000_000;

#[test]
fn the_start_menu_scan_finds_something() {
    // Not asserting a specific app: this is about the walk working at all on
    // whatever machine runs it. An empty result on Windows means the roots or
    // the extension filter are wrong.
    let found = sill_lib::apps::scan_shortcuts();

    if cfg!(windows) {
        assert!(
            !found.is_empty(),
            "the Start Menu scan found no applications at all"
        );
        assert!(
            found.iter().all(|a| !a.name.is_empty()),
            "every application needs a display name"
        );

        let lowered: Vec<String> = found.iter().map(|a| a.name.to_lowercase()).collect();
        assert!(
            !lowered.iter().any(|n| n.starts_with("uninstall")),
            "uninstallers should be filtered out of a launcher"
        );
    }
}

#[test]
#[cfg(windows)]
fn the_apps_folder_finds_what_the_start_menu_walk_misses() {
    let shortcuts = sill_lib::apps::scan_shortcuts();
    let packaged = sill_lib::apps::scan_apps_folder();

    assert!(
        !packaged.is_empty(),
        "Get-StartApps returned nothing; the Apps folder scan is broken"
    );

    let known: std::collections::HashSet<String> =
        shortcuts.iter().map(|a| a.name.to_lowercase()).collect();

    let extra: Vec<&str> = packaged
        .iter()
        .filter(|a| !known.contains(&a.name.to_lowercase()))
        .map(|a| a.name.as_str())
        .collect();

    // The whole reason for the second scan: the Start Menu walk cannot see
    // packaged apps, so the Apps folder must contribute entries of its own.
    assert!(
        !extra.is_empty(),
        "the Apps folder added nothing over the Start Menu walk, so one of the two scans is wrong"
    );

    eprintln!(
        "shortcuts: {}, apps folder: {}, only in apps folder: {} (e.g. {:?})",
        shortcuts.len(),
        packaged.len(),
        extra.len(),
        &extra[..extra.len().min(5)]
    );

    // The scan returns two kinds. Apps folder entries launch by
    // AppUserModelID; App Paths entries are bare executables and launch by
    // path. Every entry must be one or the other, never something in between.
    let (by_id, by_path): (Vec<_>, Vec<_>) = packaged
        .iter()
        .partition(|a| a.path.starts_with(sill_lib::apps::APPS_FOLDER));

    assert!(!by_id.is_empty(), "no Apps folder entries at all");
    assert!(
        by_path
            .iter()
            .all(|a| std::path::Path::new(&a.path).is_file()),
        "an App Paths entry must point at a file that exists"
    );

    eprintln!(
        "by AppUserModelID: {}, by path: {}",
        by_id.len(),
        by_path.len()
    );
}

#[test]
#[cfg(windows)]
fn app_paths_entries_do_not_duplicate_shortcuts() {
    // Deliberately NOT "no two entries share a target". Shortcuts routinely
    // point at the same host executable with different arguments, and those
    // are genuinely different commands: "Developer Command Prompt for VS 2022"
    // and "Node.js command prompt" both run cmd.exe and both belong in the
    // list. What must not happen is an App Paths entry, which is a bare
    // executable with no arguments, shadowing a shortcut that already runs it.
    let shortcuts = sill_lib::apps::scan_shortcuts();
    let all = sill_lib::apps::scan_all();

    assert!(!all.is_empty(), "the merged scan found nothing");
    assert!(
        all.len() >= shortcuts.len(),
        "merging must never lose entries: {} shortcuts became {}",
        shortcuts.len(),
        all.len()
    );

    let shortcut_targets: std::collections::HashSet<String> = shortcuts
        .iter()
        .filter_map(sill_lib::apps::target_key)
        .collect();

    // Anything in the merged list that is not a shortcut came from the second
    // scan, so it must bring a target no shortcut already covers.
    let shortcut_paths: std::collections::HashSet<&str> =
        shortcuts.iter().map(|a| a.path.as_str()).collect();

    let shadowing: Vec<&str> = all
        .iter()
        .filter(|a| !shortcut_paths.contains(a.path.as_str()))
        .filter(|a| sill_lib::apps::target_key(a).is_some_and(|t| shortcut_targets.contains(&t)))
        .map(|a| a.name.as_str())
        .collect();

    eprintln!("shortcuts {}, merged {}", shortcuts.len(), all.len());
    assert!(
        shadowing.is_empty(),
        "these duplicate a shortcut's executable: {:?}",
        &shadowing[..shadowing.len().min(8)]
    );
}

#[test]
#[cfg(windows)]
fn nothing_past_the_root_list_cap_becomes_unreachable() {
    // This started life asserting the root list returned at least 200 entries,
    // back when the limit had been set to 50 and was silently hiding most of
    // the index. That was the wrong invariant to freeze: the limit is now a
    // deliberate 120, because sending the whole index over IPC on every
    // keystroke cost half a megabyte to draw fifteen rows.
    //
    // What actually has to hold is not "the list is long". It is that **a cap
    // hides nothing**, because anything below it is still found by typing. So
    // that is what this checks, against the machine's real index.
    let all: Vec<_> = sill_lib::apps::scan_all()
        .iter()
        .map(|a| {
            sill_lib::registry::app_record(&a.name, &a.path, None, sill_lib::apps::categorize(a))
        })
        .collect();

    let limit = sill_lib::registry::SEARCH_LIMIT;
    let shown = search(&all, "", &Frecency::default(), NOW, limit);

    eprintln!("indexed {}, root list shows {}", all.len(), shown.len());

    assert_eq!(
        shown.len(),
        all.len().min(limit),
        "the empty root list should be exactly the cap, or everything if there is less"
    );
    assert!(
        limit >= 100,
        "a cap this small ({limit}) stops being a window and starts being a wall"
    );

    // Only titles that identify one entry: several vendors ship an "Uninstall"
    // and no query can be expected to pick between them.
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for record in &all {
        *seen.entry(record.title.as_str()).or_default() += 1;
    }

    // Sampled from **past the cap**, which is precisely the region the empty
    // root list can never show. Spread across the tail rather than taken from
    // its start, and sampled rather than exhaustive because this runs a fuzzy
    // match over the whole corpus per probe.
    let beyond: Vec<_> = all
        .iter()
        .skip(limit)
        .filter(|record| seen.get(record.title.as_str()) == Some(&1))
        .collect();

    if beyond.is_empty() {
        eprintln!("index is smaller than the cap; nothing is past it to check");
        return;
    }

    let step = (beyond.len() / 25).max(1);
    let mut checked = 0;

    for record in beyond.iter().step_by(step) {
        let results = search(&all, &record.title, &Frecency::default(), NOW, limit);
        assert!(
            results.iter().any(|r| r.command.id == record.id),
            "{:?} sits past the root list cap and typing its own name does not find it",
            record.title
        );
        checked += 1;
    }

    eprintln!(
        "{checked} of {} entries past the cap are all reachable by name",
        beyond.len()
    );
}

/// Reports the category spread across the real index. Diagnostic, run with
/// --nocapture.
#[test]
#[cfg(windows)]
fn report_category_distribution() {
    let all = sill_lib::apps::scan_all();
    let mut counts = std::collections::BTreeMap::<&str, usize>::new();

    for app in &all {
        *counts.entry(sill_lib::apps::categorize(app)).or_default() += 1;
    }

    eprintln!("{} entries", all.len());
    for (kind, n) in &counts {
        eprintln!("  {kind:<14} {n}");
    }
}

/// Icon coverage across everything in the index. Diagnostic; --nocapture.
#[test]
#[cfg(windows)]
fn report_icon_coverage() {
    let all = sill_lib::apps::scan_all();
    let settings = sill_lib::settings_catalog::load();

    // One cache for this probe. It used to reach for a process-wide one, which
    // is what rule 2 refuses and which meant no test could have its own.
    let icons = sill_lib::icons::Icons::new(None);

    let mut with = 0usize;
    let mut without = Vec::new();

    for app in &all {
        let source = app.icon_source.clone().unwrap_or_else(|| app.path.clone());
        match icons.data_uri(&source) {
            Some(_) => with += 1,
            None => without.push(app.name.as_str()),
        }
    }

    let settings_with = settings
        .iter()
        .filter(|s| {
            s.icon
                .as_deref()
                .and_then(|path| icons.data_uri(path))
                .is_some()
        })
        .count();

    eprintln!(
        "apps {}/{} have icons, settings {}/{}",
        with,
        all.len(),
        settings_with,
        settings.len()
    );
    eprintln!("missing: {:?}", &without[..without.len().min(15)]);
}
