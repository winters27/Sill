//! Ranking behaviour: what the root list shows, and what a query surfaces.
//!
//! This is the part of a launcher users judge hardest, so the tests are about
//! ordering outcomes rather than the arithmetic that produces them.

use sill_lib::registry::{self, search, Alias, Aliases, CommandRecord, Excluded, Frecency};

const NOW: i64 = 1_756_000_000;
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

fn command(id: &str, title: &str, extension_title: &str) -> CommandRecord {
    CommandRecord {
        id: id.to_string(),
        extension: id.split(':').next().unwrap_or(id).to_string(),
        extension_title: extension_title.to_string(),
        command: id.split(':').nth(1).unwrap_or("cmd").to_string(),
        title: title.to_string(),
        subtitle: String::new(),
        description: String::new(),
        mode: "view".to_string(),
        entrypoint: format!("C:/build/{id}.js"),
        keywords: Vec::new(),
        icon: None,
        panel: None,
        // Ranking never looks at these, so a fixture does not need any.
        preferences: serde_json::Value::Null,
    }
}

fn corpus() -> Vec<CommandRecord> {
    vec![
        command(
            "uuid-generator:viewHistory",
            "View History",
            "UUID Generator",
        ),
        command("uuid-generator:generate", "Generate UUID", "UUID Generator"),
        command(
            "password-generator:random",
            "Generate Random Password",
            "Password Generator",
        ),
        command("emoji:search", "Search Emoji", "Emoji"),
        command("clipboard:history", "Clipboard History", "Clipboard"),
    ]
}

fn titles(results: &[sill_lib::registry::RankedCommand]) -> Vec<&str> {
    results.iter().map(|r| r.command.title.as_str()).collect()
}

#[test]
fn initials_beat_scattered_letters() {
    let results = search(&corpus(), "vh", &Frecency::default(), NOW, 10);

    assert_eq!(
        titles(&results).first(),
        Some(&"View History"),
        "typing initials should find the command with those initials, got {:?}",
        titles(&results)
    );
}

#[test]
fn a_prefix_outranks_a_mid_word_match() {
    let results = search(&corpus(), "gen", &Frecency::default(), NOW, 10);
    let found = titles(&results);

    let generate = found.iter().position(|t| *t == "Generate UUID");
    assert!(
        generate.is_some(),
        "Generate UUID should match 'gen', got {found:?}"
    );

    // "Generate UUID" starts with the query; the others only contain it via
    // their extension title, so the direct hit must lead.
    assert_eq!(
        generate,
        Some(0),
        "the prefix match should lead, got {found:?}"
    );
}

#[test]
fn non_matches_are_excluded_entirely() {
    let results = search(&corpus(), "zzzz", &Frecency::default(), NOW, 10);
    assert!(
        results.is_empty(),
        "a query matching nothing returns nothing"
    );
}

#[test]
fn matched_indices_point_into_the_title() {
    let results = search(&corpus(), "vh", &Frecency::default(), NOW, 10);
    let top = &results[0];
    let chars: Vec<char> = top.command.title.chars().collect();

    let picked: String = top.matched.iter().map(|&i| chars[i]).collect();
    assert_eq!(
        picked.to_lowercase(),
        "vh",
        "highlight indices must select the queried characters, got {picked:?} from {:?}",
        top.command.title
    );
}

#[test]
fn empty_query_is_ordered_by_frecency() {
    let mut frecency = Frecency::default();
    frecency.record("emoji:search", NOW - HOUR);

    let results = search(&corpus(), "", &frecency, NOW, 10);

    assert_eq!(
        titles(&results).first(),
        Some(&"Search Emoji"),
        "the root list should lead with the recently used command, got {:?}",
        titles(&results)
    );
}

#[test]
fn recent_beats_merely_frequent() {
    let mut frecency = Frecency::default();

    // Used constantly, but not for a month.
    for _ in 0..30 {
        frecency.record("clipboard:history", NOW - DAY * 40);
    }
    // Used a few times, one of them just now.
    frecency.record("emoji:search", NOW - HOUR);

    let results = search(&corpus(), "", &frecency, NOW, 10);

    assert_eq!(
        titles(&results).first(),
        Some(&"Search Emoji"),
        "a stale habit should not outrank something used an hour ago, got {:?}",
        titles(&results)
    );
}

#[test]
fn frecency_breaks_ties_but_does_not_override_a_clear_match() {
    let mut frecency = Frecency::default();
    for _ in 0..20 {
        frecency.record("clipboard:history", NOW - 60);
    }

    // "emoji" names one command unambiguously; a heavily used unrelated
    // command must not displace it.
    let results = search(&corpus(), "emoji", &frecency, NOW, 10);

    assert_eq!(
        titles(&results).first(),
        Some(&"Search Emoji"),
        "an explicit query should beat frecency, got {:?}",
        titles(&results)
    );
}

#[test]
fn repeated_use_accumulates() {
    let mut frecency = Frecency::default();
    let once = {
        frecency.record("emoji:search", NOW);
        frecency.score("emoji:search", NOW)
    };

    for _ in 0..5 {
        frecency.record("emoji:search", NOW);
    }
    let many = frecency.score("emoji:search", NOW);

    assert!(
        many > once,
        "more launches should score higher: {many} vs {once}"
    );
}

#[test]
fn unknown_commands_score_zero() {
    let frecency = Frecency::default();
    assert_eq!(frecency.score("never:used", NOW), 0);
}

#[test]
fn the_limit_is_respected() {
    let results = search(&corpus(), "", &Frecency::default(), NOW, 2);
    assert_eq!(results.len(), 2, "the caller's limit bounds the result set");
}

/// Exercises the ranker against whatever is actually built, rather than only
/// a hand-written corpus. Skipped when nothing has been built yet.
#[test]
fn ranks_the_real_built_index() {
    let index = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("extensions")
        .join("build")
        .join("index.json");

    let commands = sill_lib::registry::load_index(&index);
    if commands.is_empty() {
        eprintln!("no built extensions; skipping");
        return;
    }

    let frecency = Frecency::default();

    // Every built command must be reachable by typing its own title.
    for command in &commands {
        let results = search(&commands, &command.title, &frecency, NOW, 50);
        assert_eq!(
            results.first().map(|r| r.command.id.as_str()),
            Some(command.id.as_str()),
            "typing a command's exact title should select it, got {:?}",
            results.first().map(|r| &r.command.title)
        );
    }

    // A partial query still has to land somewhere sensible.
    let results = search(&commands, "pass", &frecency, NOW, 50);
    assert!(
        results
            .iter()
            .all(|r| r.command.title.to_lowercase().contains("pass")
                || r.command.extension_title.to_lowercase().contains("pass")),
        "'pass' returned something unrelated: {:?}",
        results.iter().map(|r| &r.command.title).collect::<Vec<_>>()
    );
}

#[test]
fn applications_are_searchable_alongside_commands() {
    let mut corpus = corpus();
    corpus.push(sill_lib::registry::app_record(
        "Visual Studio Code",
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Visual Studio Code.lnk",
        None,
        "Application",
    ));
    corpus.push(sill_lib::registry::app_record(
        "Firefox",
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Firefox.lnk",
        None,
        "Application",
    ));

    let frecency = Frecency::default();

    // An app is found the same way a command is, through the same ranker.
    let results = search(&corpus, "code", &frecency, NOW, 10);
    assert_eq!(
        results.first().map(|r| r.command.title.as_str()),
        Some("Visual Studio Code"),
        "typing an app name should surface the app, got {:?}",
        titles(&results)
    );

    // Initials work on apps too, which is how they are usually reached.
    let results = search(&corpus, "vsc", &frecency, NOW, 10);
    assert_eq!(
        results.first().map(|r| r.command.title.as_str()),
        Some("Visual Studio Code"),
        "initials should reach the app, got {:?}",
        titles(&results)
    );

    let app = search(&corpus, "firefox", &frecency, NOW, 10);
    assert_eq!(app[0].command.mode, "app", "apps carry the app mode");
    assert!(
        app[0].command.entrypoint.ends_with(".lnk"),
        "an app launches through its shortcut, not a resolved target"
    );
}

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
fn installer_metadata_is_trimmed_from_names() {
    // Exercised through the public scan on Windows; the intent is documented
    // here so the conservative rule does not get loosened by accident.
    let cases = [
        ("7-Zip 26.02 (x64)", "7-Zip"),
        ("Python 3.13.2 (64-bit)", "Python"),
        ("Obsidian", "Obsidian"),
        // A trailing number without a dot is part of the name, not a version.
        ("PowerShell 7", "PowerShell 7"),
        ("Windows 11", "Windows 11"),
    ];

    for (input, want) in cases {
        assert_eq!(
            sill_lib::apps::tidy_name_for_test(input),
            want,
            "tidying {input:?}"
        );
    }
}

#[test]
fn a_real_application_outranks_a_path_executable() {
    // With roughly a thousand PATH executables against a couple of hundred
    // applications, an unweighted ranker lets a CLI tool win any short query.
    let corpus = vec![
        command("app:code", "Code", "Application"),
        sill_lib::registry::executable_record("code", r"C:\tools\code.exe", "Command Line"),
        sill_lib::registry::executable_record("codesign", r"C:\tools\codesign.exe", "Command Line"),
    ];

    let results = search(&corpus, "code", &Frecency::default(), NOW, 10);

    assert_eq!(
        results.first().map(|r| r.command.extension_title.as_str()),
        Some("Application"),
        "the application should lead, got {:?}",
        results
            .iter()
            .map(|r| (&r.command.title, &r.command.extension_title))
            .collect::<Vec<_>>()
    );

    // Still reachable, just not first.
    assert!(
        results.iter().any(|r| r.command.mode == "exe"),
        "PATH executables must remain searchable, not be excluded"
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

#[test]
#[cfg(windows)]
fn entries_are_categorised_by_where_they_resolve() {
    use sill_lib::apps::{categorize, AppRecord};

    let record = |name: &str, path: &str| AppRecord {
        name: name.to_string(),
        path: path.to_string(),
        icon_source: None,
    };

    // A management console is a system tool whatever it is called.
    assert_eq!(
        categorize(&record("Event Viewer", r"C:\Windows\System32\eventvwr.msc")),
        "System"
    );
    // Anything under the Windows directory is a Windows tool.
    assert_eq!(
        categorize(&record("Notepad", r"C:\Windows\System32\notepad.exe")),
        "System"
    );
    // A packaged app is identified by AppUserModelID, never by path.
    assert_eq!(
        categorize(&record(
            "Calculator",
            r"shell:AppsFolder\Microsoft.WindowsCalculator_8we!App"
        )),
        "Store App"
    );
    // A bookmark is not a program.
    assert_eq!(
        categorize(&record("Git FAQs", r"C:\Program Files\Git\faq.url")),
        "Web Link"
    );
    // Kept in the index rather than filtered out, but labelled honestly.
    assert_eq!(
        categorize(&record(
            "Node.js documentation",
            r"C:\Program Files\nodejs\doc.exe"
        )),
        "Documentation"
    );
    // An ordinary installed program.
    assert_eq!(
        categorize(&record(
            "Obsidian",
            r"C:\Users\x\AppData\Local\Obsidian\Obsidian.exe"
        )),
        "Application"
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

#[test]
fn windows_settings_are_searchable() {
    let settings = sill_lib::settings_catalog::load();
    assert!(
        settings.len() > 250,
        "the embedded settings catalog looks wrong: {} entries",
        settings.len()
    );

    let frecency = Frecency::default();

    // The catalog stores localisation keys, so this only works if they were
    // humanised into real words.
    let results = search(&settings, "bluetooth", &frecency, NOW, 20);
    assert!(
        !results.is_empty(),
        "typing a settings page name should find it"
    );

    // Section keywords make a page reachable by where it lives, not only by
    // its own one-word name.
    let proxy = search(&settings, "proxy", &frecency, NOW, 20);
    assert!(
        !proxy.is_empty(),
        "'proxy' should reach the proxy settings page"
    );

    // Every entry must know how to launch.
    assert!(
        settings.iter().all(|s| !s.entrypoint.is_empty()),
        "a settings entry with no command cannot be run"
    );

    let kinds: std::collections::BTreeSet<&str> = settings
        .iter()
        .map(|s| s.extension_title.as_str())
        .collect();
    eprintln!("{} settings across kinds {:?}", settings.len(), kinds);
}

/// Icon coverage across everything in the index. Diagnostic; --nocapture.
#[test]
#[cfg(windows)]
fn report_icon_coverage() {
    let all = sill_lib::apps::scan_all();
    let settings = sill_lib::settings_catalog::load();

    let mut with = 0usize;
    let mut without = Vec::new();

    for app in &all {
        let source = app.icon_source.clone().unwrap_or_else(|| app.path.clone());
        match sill_lib::icons::icon_data_uri(&source) {
            Some(_) => with += 1,
            None => without.push(app.name.as_str()),
        }
    }

    let settings_with = settings
        .iter()
        .filter(|s| {
            s.icon
                .as_deref()
                .and_then(sill_lib::icons::icon_data_uri)
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

#[test]
fn sill_settings_is_reachable_by_typing() {
    let commands = sill_lib::registry::builtins();
    let frecency = Frecency::default();

    // The whole point of a built-in: someone who does not know the shortcut
    // must be able to find it by typing what they want.
    for query in ["settings", "sill", "preferences", "hotkey", "options"] {
        let results = search(&commands, query, &frecency, NOW, 10);
        assert!(
            results.iter().any(|r| r.command.id == "sill:settings"),
            "typing {query:?} should reach Sill Settings, got {:?}",
            results.iter().map(|r| &r.command.title).collect::<Vec<_>>()
        );
    }

    assert!(
        commands.iter().all(|c| c.mode == "builtin"),
        "built-ins launch through Sill itself, not the shell"
    );
}

#[test]
fn an_exclusion_term_hides_matching_entries() {
    let commands = corpus();
    let frecency = Frecency::default();

    let before = sill_lib::registry::search_excluding(
        &commands,
        "",
        &frecency,
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
    );
    let after = sill_lib::registry::search_excluding(
        &commands,
        "",
        &frecency,
        &Aliases::default(),
        NOW,
        50,
        Excluded {
            terms: &["history".to_string()],
            ids: &[],
        },
    );

    assert!(
        before.iter().any(|r| r.command.title == "View History"),
        "the entry has to be there before it can be hidden"
    );
    assert!(
        !after
            .iter()
            .any(|r| r.command.title.to_lowercase().contains("history")),
        "an excluded title should not come back, got {:?}",
        after.iter().map(|r| &r.command.title).collect::<Vec<_>>()
    );
    assert!(
        after.iter().any(|r| r.command.title == "Search Emoji"),
        "a term must hide only what it matches, not clear the list"
    );
}

#[test]
fn an_exclusion_term_is_case_insensitive_and_ignores_blanks() {
    let commands = corpus();
    let frecency = Frecency::default();

    let results = sill_lib::registry::search_excluding(
        &commands,
        "",
        &frecency,
        &Aliases::default(),
        NOW,
        50,
        // A blank term matches every string. Left in, it would empty the list
        // the moment someone opened the editor and did not type.
        Excluded {
            terms: &["   ".to_string(), "HISTORY".to_string()],
            ids: &[],
        },
    );

    assert!(!results.is_empty(), "a blank term must not hide everything");
    assert!(
        !results.iter().any(|r| r.command.title == "View History"),
        "matching should ignore case"
    );
}

#[test]
fn searching_borrows_its_corpus_from_more_than_one_source() {
    // `search_commands` chains the index and the snippets rather than
    // collecting them. An earlier version cloned the whole index per
    // keystroke, which is thousands of string allocations for one keypress,
    // and a signature taking a slice is what forced that. Anything narrowing
    // it back to `&[CommandRecord]` fails to compile here.
    let index = corpus();
    let extra = vec![sill_lib::registry::snippet_record(
        "s1",
        "Signature",
        ";sig",
        "Best,\nBrandon",
    )];

    let results = sill_lib::registry::search_excluding(
        index.iter().chain(extra.iter()),
        "sig",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        20,
        Excluded::none(),
    );

    assert!(
        results.iter().any(|r| r.command.title == "Signature"),
        "a snippet from the second source should be searchable, got {:?}",
        titles(&results)
    );
}

#[test]
fn a_snippet_is_findable_by_what_is_inside_it() {
    // Half the point of searching snippets: you remember the text, not the
    // name you gave it.
    let extra = vec![sill_lib::registry::snippet_record(
        "s1",
        "Signature",
        ";sig",
        "Kind regards, Brandon Winters",
    )];

    let results = sill_lib::registry::search_excluding(
        extra.iter(),
        "regards",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        20,
        Excluded::none(),
    );

    assert_eq!(results.len(), 1, "content should be searchable");
}

// ------------------------------------------------------- ranking stability

use sill_lib::registry::{match_class, MatchClass};

/// A corpus with the collisions a real index has: several things that start
/// the same way, several that merely contain the letters.
fn crowded() -> Vec<CommandRecord> {
    vec![
        command("app:code", "Visual Studio Code", "Application"),
        command("app:codium", "VSCodium", "Application"),
        command("app:discord", "Discord", "Application"),
        command("app:docker", "Docker Desktop", "Application"),
        command("app:chrome", "Google Chrome", "Application"),
        command("app:calc", "Calculator", "Application"),
        command("app:screen", "Screen Capture", "Application"),
        command("app:notion", "Notion Calendar", "Application"),
    ]
}

fn ids(results: &[sill_lib::registry::RankedCommand]) -> Vec<String> {
    results.iter().map(|r| r.command.id.clone()).collect()
}

fn class_of(query: &str, corpus: &[CommandRecord], id: &str) -> Option<MatchClass> {
    corpus
        .iter()
        .find(|c| c.id == id)
        .and_then(|c| match_class(query, c))
}

#[test]
fn two_results_only_swap_when_one_of_them_matches_differently() {
    // The property the whole class model exists for, stated exactly: as a
    // query grows a character at a time, any two results that both still
    // match and whose kind of match has not changed must stay in the same
    // order relative to each other.
    //
    // Ordering by a fuzzy score fails this constantly. The score moves by a
    // point or two per keystroke, results trade places, and the row someone
    // was reaching for slides out from under their finger.
    //
    // Checked as pairwise inversions rather than by comparing positions:
    // one result dropping a class shifts every position below it without
    // any two of them having actually changed places.
    let corpus = realistic();
    let frecency = Frecency::default();

    for query in [
        "visual", "code", "discord", "docker", "google", "calendar", "notepad", "terminal",
        "steam", "photo", "micro", "settings", "git", "power", "explorer", "sn", "st", "ca",
    ] {
        let mut previous: Option<(String, Vec<String>)> = None;

        for length in 1..=query.len() {
            let now_query = &query[..length];
            let ranked = ids(&search(&corpus, now_query, &frecency, NOW, 50));

            if let Some((was_query, was)) = &previous {
                let rank = |list: &[String], id: &String| list.iter().position(|x| x == id);

                for a in &ranked {
                    for b in &ranked {
                        let (Some(a_then), Some(b_then)) = (rank(was, a), rank(was, b)) else {
                            continue;
                        };
                        let (Some(a_now), Some(b_now)) = (rank(&ranked, a), rank(&ranked, b))
                        else {
                            continue;
                        };

                        // Only look at pairs that actually changed places.
                        if !(a_then < b_then && a_now > b_now) {
                            continue;
                        }

                        let a_moved =
                            class_of(was_query, &corpus, a) != class_of(now_query, &corpus, a);
                        let b_moved =
                            class_of(was_query, &corpus, b) != class_of(now_query, &corpus, b);

                        assert!(
                            a_moved || b_moved,
                            "typing {was_query:?} then {now_query:?} swapped {a} past {b}                              without either changing how it matched"
                        );
                    }
                }
            }

            previous = Some((now_query.to_string(), ranked));
        }
    }
}

#[test]
fn a_better_kind_of_match_is_what_promotes_a_result() {
    // The other half: the model must not be stable by being inert. A result
    // whose match improves does move up.
    let corpus = crowded();
    let frecency = Frecency::default();

    // "c" lands on the C of Code, a word start, same as it does for
    // Calculator. "ca" is a prefix of Calculator and no longer a word-start
    // match for Visual Studio Code, so Calculator overtakes it.
    assert_eq!(
        class_of("ca", &corpus, "app:calc"),
        Some(MatchClass::TitleWord)
    );

    let after = ids(&search(&corpus, "ca", &frecency, NOW, 50));
    let calc = after.iter().position(|id| id == "app:calc");
    let code = after.iter().position(|id| id == "app:code");

    assert_eq!(calc, Some(0), "a prefix match should lead, got {after:?}");
    assert!(
        code.is_none() || code > calc,
        "Calculator should outrank Visual Studio Code for 'ca', got {after:?}"
    );
}

#[test]
fn initials_beat_a_run_of_the_same_letters() {
    // `gc` is someone typing initials. Google Chrome is what they meant;
    // Logcat Viewer merely contains a g and a c in a row.
    let mut corpus = crowded();
    corpus.push(command("app:logcat", "Logcat Viewer", "Application"));

    assert_eq!(
        class_of("gc", &corpus, "app:chrome"),
        Some(MatchClass::TitleWordStarts)
    );
    assert_eq!(
        class_of("gc", &corpus, "app:logcat"),
        Some(MatchClass::TitleSubstring)
    );

    let found = ids(&search(&corpus, "gc", &Frecency::default(), NOW, 50));
    assert_eq!(
        found.first().map(String::as_str),
        Some("app:chrome"),
        "got {found:?}"
    );
}

#[test]
fn the_kinds_of_match_are_ordered_best_first() {
    // The sort relies on the derived Ord, so the declaration order in the
    // enum is load-bearing rather than cosmetic.
    assert!(MatchClass::ExactTitle < MatchClass::TitleWord);
    assert!(MatchClass::TitleWord < MatchClass::TitleWordStarts);
    assert!(MatchClass::TitleWordStarts < MatchClass::TitleSubstring);
    assert!(MatchClass::TitleSubstring < MatchClass::TitleSubsequence);
    assert!(MatchClass::TitleSubsequence < MatchClass::Elsewhere);
}

#[test]
fn a_match_only_in_a_keyword_ranks_below_any_match_in_the_name() {
    // Keywords make a command findable; they do not make it the answer.
    let mut with_keyword = command("ext:tool", "Unrelated Name", "Extension");
    with_keyword.keywords = vec!["docker".to_string()];

    let corpus = vec![
        command("app:docker", "Docker Desktop", "Application"),
        with_keyword,
    ];
    let found = ids(&search(&corpus, "docker", &Frecency::default(), NOW, 50));

    assert_eq!(
        found,
        vec!["app:docker".to_string(), "ext:tool".to_string()],
        "the command actually called Docker should lead"
    );
}

/// Names with the shapes a real Start Menu has: shared prefixes, shared
/// initials, vendor prefixes, and long titles that contain short ones.
fn realistic() -> Vec<CommandRecord> {
    [
        "Visual Studio Code",
        "Visual Studio 2022",
        "VSCodium",
        "Discord",
        "Docker Desktop",
        "Google Chrome",
        "Google Drive",
        "Calculator",
        "Calendar",
        "Screen Capture",
        "Notion Calendar",
        "Notepad",
        "Notepad++",
        "Windows Terminal",
        "Terminal Preview",
        "Steam",
        "Steam Link",
        "Spotify",
        "Photos",
        "Photoshop",
        "File Explorer",
        "Internet Explorer",
        "PowerShell 7",
        "Windows PowerShell ISE",
        "Command Prompt",
        "Task Manager",
        "Device Manager",
        "Disk Cleanup",
        "Snipping Tool",
        "Sticky Notes",
        "Sound Recorder",
        "Settings",
        "System Settings",
        "Slack",
        "Signal",
        "Skype",
        "Zoom Workplace",
        "OBS Studio",
        "Audacity",
        "Blender",
        "GIMP",
        "Inkscape",
        "Firefox Developer Edition",
        "Microsoft Edge",
        "Microsoft Teams",
        "Microsoft Word",
        "Microsoft Excel",
        "Postman",
        "PostgreSQL",
        "DB Browser for SQLite",
        "Docker Compose",
        "Git Bash",
        "GitHub Desktop",
    ]
    .iter()
    .enumerate()
    .map(|(i, title)| command(&format!("app:{i}"), title, "Application"))
    .collect()
}

// ------------------------------------------------------------ typo tolerance

#[test]
fn a_transposed_pair_still_finds_what_was_meant() {
    // The typo people actually make. Plain Levenshtein calls this two edits
    // and would miss it at any budget tight enough to be useful, which is why
    // the distance counts an adjacent swap as one.
    let corpus = realistic();

    for (typed, wanted) in [
        ("chorme", "Google Chrome"),
        ("dsicord", "Discord"),
        ("sptoify", "Spotify"),
        ("caluclator", "Calculator"),
    ] {
        let found = search(&corpus, typed, &Frecency::default(), NOW, 50);
        let titles: Vec<&str> = found.iter().map(|r| r.command.title.as_str()).collect();
        assert!(
            titles.contains(&wanted),
            "typing {typed:?} should still reach {wanted:?}, got {titles:?}"
        );
    }
}

#[test]
fn a_guess_never_outranks_something_that_actually_matched() {
    // Typo matching is a last resort, not a competitor. If anything matched
    // for real, it leads and the near-misses follow.
    let corpus = realistic();
    let found = ids(&search(&corpus, "notepad", &Frecency::default(), NOW, 50));

    let real = found
        .iter()
        .position(|id| class_of("notepad", &corpus, id) != Some(MatchClass::TitleTypo));
    let guess = found
        .iter()
        .position(|id| class_of("notepad", &corpus, id) == Some(MatchClass::TitleTypo));

    if let (Some(real), Some(guess)) = (real, guess) {
        assert!(
            real < guess,
            "a guess was offered above a real match: {found:?}"
        );
    }

    assert_eq!(
        class_of("notepad", &corpus, "app:11"),
        Some(MatchClass::ExactTitle),
        "Notepad should match itself exactly"
    );
}

#[test]
fn short_queries_are_not_forgiven_anything() {
    // At three characters a budget of one matches an enormous share of any
    // index, and the list fills with things nobody asked for. The first
    // keystrokes are also where a launcher is judged.
    let corpus = realistic();

    for typed in ["zzz", "qq", "x"] {
        let found = ids(&search(&corpus, typed, &Frecency::default(), NOW, 50));
        let guesses: Vec<&String> = found
            .iter()
            .filter(|id| class_of(typed, &corpus, id) == Some(MatchClass::TitleTypo))
            .collect();

        assert!(
            guesses.is_empty(),
            "{typed:?} produced typo matches: {guesses:?}"
        );
    }
}

#[test]
fn a_word_of_a_longer_title_is_what_gets_compared() {
    // Comparing against the whole title would never find this: "chorme" is
    // nowhere near "Google Chrome" as a string, and very near one word of it.
    assert_eq!(
        class_of("chorme", &realistic(), "app:5"),
        Some(MatchClass::TitleTypo)
    );
}

#[test]
fn a_guess_is_the_last_thing_offered() {
    // The sort relies on the derived Ord, so where this sits in the enum is
    // what keeps a near-miss below every real match including a keyword hit.
    assert!(MatchClass::Elsewhere < MatchClass::TitleTypo);
    assert!(MatchClass::TitleSubsequence < MatchClass::TitleTypo);
}

// ---------------------------------------------------------------- aliases

/// The alias target, chosen so its own title cannot match the alias.
///
/// "music" appears in no title in the corpus, and Spotify contains no `m` at
/// all, so nothing on the way to typing it can reach Spotify by accident
/// either. Both properties matter: without the second, the partial-typing
/// test would pass for the wrong reason.
const TARGET: &str = "Spotify";
const ALIAS: &str = "music";

fn aliased() -> (Vec<CommandRecord>, Aliases) {
    let commands = realistic();
    let target = commands
        .iter()
        .find(|c| c.title == TARGET)
        .unwrap_or_else(|| panic!("the corpus has {TARGET}"));

    let aliases = Aliases::new(&[Alias {
        alias: ALIAS.into(),
        command: target.id.clone(),
    }]);

    (commands, aliases)
}

#[test]
fn an_alias_finds_something_its_name_does_not_contain() {
    // The point of an alias. "notes" appears nowhere in "Obsidian", so
    // without one there is no query that reaches it by that word at all.
    let (commands, aliases) = aliased();

    let found = registry::search_excluding(
        commands.iter(),
        ALIAS,
        &Frecency::default(),
        &aliases,
        NOW,
        50,
        Excluded::none(),
    );

    assert_eq!(
        found.first().map(|r| r.command.title.as_str()),
        Some(TARGET),
        "an alias has to reach a title that does not contain it"
    );
}

#[test]
fn an_alias_outranks_an_exact_title_match() {
    // The hard case, and the reason Alias is above ExactTitle rather than
    // beside it. An alias is the one ranking input that is not a guess:
    // somebody typed it in and said what those letters mean. A model that can
    // overrule it has turned an instruction into a suggestion.
    let mut commands = realistic();

    // Something whose title IS the alias, which would otherwise win outright.
    commands.push(command("decoy:music", "Music", "Decoy"));

    let target = commands
        .iter()
        .find(|c| c.title == TARGET)
        .expect("in the corpus")
        .id
        .clone();

    let aliases = Aliases::new(&[Alias {
        alias: ALIAS.into(),
        command: target,
    }]);

    let found = registry::search_excluding(
        commands.iter(),
        ALIAS,
        &Frecency::default(),
        &aliases,
        NOW,
        50,
        Excluded::none(),
    );

    assert_eq!(
        found.first().map(|r| r.command.title.as_str()),
        Some(TARGET),
        "the alias lost to a title that happened to match"
    );

    // And the decoy is still findable, immediately below.
    assert!(
        found.iter().any(|r| r.command.title == "Music"),
        "the alias must not hide anything"
    );
}

#[test]
fn an_alias_only_applies_when_it_is_typed_in_full() {
    // Partway through typing, ordinary matching applies. An alias that pulled
    // its target to the top from the first letter would make every result
    // list unpredictable while typing anything that starts the same way.
    let (commands, aliases) = aliased();

    for partial in ["m", "mu", "mus", "musi"] {
        let found = registry::search_excluding(
            commands.iter(),
            partial,
            &Frecency::default(),
            &aliases,
            NOW,
            50,
            Excluded {
                terms: &[],
                ids: &[],
            },
        );

        assert_ne!(
            found.first().map(|r| r.command.title.as_str()),
            Some(TARGET),
            "{partial:?} pulled the alias target up before it was finished"
        );
    }
}

#[test]
fn an_alias_is_matched_regardless_of_case() {
    let (commands, _) = aliased();
    let target = commands
        .iter()
        .find(|c| c.title == TARGET)
        .expect("in the corpus")
        .id
        .clone();

    // Stored with capitals and padding, which the store cleans on the way in.
    let aliases = Aliases::new(&[Alias {
        alias: "  MUSIC  ".into(),
        command: target,
    }]);

    for typed in ["music", "MUSIC", "Music"] {
        let found = registry::search_excluding(
            commands.iter(),
            typed,
            &Frecency::default(),
            &aliases,
            NOW,
            50,
            Excluded {
                terms: &[],
                ids: &[],
            },
        );

        assert_eq!(
            found.first().map(|r| r.command.title.as_str()),
            Some(TARGET),
            "{typed:?} did not match the alias"
        );
    }
}

#[test]
fn an_alias_pointing_at_something_that_is_gone_changes_nothing() {
    // An application is uninstalled and the alias outlives it. The alias
    // simply never matches; it must not make the search fail or hide results.
    let commands = realistic();
    let aliases = Aliases::new(&[Alias {
        alias: ALIAS.into(),
        command: "app:C:/gone/forever.exe".into(),
    }]);

    let with = registry::search_excluding(
        commands.iter(),
        "disc",
        &Frecency::default(),
        &aliases,
        NOW,
        50,
        Excluded::none(),
    );
    let without = registry::search_excluding(
        commands.iter(),
        "disc",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
    );

    let titles = |r: &[registry::RankedCommand]| {
        r.iter()
            .map(|c| c.command.title.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&with), titles(&without));
}

#[test]
fn a_blank_alias_is_not_an_alias_that_matches_everything() {
    // An empty string is a prefix of every query. Storing one and comparing
    // it naively would make one command win every search ever typed.
    let commands = realistic();
    let aliases = Aliases::new(&[
        Alias {
            alias: "   ".into(),
            command: commands[0].id.clone(),
        },
        Alias {
            alias: "".into(),
            command: commands[1].id.clone(),
        },
    ]);

    assert!(aliases.is_empty(), "blank aliases were kept");

    // And the ordering is exactly what it would be with no aliases at all,
    // rather than merely "not obviously wrong".
    let with = registry::search_excluding(
        commands.iter(),
        "docker",
        &Frecency::default(),
        &aliases,
        NOW,
        50,
        Excluded::none(),
    );
    let without = registry::search_excluding(
        commands.iter(),
        "docker",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
    );

    let titles = |r: &[registry::RankedCommand]| {
        r.iter()
            .map(|c| c.command.title.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(titles(&with), titles(&without));
    assert!(
        !titles(&with).is_empty(),
        "the query matched nothing at all"
    );
}

// --------------------------------------------------------------- learning

/// A frecency store that has already seen `query` reach `title` `times` over.
fn taught(commands: &[CommandRecord], query: &str, title: &str, times: u32) -> Frecency {
    let id = commands
        .iter()
        .find(|c| c.title == title)
        .unwrap_or_else(|| panic!("the corpus has {title}"))
        .id
        .clone();

    let mut frecency = Frecency::default();
    for _ in 0..times {
        frecency.record_query(query, &id, NOW);
    }
    frecency
}

fn ranked(commands: &[CommandRecord], query: &str, frecency: &Frecency) -> Vec<String> {
    registry::search_excluding(
        commands.iter(),
        query,
        frecency,
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
    )
    .into_iter()
    .map(|r| r.command.title)
    .collect()
}

#[test]
fn choosing_the_same_thing_twice_for_a_query_promotes_it_next_time() {
    // The whole point, and the case measured on this machine: typing
    // "notepad" reaches several things, and whichever one you keep choosing
    // should stop being the one you have to arrow down to.
    let commands = realistic();

    // Notepad++ is not what "notepad" ranks first on its own.
    let cold = ranked(&commands, "notepad", &Frecency::default());
    assert_ne!(cold.first().map(String::as_str), Some("Notepad++"));

    let learned = taught(&commands, "notepad", "Notepad++", 2);
    let warm = ranked(&commands, "notepad", &learned);

    assert_eq!(
        warm.first().map(String::as_str),
        Some("Notepad++"),
        "two choices did not teach it"
    );
}

#[test]
fn choosing_it_once_is_not_enough_to_reorder_anything() {
    // Once is a keystroke that could have been a mistake. A single stray
    // Enter must not silently reorder a list for good.
    let commands = realistic();

    let once = taught(&commands, "notepad", "Notepad++", 1);
    assert_eq!(
        ranked(&commands, "notepad", &once),
        ranked(&commands, "notepad", &Frecency::default()),
        "one choice changed the order"
    );
}

#[test]
fn learning_never_invents_a_match_that_was_not_there() {
    // The rule that keeps this explainable. A learned pair promotes something
    // the query would have found anyway; it must not conjure an unrelated
    // result out of fifteen hundred entries because the letters were typed
    // near it once. Otherwise a stray Enter puts Spotify at the top of
    // "docker" and nothing on screen explains why.
    let commands = realistic();
    let learned = taught(&commands, "docker", "Spotify", 5);

    let found = ranked(&commands, "docker", &learned);

    assert!(
        !found.iter().any(|t| t == "Spotify"),
        "learning dragged in something the query does not match: {found:?}"
    );
    assert!(
        found.iter().any(|t| t == "Docker Desktop"),
        "and it lost the results that do match"
    );
}

#[test]
fn what_was_learned_for_one_query_stays_there() {
    // Learning is per query. Teaching "notepad" must not change "note", which
    // is a different thing the user might mean differently.
    let commands = realistic();
    let learned = taught(&commands, "notepad", "Notepad++", 3);

    assert_eq!(
        ranked(&commands, "note", &learned),
        ranked(&commands, "note", &Frecency::default()),
        "teaching one query leaked into another"
    );
}

#[test]
fn a_query_is_remembered_the_same_however_it_was_typed() {
    let commands = realistic();
    let id = commands
        .iter()
        .find(|c| c.title == "Notepad++")
        .expect("in the corpus")
        .id
        .clone();

    let mut frecency = Frecency::default();
    frecency.record_query("  NoTePaD  ", &id, NOW);
    frecency.record_query("notepad", &id, NOW);

    assert_eq!(
        ranked(&commands, "NOTEPAD", &frecency)
            .first()
            .map(String::as_str),
        Some("Notepad++"),
        "case and padding split one habit into two"
    );
}

#[test]
fn a_long_query_teaches_nothing() {
    // Somebody who typed the whole name has already found the thing.
    // Remembering it grows the file with one entry per long search and
    // teaches nothing, since the next identical search finds it anyway.
    let commands = realistic();
    let id = commands[0].id.clone();

    let mut frecency = Frecency::default();
    for _ in 0..5 {
        frecency.record_query(
            "an extremely long thing nobody would use as shorthand",
            &id,
            NOW,
        );
    }

    assert_eq!(frecency.learned_len(), 0);
}

#[test]
fn an_empty_query_teaches_nothing() {
    // The root list. Everything matched equally, so choosing something says
    // nothing about what any letters mean.
    let commands = realistic();
    let id = commands[0].id.clone();

    let mut frecency = Frecency::default();
    frecency.record_query("", &id, NOW);
    frecency.record_query("   ", &id, NOW);

    assert_eq!(frecency.learned_len(), 0);
}

#[test]
fn what_is_remembered_is_bounded_and_drops_the_oldest_first() {
    // It is written to disk and read on every launch. An unbounded map of
    // everything ever typed is the quiet growth rule 23 exists to stop.
    let mut frecency = Frecency::default();

    // Old, and first in.
    frecency.record_query("oldest", "app:a", NOW - 100_000);

    for n in 0..600 {
        frecency.record_query(&format!("q{n}"), "app:b", NOW);
    }

    assert!(
        frecency.learned_len() <= 400,
        "grew to {}",
        frecency.learned_len()
    );
    assert!(
        frecency.learned_for("oldest").is_none(),
        "the oldest query survived the trim"
    );
    assert!(
        frecency.learned_for("q599").is_some(),
        "the newest query was dropped instead"
    );
}

#[test]
fn an_alias_still_beats_something_learned() {
    // Stated beats inferred. An alias is an instruction and learning is
    // evidence, however strong the evidence has got.
    let commands = realistic();

    let spotify = commands
        .iter()
        .find(|c| c.title == "Spotify")
        .expect("in the corpus")
        .id
        .clone();

    // "steam" is taught hard towards Steam Link...
    let learned = taught(&commands, "steam", "Steam Link", 9);
    // ...but the user has said outright that "steam" means Spotify.
    let aliases = Aliases::new(&[Alias {
        alias: "steam".into(),
        command: spotify,
    }]);

    let found = registry::search_excluding(
        commands.iter(),
        "steam",
        &learned,
        &aliases,
        NOW,
        50,
        Excluded::none(),
    );

    assert_eq!(
        found.first().map(|r| r.command.title.as_str()),
        Some("Spotify"),
        "learning overruled an instruction"
    );
}

// -------------------------------------------------------- switched off

#[test]
fn hiding_one_entry_hides_exactly_that_one() {
    // The reason this is not the term list. "Notepad" as a term would take
    // Notepad++ with it; hiding by id has to mean this one and nothing else.
    let commands = realistic();
    let notepad = commands
        .iter()
        .find(|c| c.title == "Notepad")
        .expect("in the corpus")
        .id
        .clone();

    let found = registry::search_excluding(
        commands.iter(),
        "notepad",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded {
            terms: &[],
            ids: &[notepad],
        },
    );

    let titles: Vec<&str> = found.iter().map(|r| r.command.title.as_str()).collect();
    assert!(!titles.contains(&"Notepad"), "it is still listed");
    assert!(
        titles.contains(&"Notepad++"),
        "hiding one took its namesake with it: {titles:?}"
    );
}

#[test]
fn hiding_by_id_never_matches_a_title_that_merely_contains_it() {
    // Ids are paths. If they were compared loosely, hiding one executable
    // would hide every entry under the same directory.
    let commands = realistic();

    let found = registry::search_excluding(
        commands.iter(),
        "",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        2000,
        Excluded {
            terms: &[],
            // A prefix of many real ids in the corpus.
            ids: &["app:".to_string()],
        },
    );

    assert_eq!(
        found.len(),
        commands.len(),
        "a partial id hid entries it does not name"
    );
}

#[test]
fn the_term_list_and_the_hidden_list_both_apply() {
    // Two different tools for two different jobs, and neither replaces the
    // other: a term for "nothing from this vendor", an id for "not this one".
    let commands = realistic();
    let steam = commands
        .iter()
        .find(|c| c.title == "Steam")
        .expect("in the corpus")
        .id
        .clone();

    let found = registry::search_excluding(
        commands.iter(),
        "",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        2000,
        Excluded {
            terms: &["notepad".to_string()],
            ids: &[steam],
        },
    );

    let titles: Vec<&str> = found.iter().map(|r| r.command.title.as_str()).collect();
    assert!(
        !titles.iter().any(|t| t.starts_with("Notepad")),
        "{titles:?}"
    );
    assert!(!titles.contains(&"Steam"), "{titles:?}");
    assert!(titles.contains(&"Steam Link"), "the term-free one went too");
}

// --------------------------------------------------------------- history

#[test]
fn the_most_recent_query_is_first() {
    let mut frecency = Frecency::default();
    frecency.remember("docker");
    frecency.remember("spotify");
    frecency.remember("notepad");

    assert_eq!(frecency.history(), ["notepad", "spotify", "docker"]);
}

#[test]
fn searching_for_the_same_thing_twice_moves_it_rather_than_repeating_it() {
    // Both halves matter. Refusing the duplicate leaves it buried under
    // everything done since, and keeping both fills the history with one
    // query typed over and over.
    let mut frecency = Frecency::default();
    frecency.remember("docker");
    frecency.remember("spotify");
    frecency.remember("docker");

    assert_eq!(frecency.history(), ["docker", "spotify"]);
}

#[test]
fn the_history_is_bounded() {
    // It is written to disk and read on every launch, and walking back
    // through hundreds of entries is slower than retyping.
    let mut frecency = Frecency::default();
    for n in 0..200 {
        frecency.remember(&format!("query {n}"));
    }

    assert!(
        frecency.history().len() <= 50,
        "{}",
        frecency.history().len()
    );
    assert_eq!(
        frecency.history().first().map(String::as_str),
        Some("query 199")
    );
}

#[test]
fn an_empty_query_is_not_remembered() {
    // Summoning and pressing Enter on the root list is not a search, and
    // offering it back later would be offering nothing back.
    let mut frecency = Frecency::default();
    frecency.remember("");
    frecency.remember("   ");

    assert!(frecency.history().is_empty());
}

#[test]
fn a_long_query_is_remembered_even_though_it_teaches_nothing() {
    // The two are different questions. Learning ignores a long query because
    // whoever typed the whole name had already found the thing; history keeps
    // it because getting it back without retyping is the entire point.
    let mut frecency = Frecency::default();
    let long = "a very long thing somebody actually searched for once";

    frecency.record_query(long, "app:x", NOW);
    frecency.remember(long);

    assert_eq!(frecency.learned_len(), 0, "it should teach nothing");
    assert_eq!(frecency.history(), [long], "but it should be recallable");
}

// --------------------------------------------------------- exact keywords

#[test]
fn an_exact_keyword_beats_letters_found_scattered_in_a_title() {
    // Measured, not preferred. Searching emoji for "tada" returned the trade
    // mark sign: t, a, d, a really are in "trade mark" in that order, so it
    // matched as a subsequence, while the party popper only matched on its
    // shortcode. A whole word somebody declared as another name for the thing
    // is better evidence than four letters scattered through a longer one.
    let scattered = command("x:trade", "trade mark", "Symbols");
    let mut declared = command("x:party", "party popper", "Activities");
    declared.keywords = vec!["tada".to_string()];

    let commands = vec![scattered, declared];

    let found = registry::search_excluding(
        commands.iter(),
        "tada",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
    );

    assert_eq!(
        found.first().map(|r| r.command.title.as_str()),
        Some("party popper"),
        "the scattered match won"
    );
    assert_eq!(found.len(), 2, "and the other is still findable");
}

#[test]
fn a_keyword_has_to_be_the_whole_query_to_count_as_exact() {
    // "mail" is not an exact match for the keyword "email". Treating a partial
    // one as exact would promote almost everything, since keywords are short
    // and there are many of them.
    let mut thing = command("x:thing", "Some Thing", "Test");
    thing.keywords = vec!["email".to_string()];

    assert_eq!(
        match_class("mail", &thing),
        Some(MatchClass::Elsewhere),
        "a partial keyword was treated as exact"
    );
    assert_eq!(match_class("email", &thing), Some(MatchClass::KeywordExact));
}

#[test]
fn an_exact_keyword_does_not_outrank_the_title_itself() {
    // It is better evidence than a scattered subsequence, not better than
    // somebody typing the name. A prefix of the real title still wins.
    let mut aliased = command("x:other", "Something Else", "Test");
    aliased.keywords = vec!["note".to_string()];
    let named = command("x:notes", "Notes", "Test");

    let commands = vec![aliased, named];

    let found = registry::search_excluding(
        commands.iter(),
        "note",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
    );

    assert_eq!(
        found.first().map(|r| r.command.title.as_str()),
        Some("Notes"),
        "a keyword outranked the actual name"
    );
}

#[test]
fn a_keyword_written_with_capitals_still_matches() {
    // Keywords are written by hand in the index and in extension manifests,
    // so they are not reliably lowercase.
    let mut thing = command("x:thing", "Some Thing", "Test");
    thing.keywords = vec!["Screenshot".to_string()];

    assert_eq!(
        match_class("screenshot", &thing),
        Some(MatchClass::KeywordExact)
    );
}

// ------------------------------------------------- volunteering results

#[test]
fn only_a_named_match_is_strong_enough_to_volunteer() {
    // Emoji append themselves to an ordinary search, so they have to earn the
    // room. Their names are ordinary words and there are nearly two thousand
    // of them, so anything looser than "the user named this" would put a
    // smiley in the middle of every search anybody ever typed.
    use registry::MatchClass;

    for strong in [
        MatchClass::Alias,
        MatchClass::Learned,
        MatchClass::ExactTitle,
        MatchClass::TitleWord,
        MatchClass::TitleWordStarts,
        MatchClass::KeywordExact,
    ] {
        assert!(registry::is_strong(strong), "{strong:?} should volunteer");
    }

    for weak in [
        MatchClass::TitleSubstring,
        MatchClass::TitleSubsequence,
        MatchClass::Elsewhere,
        MatchClass::TitleTypo,
    ] {
        assert!(!registry::is_strong(weak), "{weak:?} should not volunteer");
    }
}

#[test]
fn a_scattered_match_on_an_emoji_name_does_not_volunteer() {
    // The case this exists for. "code" finds "chart decreasing" as a
    // subsequence: c, o... no, but "cloud" does, and so do dozens of others
    // for almost any query. Checked through the real classifier rather than
    // by listing classes.
    let mut emoji = command("emoji:x", "chart increasing", "Symbols");
    emoji.keywords = vec!["chart_increasing".to_string()];

    let class = match_class("cri", &emoji).expect("it does match somehow");

    assert_eq!(class, MatchClass::TitleSubsequence);
    assert!(
        !registry::is_strong(class),
        "a scattered match volunteered itself"
    );
}

#[test]
fn typing_the_name_of_an_emoji_does_volunteer_it() {
    let mut emoji = command("emoji:rocket", "rocket", "Travel and Places");
    emoji.keywords = vec!["rocket".to_string()];

    for typed in ["rocket", "rock", "ROCKET"] {
        let class = match_class(typed, &emoji).expect("matches");
        assert!(
            registry::is_strong(class),
            "{typed:?} gave {class:?}, which will not be offered"
        );
    }
}

/// What a query actually pulls out of the real emoji set.
///
/// The whole set, not a fixture: the question is whether nearly two thousand
/// ordinary English words quietly match everything, and only the real names
/// answer that.
fn volunteered(query: &str) -> Vec<String> {
    let frecency = Frecency::default();
    let aliases = Aliases::default();
    let records = sill_lib::emoji::records(sill_lib::emoji::Tone::Default);

    registry::search_excluding(
        records.iter(),
        query,
        &frecency,
        &aliases,
        NOW,
        registry::SEARCH_LIMIT,
        Excluded::none(),
    )
    .into_iter()
    .filter(|ranked| match_class(query, &ranked.command).is_some_and(registry::is_strong))
    .map(|ranked| ranked.command.title.clone())
    .collect()
}

#[test]
fn launching_something_does_not_drag_in_emoji() {
    // The measurement this rule exists for. These are what a person types to
    // launch a program, and not one of them is asking for a picture. Measured
    // against the real set of nearly two thousand: every one comes back empty.
    for typed in [
        "code", "chrome", "term", "settings", "explorer", "obs", "python",
        "git", "docker", "slack", "word", "excel", "calc", "edge", "zoom",
        "teams", "photo", "spot",
    ] {
        let found = volunteered(typed);
        assert!(found.is_empty(), "{typed:?} dragged in {found:?}");
    }
}

#[test]
fn a_word_that_names_an_emoji_offers_a_few_and_not_a_screenful() {
    // Some words genuinely are the name of an emoji as well as of a program.
    // Those do offer some, which is the point, and the number is what decides
    // whether it reads as a helpful aside or as the list being taken over.
    //
    // "heart" is here as the worst case in the whole set: thirty-five emoji
    // are named with that word, and the caller shows four of them.
    for (typed, most) in [("file", 4), ("mail", 5), ("note", 5), ("steam", 6)] {
        let found = volunteered(typed);
        assert!(
            !found.is_empty() && found.len() <= most,
            "{typed:?} offered {}: {found:?}",
            found.len()
        );
    }
}

#[test]
fn asking_for_an_emoji_by_name_still_finds_it() {
    // The other half. Tightening the rule until nothing gets through would
    // pass the test above and make the feature useless.
    for (typed, wanted) in [
        ("rocket", "rocket"),
        // The shortcode rather than the name, which is what people type.
        ("tada", "party popper"),
        ("thumbs", "thumbs up"),
        ("fire", "fire"),
        ("cat", "cat"),
        ("check", "check mark button"),
        ("party", "party popper"),
        ("100", "hundred points"),
        // Named by a word that is not the first one. This is the case the
        // whole-word class was added for: before it, "red heart" was in the
        // same bucket as an accident of spelling and was never offered.
        ("heart", "red heart"),
        ("moon", "full moon"),
        ("hand", "raised hand"),
    ] {
        let found = volunteered(typed);
        assert!(
            found.iter().any(|title| title == wanted),
            "{typed:?} did not offer {wanted:?}, only {:?}",
            found.iter().take(6).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_result_says_whether_the_query_named_it() {
    // The window merges two searches into one list and needs this to place
    // them. Measured through the real conversion rather than the classifier,
    // because it is the serialised value that decides what the window sees.
    let corpus = vec![
        command("app:chrome", "Google Chrome", "Application"),
        command("app:logcat", "Logcat Viewer", "Application"),
    ];

    let found = search(&corpus, "chrome", &Frecency::default(), NOW, 50);
    let named: Vec<registry::SearchResult> = found.into_iter().map(Into::into).collect();
    assert!(named[0].strong, "typing the name did not count as naming it");

    // Two letters that happen to sit next to each other in a longer word.
    let found = search(&corpus, "gc", &Frecency::default(), NOW, 50);
    let loose: Vec<registry::SearchResult> = found.into_iter().map(Into::into).collect();
    let logcat = loose
        .iter()
        .find(|r| r.id == "app:logcat")
        .expect("still a result");
    assert!(!logcat.strong, "a run of letters counted as naming it");
}

#[test]
fn the_flag_is_left_out_of_the_payload_when_it_is_false() {
    // Serialised on every keystroke, and most results in a long list are not
    // strong. Sending "strong":false sixty times a keystroke is the kind of
    // thing that made the payload half a megabyte in the first place.
    let corpus = vec![command("app:logcat", "Logcat Viewer", "Application")];
    let found = search(&corpus, "gc", &Frecency::default(), NOW, 50);
    let result: registry::SearchResult = found.into_iter().next().expect("a result").into();

    let wire = serde_json::to_string(&result).expect("serialises");
    assert!(!wire.contains("strong"), "{wire}");

    let corpus = vec![command("app:logcat", "Logcat Viewer", "Application")];
    let found = search(&corpus, "logcat", &Frecency::default(), NOW, 50);
    let result: registry::SearchResult = found.into_iter().next().expect("a result").into();
    let wire = serde_json::to_string(&result).expect("serialises");
    assert!(wire.contains("\"strong\":true"), "{wire}");
}

#[test]
fn a_query_the_index_only_half_recognises_has_no_strong_results() {
    // The case that made the merge necessary. "tada" is not the name of
    // anything a person installs, so nothing in an index should claim it, and
    // the emoji somebody plainly meant has to be able to get above the noise.
    let corpus = vec![
        command("sill:dictation", "Dictation and transcripts", "Sill Settings"),
        command("app:notepad", "Notepad", "Application"),
        command("app:task", "Task Manager", "Application"),
    ];

    let found = search(&corpus, "tada", &Frecency::default(), NOW, 50);
    let results: Vec<registry::SearchResult> = found.into_iter().map(Into::into).collect();

    assert!(
        results.iter().all(|r| !r.strong),
        "something claimed to be named tada: {:?}",
        results.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
}


// ------------------------------------------- where a scattered match may start

#[test]
fn a_scattered_match_has_to_start_where_a_word_starts() {
    // The letters of "tada" really do appear in that order inside
    // "MTWSAndroidAppHelper", and matching them there is how a query nobody
    // meant as a search for anything installed still returned fifty-seven
    // results. The first typed character landing mid-word is the tell.
    let corpus = vec![
        command("app:mtws", "MTWSAndroidAppHelper", "Application"),
        command("setting:team", "Team Device Management", "Settings"),
    ];

    let found = ids(&search(&corpus, "tada", &Frecency::default(), NOW, 50));

    assert_eq!(
        found,
        vec!["setting:team"],
        "the one starting at a word should be the only one left"
    );
}

#[test]
fn a_scattered_match_that_starts_a_word_is_still_offered() {
    // The other half, and the reason this is a rule about the first character
    // rather than about gaps. "steam" reaches StreamNook by skipping one
    // letter, which is the single scattered match on a real machine that
    // anybody actually wants.
    let corpus = vec![
        command("app:streamnook", "StreamNook", "Application"),
        command("exe:dscdt", "DataStoreCacheDumpTool", "Developer"),
    ];

    let found = ids(&search(&corpus, "steam", &Frecency::default(), NOW, 50));

    assert!(
        found.contains(&"app:streamnook".to_string()),
        "lost the one match worth keeping: {found:?}"
    );
}

#[test]
fn typing_a_whole_word_finds_it_wherever_it_sits_in_the_title() {
    // Prefix and whole-word-elsewhere are the same act: somebody typed a word
    // this thing is called. Ranking the first above the second put a heart
    // suit above the red heart and a moon cake above the full moon, because
    // position was standing in for quality. The shorter title decides instead.
    let corpus = vec![
        command("emoji:suit", "heart suit", "Symbols"),
        command("emoji:red", "red heart", "Smileys and Emotion"),
        command("emoji:decor", "heart decoration", "Smileys and Emotion"),
    ];

    let found = ids(&search(&corpus, "heart", &Frecency::default(), NOW, 50));

    assert_eq!(
        found.first().map(String::as_str),
        Some("emoji:red"),
        "got {found:?}"
    );
}
