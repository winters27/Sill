//! Ranking behaviour: what the root list shows, and what a query surfaces.
//!
//! This is the part of a launcher users judge hardest, so the tests are about
//! ordering outcomes rather than the arithmetic that produces them.

use sill_lib::registry::{search, CommandRecord, Frecency};

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
        command("uuid-generator:viewHistory", "View History", "UUID Generator"),
        command("uuid-generator:generate", "Generate UUID", "UUID Generator"),
        command("password-generator:random", "Generate Random Password", "Password Generator"),
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
    assert!(generate.is_some(), "Generate UUID should match 'gen', got {found:?}");

    // "Generate UUID" starts with the query; the others only contain it via
    // their extension title, so the direct hit must lead.
    assert_eq!(generate, Some(0), "the prefix match should lead, got {found:?}");
}

#[test]
fn non_matches_are_excluded_entirely() {
    let results = search(&corpus(), "zzzz", &Frecency::default(), NOW, 10);
    assert!(results.is_empty(), "a query matching nothing returns nothing");
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

    assert!(many > once, "more launches should score higher: {many} vs {once}");
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

    eprintln!("by AppUserModelID: {}, by path: {}", by_id.len(), by_path.len());
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

    let shortcut_targets: std::collections::HashSet<String> =
        shortcuts.iter().filter_map(sill_lib::apps::target_key).collect();

    // Anything in the merged list that is not a shortcut came from the second
    // scan, so it must bring a target no shortcut already covers.
    let shortcut_paths: std::collections::HashSet<&str> =
        shortcuts.iter().map(|a| a.path.as_str()).collect();

    let shadowing: Vec<&str> = all
        .iter()
        .filter(|a| !shortcut_paths.contains(a.path.as_str()))
        .filter(|a| {
            sill_lib::apps::target_key(a).is_some_and(|t| shortcut_targets.contains(&t))
        })
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
    assert!(!proxy.is_empty(), "'proxy' should reach the proxy settings page");

    // Every entry must know how to launch.
    assert!(
        settings.iter().all(|s| !s.entrypoint.is_empty()),
        "a settings entry with no command cannot be run"
    );

    let kinds: std::collections::BTreeSet<&str> =
        settings.iter().map(|s| s.extension_title.as_str()).collect();
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

    let before = sill_lib::registry::search_excluding(&commands, "", &frecency, NOW, 50, &[]);
    let after = sill_lib::registry::search_excluding(
        &commands,
        "",
        &frecency,
        NOW,
        50,
        &["history".to_string()],
    );

    assert!(
        before.iter().any(|r| r.command.title == "View History"),
        "the entry has to be there before it can be hidden"
    );
    assert!(
        !after.iter().any(|r| r.command.title.to_lowercase().contains("history")),
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
        NOW,
        50,
        // A blank term matches every string. Left in, it would empty the list
        // the moment someone opened the editor and did not type.
        &["   ".to_string(), "HISTORY".to_string()],
    );

    assert!(
        !results.is_empty(),
        "a blank term must not hide everything"
    );
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
        NOW,
        20,
        &[],
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
        NOW,
        20,
        &[],
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
        "visual", "code", "discord", "docker", "google", "calendar", "notepad",
        "terminal", "steam", "photo", "micro", "settings", "git", "power",
        "explorer", "sn", "st", "ca",
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
                        let (Some(a_now), Some(b_now)) = (rank(&ranked, a), rank(&ranked, b)) else {
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
    assert_eq!(class_of("ca", &corpus, "app:calc"), Some(MatchClass::TitlePrefix));

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
    assert_eq!(found.first().map(String::as_str), Some("app:chrome"), "got {found:?}");
}

#[test]
fn the_kinds_of_match_are_ordered_best_first() {
    // The sort relies on the derived Ord, so the declaration order in the
    // enum is load-bearing rather than cosmetic.
    assert!(MatchClass::ExactTitle < MatchClass::TitlePrefix);
    assert!(MatchClass::TitlePrefix < MatchClass::TitleWordStarts);
    assert!(MatchClass::TitleWordStarts < MatchClass::TitleSubstring);
    assert!(MatchClass::TitleSubstring < MatchClass::TitleSubsequence);
    assert!(MatchClass::TitleSubsequence < MatchClass::Elsewhere);
}

#[test]
fn a_match_only_in_a_keyword_ranks_below_any_match_in_the_name() {
    // Keywords make a command findable; they do not make it the answer.
    let mut with_keyword = command("ext:tool", "Unrelated Name", "Extension");
    with_keyword.keywords = vec!["docker".to_string()];

    let corpus = vec![command("app:docker", "Docker Desktop", "Application"), with_keyword];
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
        "Visual Studio Code", "Visual Studio 2022", "VSCodium", "Discord",
        "Docker Desktop", "Google Chrome", "Google Drive", "Calculator",
        "Calendar", "Screen Capture", "Notion Calendar", "Notepad",
        "Notepad++", "Windows Terminal", "Terminal Preview", "Steam",
        "Steam Link", "Spotify", "Photos", "Photoshop", "File Explorer",
        "Internet Explorer", "PowerShell 7", "Windows PowerShell ISE",
        "Command Prompt", "Task Manager", "Device Manager", "Disk Cleanup",
        "Snipping Tool", "Sticky Notes", "Sound Recorder", "Settings",
        "System Settings", "Slack", "Signal", "Skype", "Zoom Workplace",
        "OBS Studio", "Audacity", "Blender", "GIMP", "Inkscape",
        "Firefox Developer Edition", "Microsoft Edge", "Microsoft Teams",
        "Microsoft Word", "Microsoft Excel", "Postman", "PostgreSQL",
        "DB Browser for SQLite", "Docker Compose", "Git Bash", "GitHub Desktop",
    ]
    .iter()
    .enumerate()
    .map(|(i, title)| command(&format!("app:{i}"), title, "Application"))
    .collect()
}
