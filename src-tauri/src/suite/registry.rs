//! Ranking behaviour: what the root list shows, and what a query surfaces.
//!
//! This is the part of a launcher users judge hardest, so the tests are about
//! ordering outcomes rather than the arithmetic that produces them.

use crate::registry::{self, search, Alias, Aliases, CommandRecord, Excluded, Frecency};

const NOW: i64 = 1_756_000_000;
const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

fn snippet(name: &str, keyword: &str, content: &str) -> crate::snippets::store::Snippet {
    crate::snippets::store::Snippet {
        id: "s1".to_string(),
        name: name.to_string(),
        keyword: keyword.to_string(),
        content: content.to_string(),
        ..Default::default()
    }
}

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
        manifest: None,
        toggle: None,
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

fn titles(results: &[crate::registry::RankedCommand]) -> Vec<&str> {
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
/// a hand-written corpus.
///
/// **This proved nothing for as long as it existed.** `extensions/build` is
/// gitignored, and in CI `verify:rust` runs before `gate:views`, which is the
/// only thing that builds it. So the file was never there when this ran, the
/// skip was taken every time, and the test reported a passing ranker on an
/// empty corpus. Confirmed by sabotage: `search` was made to return an empty
/// vector for every query and this still passed in 0.01 seconds. With the
/// index present the same sabotage failed it at once.
///
/// So the skip is now a decision somebody made rather than an accident of what
/// happens to be on disk. `SILL_BUILT_INDEX=required` turns a missing index
/// into a failure, and the workflow sets it after building the extensions.
/// Locally, where somebody may never have cloned the upstream tree, it still
/// skips and says so.
#[test]
fn ranks_the_real_built_index() {
    let index = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("extensions")
        .join("build")
        .join("index.json");

    let commands = crate::registry::load_index(&index);
    if commands.is_empty() {
        assert_ne!(
            std::env::var("SILL_BUILT_INDEX").as_deref(),
            Ok("required"),
            "SILL_BUILT_INDEX=required, and {} lists nothing. Build the \
             extensions before the Rust suite, or this test measures an empty \
             corpus and calls the ranker correct",
            index.display()
        );
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
    corpus.push(crate::registry::app_record(
        "Visual Studio Code",
        r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Visual Studio Code.lnk",
        None,
        "Application",
    ));
    corpus.push(crate::registry::app_record(
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
            crate::apps::tidy_name_for_test(input),
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
        crate::registry::executable_record("code", r"C:\tools\code.exe", "Command Line"),
        crate::registry::executable_record("codesign", r"C:\tools\codesign.exe", "Command Line"),
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
fn entries_are_categorised_by_where_they_resolve() {
    use crate::apps::{categorize, AppRecord};

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

#[test]
fn windows_settings_are_searchable() {
    let settings = crate::settings_catalog::load();
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

#[test]
fn sill_settings_is_reachable_by_typing() {
    let commands = crate::registry::builtins();
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

    // Nothing here launches through the shell. Two modes, because Windows'
    // own switches are not things Sill does to itself and are dispatched by
    // their own action, but both go through Sill rather than out to a program.
    for command in &commands {
        assert!(
            matches!(command.mode.as_str(), "builtin" | "system"),
            "{} has mode {:?}, which launches through the shell",
            command.title,
            command.mode
        );
    }
}

#[test]
fn an_exclusion_term_hides_matching_entries() {
    let commands = corpus();
    let frecency = Frecency::default();

    let before = crate::registry::search_excluding(
        &commands,
        "",
        &frecency,
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
        &[],
    );
    let after = crate::registry::search_excluding(
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
        &[],
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

    let results = crate::registry::search_excluding(
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
        &[],
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
    let extra = vec![crate::registry::snippet_record(&snippet(
        "Signature",
        ";sig",
        "Best,\nBrandon",
    ))];

    let results = crate::registry::search_excluding(
        index.iter().chain(extra.iter()),
        "sig",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        20,
        Excluded::none(),
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
    let extra = vec![crate::registry::snippet_record(&snippet(
        "Signature",
        ";sig",
        "Kind regards, Brandon Winters",
    ))];

    let results = crate::registry::search_excluding(
        extra.iter(),
        "regards",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        20,
        Excluded::none(),
        &[],
    );

    assert_eq!(results.len(), 1, "content should be searchable");
}

// ------------------------------------------------------- ranking stability

use crate::registry::{match_class, MatchClass};

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

fn ids(results: &[crate::registry::RankedCommand]) -> Vec<String> {
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
                            "typing {was_query:?} then {now_query:?} swapped {a} past \
                             {b} without either changing how it matched"
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
        &[],
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
        &[],
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
            &[],
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
            &[],
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
        &[],
    );
    let without = registry::search_excluding(
        commands.iter(),
        "disc",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
        &[],
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
        &[],
    );
    let without = registry::search_excluding(
        commands.iter(),
        "docker",
        &Frecency::default(),
        &Aliases::default(),
        NOW,
        50,
        Excluded::none(),
        &[],
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
        &[],
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
        &[],
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
        &[],
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
        &[],
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
        &[],
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
        &[],
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
        &[],
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
    let records = crate::emoji::records(crate::emoji::Tone::Default);

    registry::search_excluding(
        records.iter(),
        query,
        &frecency,
        &aliases,
        NOW,
        registry::SEARCH_LIMIT,
        Excluded::none(),
        &[],
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
        "code", "chrome", "term", "settings", "explorer", "obs", "python", "git", "docker",
        "slack", "word", "excel", "calc", "edge", "zoom", "teams", "photo", "spot",
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
    assert!(
        named[0].strong,
        "typing the name did not count as naming it"
    );

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
        command(
            "sill:dictation",
            "Dictation and transcripts",
            "Sill Settings",
        ),
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

    assert!(
        !found.contains(&"app:mtws".to_string()),
        "a match starting mid-word survived: {found:?}"
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

// ------------------------------------------------- how far a match may skip

#[test]
fn skipping_a_letter_is_a_match_and_skipping_fifty_is_not() {
    // Measured on real data before the number was chosen. The matches worth
    // keeping had widest gaps of 1, 2 and 2; the ones worth dropping had 7, 11
    // and 51. Both sides are pinned here so the number cannot drift into
    // either of them unnoticed.
    let keep = vec![
        command("app:streamnook", "StreamNook", "Application"),
        command("app:disk", "Disk Cleanup", "Application"),
    ];

    for (typed, wanted) in [("steam", "app:streamnook"), ("disc", "app:disk")] {
        let found = ids(&search(&keep, typed, &Frecency::default(), NOW, 50));
        assert!(
            found.contains(&wanted.to_string()),
            "{typed:?} lost {wanted}: {found:?}"
        );
    }

    // A long name whose letters happen to contain a short query in order.
    // This is not a near miss, it is a coincidence of spelling.
    let junk = vec![command(
        "note:radio",
        "An app that allows anyone to be a radio dj, playing music from your \
         library live to whoever wants to listen",
        "Notes",
    )];

    let found = ids(&search(&junk, "registry.rs", &Frecency::default(), NOW, 50));
    assert!(found.is_empty(), "matched by coincidence: {found:?}");
}

#[test]
fn the_widest_jump_is_what_counts_and_not_the_distance_covered() {
    // Span was tried first and separates nothing: a short query over a short
    // name and a long query over a long one cover the same distance while
    // being nothing alike. "StreamNook" and a sentence both span about six
    // characters per matched letter. What tells them apart is whether any
    // single jump is implausible.
    let corpus = vec![
        // Five letters, one skipped, spanning six.
        command("app:streamnook", "StreamNook", "Application"),
        // Four letters, spanning fourteen, because one jump is seven.
        command("setting:team", "Team Device Management", "Settings"),
    ];

    let found = ids(&search(&corpus, "steam", &Frecency::default(), NOW, 50));
    assert!(found.contains(&"app:streamnook".to_string()), "{found:?}");

    let found = ids(&search(&corpus, "tada", &Frecency::default(), NOW, 50));
    assert!(
        found.is_empty(),
        "a seven-character jump matched: {found:?}"
    );
}

#[test]
fn initials_are_read_as_initials_and_not_as_a_lucky_subsequence() {
    // The regression the gap limit caused, and why it is worth a pass of its
    // own. Matching greedily takes the s inside "Visual" and then has to reach
    // eleven characters for the c, which reads as a scattered near miss rather
    // than as somebody typing initials, and the gap limit then discards it.
    let corpus = vec![command("app:vscode", "Visual Studio Code", "Application")];

    let class = match_class("vsc", &corpus[0]).expect("initials should match");
    assert_eq!(class, MatchClass::TitleWordStarts, "read as {class:?}");

    let found = ids(&search(&corpus, "vsc", &Frecency::default(), NOW, 50));
    assert_eq!(found, vec!["app:vscode"]);
}

#[test]
fn initials_only_count_where_words_actually_begin() {
    // Otherwise it is just a subsequence with a nicer name, and it would put
    // every long title in front of the thing somebody meant.
    let corpus = vec![command("app:logcat", "Logcat Viewer", "Application")];

    // g and c sit next to each other inside "Logcat", starting no words.
    let class = match_class("gc", &corpus[0]).expect("still matches somehow");
    assert_ne!(class, MatchClass::TitleWordStarts, "read as initials");
}

// --------------------------------------------------- reaching a switch first

/// A switch is the thing that acts, so it wins a tie against a page about it.
///
/// Every one of these lost before, and each for its own reason: the hyphen in
/// "Wi-Fi", a tie broken by title length, and a phrase whose words sit in
/// different fields. Kept together because the answer somebody wants is the
/// same in all three, whatever it took to get there.
mod a_switch_is_reached_first {
    use super::*;

    /// A switch, built the way `builtins` builds one.
    ///
    /// A fixture rather than `registry::builtins()`, and the first version of
    /// this used the real thing and was flaky for two reasons at once: the
    /// corpus reads live hardware, so a machine with no Wi-Fi has no Wi-Fi
    /// switch to rank, and the title of a radio switch **names the state**, so
    /// "Turn Wi-Fi Off" becomes "Turn Wi-Fi On" the moment somebody turns it
    /// off. Both make the test say the ranking broke when nothing did.
    ///
    /// The titles and keywords here are copied from `registry::system_switch`
    /// rather than invented, so what is ranked is what ships.
    fn switch(id: &str, title: &str, keywords: &[&str]) -> CommandRecord {
        let mut row = command(id, title, "System Controls");
        row.mode = "system".to_string();
        row.keywords = keywords.iter().map(|k| k.to_string()).collect();
        row
    }

    /// The real switches against the real settings pages they compete with.
    fn corpus() -> Vec<CommandRecord> {
        let mut rows = vec![
            switch(
                "sill:system.radio:wifi",
                "Turn Wi-Fi Off",
                &["wifi", "wi-fi", "wireless", "wlan", "internet", "network"],
            ),
            switch(
                "sill:system.radio:bluetooth",
                "Turn Bluetooth Off",
                &["bluetooth", "bt", "wireless", "pair", "headphones"],
            ),
            switch(
                "sill:system.audio.output:speakers",
                "Speakers",
                &[
                    "audio",
                    "sound",
                    "output",
                    "speakers",
                    "headphones",
                    "device",
                ],
            ),
        ];

        // Titles taken from the Windows settings index, not invented: these
        // are the rows that actually outranked the switches.
        for (id, title) in [
            ("setting:wifi", "Wi Fi"),
            ("setting:wifi-calling", "Wi Fi Calling"),
            ("setting:bluetooth", "Bluetooth Devices"),
            ("setting:bluetooth-devices", "Bluetooth And Devices"),
            ("setting:sound", "Sound"),
        ] {
            let mut row = command(id, title, "Windows Settings");
            row.mode = "setting".to_string();
            rows.push(row);
        }

        // The PATH executable that took first place for "wifi".
        let mut exe = command("exe:wifitask", "wifitask", "Command Line");
        exe.mode = "exe".to_string();
        rows.push(exe);

        rows
    }

    fn first(query: &str) -> String {
        search(&corpus(), query, &Frecency::default(), NOW, 20)
            .first()
            .map(|hit| hit.command.title.clone())
            .unwrap_or_else(|| format!("nothing matched {query:?}"))
    }

    /// The mark in the middle of a word is not a wall, in either direction.
    ///
    /// Asked of the matcher rather than of a ranking, because that is where
    /// the change is. "wifi" against "Turn Wi-Fi Off" was a scattered
    /// subsequence, which is the weakest class there is above a typo, so two
    /// settings pages and a PATH executable came first.
    #[test]
    fn the_hyphen_in_wi_fi_is_not_a_wall() {
        for (typed, title) in [
            ("wifi", "Turn Wi-Fi Off"),
            ("wi-fi", "Turn WiFi Off"),
            ("nodejs", "Node.js"),
            ("dont", "Don't Ask Again"),
        ] {
            let word: Vec<char> = typed.chars().collect();
            let class = registry::match_name(&word, title).map(|(class, _)| class);

            assert!(
                matches!(
                    class,
                    Some(registry::MatchClass::ExactTitle | registry::MatchClass::TitleWord)
                ),
                "{typed:?} against {title:?} is {class:?}, not a name",
            );
        }
    }

    /// And the highlight skips the mark rather than running through it.
    #[test]
    fn the_mark_is_not_underlined_as_though_it_were_typed() {
        let word: Vec<char> = "wifi".chars().collect();
        let (_, matched) = registry::match_name(&word, "Turn Wi-Fi Off").expect("a match");

        // T u r n _ W i - F i _ O f f
        // 0 1 2 3 4 5 6 7 8 9
        assert_eq!(matched, vec![5, 6, 8, 9], "the hyphen at 7 is not a match");
    }

    #[test]
    fn the_switch_is_first_for_the_name_of_the_thing_it_switches() {
        assert_eq!(first("wifi"), "Turn Wi-Fi Off");
        assert_eq!(first("wi-fi"), "Turn Wi-Fi Off");
    }

    #[test]
    fn a_switch_beats_a_settings_page_that_matches_as_well() {
        // Both are word matches, so the shorter title won and "Bluetooth
        // Devices" came first. One of them opens a window where the thing can
        // be done; the other does it.
        assert_eq!(first("bluetooth"), "Turn Bluetooth Off");
    }

    #[test]
    fn a_phrase_reaches_a_switch_whose_words_are_spread_across_its_fields() {
        // "audio" and "output" are both keywords of the audio switches and
        // neither is in a title, so nothing matched at all: the row was not
        // ranked badly, it was absent.
        assert_eq!(first("audio output"), "Speakers");
    }

    /*
     * Where you were, above what exists.
     *
     * The conversation you left is one row and it expires by itself, so it
     * takes the top of the list outright rather than being nudged towards it.
     * The switch floor above deliberately skips the empty query; this one
     * does not, and these tests are the difference written down.
     */
    mod the_conversation_you_left {
        use super::*;

        fn with_a_conversation() -> Vec<CommandRecord> {
            let mut rows = corpus();
            rows.push(registry::conversation_record(
                "chat:1",
                "why is my bluetooth dropping",
                "Just now · 2 replies",
            ));
            rows
        }

        /// The empty query is ordered purely by what you reach for, and this
        /// still comes first: it is where you were, and it is about to stop
        /// existing.
        #[test]
        fn it_is_first_on_the_empty_root_list() {
            let mut frecency = Frecency::default();
            // Something reached for constantly, at the top of the frecency
            // curve. Recency 100 and the frequency cap both maxed out, which
            // is the highest score the ranker can produce.
            for _ in 0..40 {
                frecency.record("setting:sound", NOW - 60);
            }

            let found = search(&with_a_conversation(), "", &frecency, NOW, 20);

            assert_eq!(
                found.first().map(|hit| hit.command.mode.as_str()),
                Some("conversation"),
                "the most-used command in the index outranked it",
            );
        }

        /// Escape puts the search back in the field, and the search is usually
        /// the question. So the row is found by the words already there,
        /// which is why it is a record rather than a row spliced on top.
        #[test]
        fn typing_the_question_finds_it() {
            let found = search(
                &with_a_conversation(),
                "bluetooth",
                &Frecency::default(),
                NOW,
                20,
            );

            assert_eq!(
                found.first().map(|hit| hit.command.mode.as_str()),
                Some("conversation"),
                "it lost to a switch of the same name",
            );
        }

        /// It is one row about the past sitting in a list about the present.
        /// A query it has nothing to do with must not surface it.
        #[test]
        fn a_query_it_does_not_match_does_not_surface_it() {
            let found = search(
                &with_a_conversation(),
                "speakers",
                &Frecency::default(),
                NOW,
                20,
            );

            assert!(
                !found.iter().any(|hit| hit.command.mode == "conversation"),
                "it appeared for a query that has nothing to do with it",
            );
        }

        /// The id is what the window sends back to reopen it, and the row id
        /// is what frecency and the keyed list use. They are not the same
        /// string, and mixing them up reopens nothing.
        #[test]
        fn the_row_carries_the_conversation_id_separately_from_its_own() {
            let row = registry::conversation_record("chat:7", "a question", "Just now · 1 reply");

            assert_eq!(row.entrypoint, "chat:7", "what gets reopened");
            assert_eq!(row.id, "sill:chat:7", "what the list is keyed by");
            assert_ne!(row.id, row.entrypoint);
        }
    }

    /// One visit is not a preference, and this is where a bonus was too small.
    ///
    /// The first attempt added a dozen points, which read as enough. It was
    /// not: recency dominates the frecency curve, so a page opened **once,
    /// earlier today** scores 77 and buried the switch in the running app
    /// while every test passed. Hence a floor rather than a bonus.
    #[test]
    fn a_page_visited_once_today_does_not_bury_the_switch() {
        let mut frecency = Frecency::default();
        frecency.record("setting:bluetooth", NOW - 5 * HOUR);

        let found = search(&corpus(), "bluetooth", &frecency, NOW, 20);
        assert_eq!(found[0].command.title, "Turn Bluetooth Off");
    }

    /// The floor is a starting point. It does not overrule what somebody uses.
    #[test]
    fn a_page_someone_opens_every_day_still_wins() {
        let mut frecency = Frecency::default();
        for _ in 0..5 {
            frecency.record("setting:bluetooth", NOW - HOUR);
        }

        let found = search(&corpus(), "bluetooth", &frecency, NOW, 20);
        assert_eq!(found[0].command.title, "Bluetooth Devices");
    }

    /// The root list is ordered by what you reach for, not by what is a switch.
    #[test]
    fn an_empty_query_is_not_reshuffled_by_the_floor() {
        let mut frecency = Frecency::default();
        frecency.record("setting:sound", NOW - HOUR);

        let found = search(&corpus(), "", &frecency, NOW, 20);
        assert_eq!(
            found[0].command.title, "Sound",
            "a switch climbed a list that nobody typed into",
        );
    }
}

// ----------------------------------------------------------- system switches

#[test]
fn the_system_switches_are_reachable_by_the_words_people_use() {
    // Nobody types "toggle mute". They type "mute", or "quiet", or "silence",
    // and a switch nobody can find is a switch that does not exist.
    let corpus = registry::builtins();

    for (typed, wanted) in [
        ("mute", "sill:system.mute"),
        ("quiet", "sill:system.mute"),
        ("silence", "sill:system.mute"),
        ("volume", "sill:system.volume.up"),
        ("louder", "sill:system.volume.up"),
        ("quieter", "sill:system.volume.down"),
        ("dark", "sill:system.theme"),
        ("dark mode", "sill:system.theme"),
        ("night", "sill:system.theme"),
        ("lock", "sill:system.lock"),
        ("afk", "sill:system.lock"),
        // The power rows, which are worth nothing at all if the word somebody
        // types for them reaches something else instead.
        ("sleep", "sill:system.power.sleep"),
        ("hibernate", "sill:system.power.hibernate"),
        ("sign out", "sill:system.power.signout"),
        ("log off", "sill:system.power.signout"),
        ("restart", "sill:system.power.restart"),
        ("reboot", "sill:system.power.restart"),
        ("shut down", "sill:system.power.shutdown"),
        ("shutdown", "sill:system.power.shutdown"),
        ("turn off", "sill:system.power.shutdown"),
    ] {
        let found = ids(&search(&corpus, typed, &Frecency::default(), NOW, 50));

        assert!(
            found.iter().any(|id| id == wanted),
            "{typed:?} does not reach {wanted}: {:?}",
            found.iter().take(5).collect::<Vec<_>>()
        );
    }
}

/// Every way of ending a session is a row, and none of them is a switch.
///
/// The two halves matter separately. A row that is not built is a command that
/// does not exist, and a row the launcher thinks is a switch would be drawn
/// with a control beside it saying the machine is currently switched off.
#[test]
fn the_power_commands_are_rows_and_none_of_them_draws_a_switch() {
    let corpus = registry::builtins();

    for power in crate::system::Power::ALL {
        let row = corpus
            .iter()
            .find(|row| row.entrypoint == power.id())
            .unwrap_or_else(|| panic!("{:?} has no row", power));

        assert_eq!(row.id, format!("sill:{}", power.id()));
        assert_eq!(row.mode, "system");
        assert!(!row.title.is_empty(), "{} has no title", row.id);
        assert!(!row.subtitle.is_empty(), "{} says nothing", row.id);
        assert!(row.icon.is_some(), "{} wears nothing", row.id);

        assert!(
            !crate::system::is_switch(&row.entrypoint),
            "{} would be drawn as something with an on and an off",
            row.id,
        );
    }
}

/// Emptying the bin is a system row and is not a switch.
///
/// The same two halves the power rows are checked for, and the second is the
/// one that matters here: a row Sill thought was a switch would be drawn with
/// a control beside it, and a control says "this is a state you can put back".
/// Nothing about this can be put back.
#[test]
fn the_recycle_bin_row_exists_and_is_not_drawn_as_something_with_an_off() {
    let corpus = registry::builtins();

    let row = corpus
        .iter()
        .find(|row| row.entrypoint == "system.recycle-bin.empty")
        .expect("the recycle bin has a row");

    assert_eq!(row.mode, "system");
    assert!(row.icon.is_some(), "{} wears nothing", row.id);
    assert!(!row.subtitle.is_empty(), "{} says nothing", row.id);

    assert!(
        !crate::system::is_switch(&row.entrypoint),
        "{} would be drawn as something with an on and an off",
        row.id,
    );
}

/// The words that reach it are the ones that mean it and nothing else.
///
/// "delete" and "clear" are typed about all sorts of things, and a row that
/// permanently removes everything in the bin turning up in those searches is a
/// row somebody arrows onto by accident on the way to something else.
#[test]
fn emptying_the_bin_is_not_found_by_a_word_about_something_else() {
    let corpus = registry::builtins();
    let bin = "sill:system.recycle-bin.empty";

    for wanted in ["recycle bin", "empty recycle", "trash"] {
        let found = ids(&search(&corpus, wanted, &Frecency::default(), NOW, 50));
        assert!(
            found.iter().any(|id| id == bin),
            "{wanted:?} does not reach the recycle bin row"
        );
    }

    for loose in ["delete", "clear", "remove"] {
        let found = ids(&search(&corpus, loose, &Frecency::default(), NOW, 50));
        assert!(
            !found.iter().any(|id| id == bin),
            "{loose:?} brings up the row that permanently deletes everything"
        );
    }
}

#[test]
fn a_system_switch_says_what_it_does_rather_than_what_the_machine_is_doing() {
    // Titles that named the state would need the audio endpoint queried to
    // know which word to use, and the index is built at startup and searched
    // on every keystroke. Neither is a place for a COM round trip.
    let corpus = registry::builtins();
    let switches: Vec<&CommandRecord> = corpus
        .iter()
        .filter(|row| row.id.starts_with("sill:system."))
        .collect();

    assert!(switches.len() >= 7, "only {} switches", switches.len());

    for switch in switches {
        assert!(!switch.title.is_empty(), "{} has no title", switch.id);

        /*
         * A row says something, unless the switch on it already does.
         *
         * This asked every switch for a subtitle, and it was written before
         * the rows drew a control. A radio's said "It is on" and an output's
         * said "Sound is going here", which is exactly what the switch beside
         * them now says: the same fact twice, and the second copy goes stale
         * on its own if anything ever reads one and not the other.
         *
         * Asked of `is_switch`, which is the one function that decides what
         * counts as a switch, so this and the row cannot disagree. Not of
         * `toggle_state`: that answers with the machine's current reading, and
         * a fixture has no radios in it, so every radio came back "not a
         * switch" and this asked them for a subtitle after all.
         */
        let drawn_as_a_switch = crate::system::is_switch(&switch.entrypoint);

        if !drawn_as_a_switch {
            assert!(!switch.subtitle.is_empty(), "{} says nothing", switch.id);
        }
        // "Unmute" and "Switch to Light Mode" both describe a state.
        assert!(
            !switch.title.starts_with("Unmute") && !switch.title.contains("Switch to"),
            "{} names a state rather than an action: {:?}",
            switch.id,
            switch.title
        );
    }
}

/// A row that is a switch has to reach the window still knowing it is one.
///
/// The state is read at search time and written onto the record, and the
/// record is then narrowed into the smaller shape the window is sent. That
/// conversion listed every field by hand and set this one to `None`, so the
/// reading was taken, thrown away, and every switch drew as an ordinary row.
/// Nothing failed and nothing was logged.
#[test]
fn the_way_a_switch_is_set_survives_being_narrowed_for_the_window() {
    let mut record = command("sill:system.mute", "Mute", "System Controls");
    record.mode = "system".to_string();
    record.toggle = Some(true);

    let ranked = registry::RankedCommand {
        command: record.clone(),
        matched: Vec::new(),
        class: registry::MatchClass::ExactTitle,
        score: 1,
    };

    let narrowed: registry::SearchResult = ranked.into();
    assert_eq!(
        narrowed.toggle,
        Some(true),
        "the switch reached the window not knowing it was one",
    );

    // And the path the switcher takes, which does not rank.
    assert_eq!(
        registry::SearchResult::from_record(record).toggle,
        Some(true)
    );
}

/// The index has to hold each id once, whichever way it arrived.
///
/// An id is not a label. Aliases, hotkeys, hidden entries and frecency scores
/// are all keyed on it, so two rows sharing one id share all four: running
/// either promotes both, hiding either hides both. It is also the identity the
/// result list is drawn by, and there a repeat costs the whole list rather than
/// a row.
///
/// Four Windows settings pages reached the index as `setting:mmc.exe`, and the
/// launcher opened on an empty list.
mod one_id_per_row {
    use super::*;

    #[test]
    fn the_first_of_a_repeated_id_is_the_one_kept() {
        let out = registry::one_per_id(vec![
            command("app:terminal", "Terminal", "Applications"),
            command("app:browser", "Browser", "Applications"),
            command(
                "app:terminal",
                "Terminal, from somewhere else",
                "Applications",
            ),
        ]);

        let titles: Vec<&str> = out.iter().map(|r| r.title.as_str()).collect();
        assert_eq!(titles, ["Terminal", "Browser"]);
    }

    #[test]
    fn a_list_with_no_repeats_is_left_exactly_as_it_was() {
        let given = vec![
            command("app:c", "C", "Applications"),
            command("app:a", "A", "Applications"),
            command("app:b", "B", "Applications"),
        ];

        let out = registry::one_per_id(given.clone());

        assert_eq!(out.len(), given.len());
        // Order matters as much as membership: it is the ranking's own.
        assert_eq!(
            out.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            given.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        );
    }
}

/// The store is reachable by typing the words somebody would type.
///
/// The same rule as text recognition below, and it needs its own guard for a
/// harder reason: **the store competes with the Microsoft Store.** Typing
/// "store" on any Windows machine returns a screenful of real applications
/// with that word in their name, and a row that ranks below them is a row
/// nobody finds. The word is in the title, so it has to place, and a keyword
/// bag alone would not have been enough to notice if it stopped.
mod the_store_is_findable {
    use super::*;

    fn found(query: &str) -> Vec<String> {
        let commands = registry::builtins();
        search(&commands, query, &Frecency::default(), NOW, 60)
            .into_iter()
            .map(|hit| hit.command.title.clone())
            .collect()
    }

    #[test]
    fn the_words_somebody_would_actually_type_find_it() {
        for query in [
            "store",
            "extension",
            "extensions",
            "extension store",
            "browse",
            "install",
            "marketplace",
            "raycast",
            "plugin",
            "discover",
        ] {
            let titles = found(query);
            assert!(
                titles.iter().any(|t| t == "Extension Store"),
                "{query:?} did not find the store, found {titles:?}",
            );
        }
    }

    #[test]
    fn updating_is_findable_by_the_words_that_mean_updating() {
        for query in [
            "update",
            "updates",
            "upgrade",
            "outdated",
            "update extensions",
        ] {
            let titles = found(query);
            assert!(
                titles.iter().any(|t| t == "Update Extensions"),
                "{query:?} did not find it, found {titles:?}",
            );
        }
    }

    /// Both extension rows and the folder install are three different jobs.
    ///
    /// They share every keyword, so the risk is not that one is missing but
    /// that one buries the others. Typing the bare word has to offer all three.
    #[test]
    fn the_three_extension_rows_all_place_for_the_bare_word() {
        let titles = found("extension");

        for wanted in ["Extension Store", "Update Extensions", "Install Extension"] {
            assert!(
                titles.iter().any(|t| t == wanted),
                "{wanted} is missing from {titles:?}",
            );
        }
    }

    /// **Against the machine, not against an empty index.**
    ///
    /// Searching `builtins()` alone proves the row exists and proves nothing
    /// about whether anybody sees it. On a real Windows machine "store" is a
    /// crowded word: the Microsoft Store, three SDK documentation entries, and
    /// every packaged application in the index reports itself as a Store App.
    /// The observed list on this machine had the store nowhere on the first
    /// screen, and the row existing was never the question.
    ///
    /// These are the actual competing titles, taken from a screenshot of the
    /// running launcher rather than invented.
    fn against_the_real_index(query: &str) -> Vec<String> {
        const COMPETITORS: &[(&str, &str)] = &[
            ("store", "Application"),
            ("Microsoft Store", "Store App"),
            ("Tools for Windows Store Apps", "Application"),
            ("Samples for Windows Store Apps", "Application"),
            ("Documentation for Windows Store Apps", "Documentation"),
            ("Notepad", "Store App"),
            ("Calculator", "Store App"),
            ("Settings", "Store App"),
            ("Microsoft Store Install Service", "Application"),
            ("Windows Store Restore", "Application"),
        ];

        let mut corpus = registry::builtins();
        for (name, kind) in COMPETITORS {
            corpus.push(crate::registry::app_record(
                name,
                &format!(r"C:\Program Files\{name}.exe"),
                None,
                kind,
            ));
        }

        search(&corpus, query, &Frecency::default(), NOW, 60)
            .into_iter()
            .map(|hit| hit.command.title.clone())
            .collect()
    }

    /// How far down the list somebody has to look before giving up.
    ///
    /// Eight, because that is roughly what the launcher shows without
    /// scrolling at its default height. A row below the fold for the most
    /// obvious query is a row nobody finds, which is the same standard the
    /// text recognition rows are held to.
    const FIRST_SCREEN: usize = 8;

    #[test]
    fn the_store_is_on_the_first_screen_for_the_word_store() {
        let titles = against_the_real_index("store");
        let at = titles.iter().position(|t| t == "Extension Store");

        assert!(
            at.is_some_and(|at| at < FIRST_SCREEN),
            "Extension Store placed at {at:?} against the Microsoft Store and \
             friends, so nobody typing \"store\" would see it. Got {titles:?}",
        );
    }

    /// The narrower word must be decisive.
    ///
    /// Nothing Windows ships is called an extension, so if this one is not
    /// first something is very wrong with the keywords.
    #[test]
    fn the_word_extension_reaches_it_first() {
        let titles = against_the_real_index("extension");
        assert_eq!(
            titles.first().map(String::as_str),
            Some("Extension Store"),
            "got {titles:?}",
        );
    }
}

/// A capability nobody can find is a capability nobody has.
///
/// Text recognition is reachable three ways on purpose: as an action on a
/// clipboard row, as a row in the list, and as a key bound to it. This holds
/// the middle one, which is the one somebody discovers by typing.
mod text_recognition_is_findable {
    use super::*;

    fn found(query: &str) -> Vec<String> {
        let commands = registry::builtins();
        search(&commands, query, &Frecency::default(), NOW, 60)
            .into_iter()
            .map(|hit| hit.command.title.clone())
            .collect()
    }

    #[test]
    fn the_words_somebody_would_actually_type_find_it() {
        // "read text" is here because it used to be impossible. A query of
        // more than one word only matched when those words sat together in one
        // field, and "read" is a keyword while "text" is in the title, so
        // nothing put them together. A phrase now matches when every word of
        // it lands somewhere on the row.
        for query in [
            "ocr",
            "read",
            "text",
            "scan",
            "picture",
            "screenshot",
            "extract text",
            "read text",
        ] {
            let titles = found(query);
            assert!(
                titles.iter().any(|t| t == "Extract Text from Image"),
                "{query:?} did not find it, found {titles:?}",
            );
        }
    }

    /// A phrase still has to be about this row, not merely overlap it.
    ///
    /// Every word landing somewhere is the rule, and each word has to be a
    /// whole word of the title or a keyword of its own. Without that strictness
    /// a phrase would find something for almost anything typed, which is worse
    /// than finding nothing.
    #[test]
    fn a_phrase_with_a_word_that_lands_nowhere_does_not_match() {
        assert!(found("read banana").is_empty());
        assert!(found("extract sandwich").is_empty());
    }
}

/// Sill's own settings are findable from the launcher.
///
/// They live in their own list rather than in the scanned index, so they are
/// absent from `index-cache.json` and present in every search. Reading the
/// cache and concluding otherwise is a mistake this test exists to stop.
mod sills_own_settings_are_findable {
    use super::*;

    fn found(query: &str) -> Vec<String> {
        let commands = registry::builtins();
        let own = crate::settings_index::records();

        registry::search_excluding(
            commands.iter().chain(own.iter()),
            query,
            &Frecency::default(),
            &Aliases::default(),
            NOW,
            60,
            Excluded {
                terms: &[],
                ids: &[],
            },
            &[],
        )
        .into_iter()
        .map(|hit| hit.command.title.clone())
        .collect()
    }

    #[test]
    fn a_setting_is_found_by_its_own_name() {
        for query in ["stroke width", "engine", "bookmarks"] {
            assert!(!found(query).is_empty(), "{query:?} found nothing");
        }
    }

    #[test]
    fn the_new_screenshot_settings_are_among_them() {
        for (query, wanted) in [
            ("screenshot hotkey", "Screenshot hotkey"),
            ("badge", "Badges start at"),
            ("walkthrough", "Badges start at"),
            ("markup", "After taking one"),
        ] {
            let titles = found(query);
            assert!(
                titles.iter().any(|t| t == wanted),
                "{query:?} did not find {wanted:?}, found {titles:?}",
            );
        }
    }
}

/// Each system switch answers to its own name and not its neighbour's.
///
/// Both radio rows shared one keyword list at first, so "wifi" was an exact
/// keyword on the Bluetooth row and put it above the Wi-Fi one. An exact
/// keyword outranks the subsequence match "wifi" makes against "Turn Wi-Fi
/// Off", so the wrong switch came first for its own name.
mod a_switch_answers_to_its_own_name {
    use super::*;

    fn best(commands: &[CommandRecord], query: &str) -> Option<String> {
        search(commands, query, &Frecency::default(), NOW, 20)
            .into_iter()
            .find(|hit| hit.command.mode == "system")
            .map(|hit| hit.command.title.clone())
    }

    #[test]
    fn the_radio_somebody_named_is_the_one_that_comes_first() {
        let commands = registry::builtins();

        // Only meaningful on a machine that has them, and the probe beside
        // this covers the case where it does not.
        let has_wifi = commands.iter().any(|c| c.title.contains("Wi-Fi"));
        let has_bt = commands.iter().any(|c| c.title.contains("Bluetooth"));
        if !has_wifi || !has_bt {
            return;
        }

        let wifi = best(&commands, "wifi").unwrap_or_default();
        assert!(wifi.contains("Wi-Fi"), "\"wifi\" gave {wifi:?}");

        let bluetooth = best(&commands, "bluetooth").unwrap_or_default();
        assert!(
            bluetooth.contains("Bluetooth"),
            "\"bluetooth\" gave {bluetooth:?}"
        );
    }
}

/// A program's volume, shaped into a row.
///
/// The switch on one of these means **audible**, not muted, and that is a rule
/// rather than an inconsistency: the switch answers whatever the row's title
/// names. The system row is called "Toggle Mute", so its switch says whether
/// mute is on. This row is called by the program's name, so its switch says
/// whether the program is.
mod a_program_volume_row {
    use super::*;
    use crate::app_volume::Session;

    fn session(name: &str, volume: f32, muted: bool) -> Session {
        Session {
            id: r"{0.0.0}.{abc}|C:\x\thing.exe%b|1%b900".to_string(),
            name: name.to_string(),
            volume,
            muted,
            path: r"C:\x\thing.exe".to_string(),
        }
    }

    #[test]
    fn the_switch_says_whether_you_can_hear_it() {
        let audible = registry::audio_session_record(&session("Thing", 0.6, false));
        assert_eq!(audible.toggle, Some(true));

        let silent = registry::audio_session_record(&session("Thing", 0.6, true));
        assert_eq!(silent.toggle, Some(false));
    }

    /// How loud, which is the one thing the switch cannot say.
    #[test]
    fn the_subtitle_carries_the_level() {
        let row = registry::audio_session_record(&session("Thing", 0.6, false));
        assert_eq!(row.subtitle, "60%");
    }

    /// Muting keeps the level, so unmuting puts it back where it was. The row
    /// says so rather than reading as though it had been turned down to zero.
    #[test]
    fn a_muted_row_still_says_where_the_slider_is() {
        let row = registry::audio_session_record(&session("Thing", 0.3, true));
        assert_eq!(row.subtitle, "Muted, was at 30%");
    }

    /// The row wears the mark of the program behind it, like every other row.
    #[test]
    fn the_row_wears_the_programs_own_mark() {
        let row = registry::audio_session_record(&session("Thing", 1.0, false));
        assert_eq!(row.icon.as_deref(), Some(r"C:\x\thing.exe"));
    }

    /// System sounds have no program, so there is no mark to take.
    #[test]
    fn nothing_pretends_to_have_an_icon_it_does_not() {
        let mut without = session("System Sounds", 1.0, false);
        without.path = String::new();

        assert_eq!(registry::audio_session_record(&without).icon, None);
    }

    /// The row has to be able to find the session again, and only Windows'
    /// own identifier does: a name is shared and a process number is not the
    /// same one tomorrow.
    #[test]
    fn the_row_carries_the_identifier_that_finds_it_again() {
        let one = session("Thing", 1.0, false);
        let row = registry::audio_session_record(&one);

        assert_eq!(row.entrypoint, one.id);
        assert!(row.id.starts_with("audio-session:"));
    }
}

/// Saving the ranking history leaves nothing behind and reads back whole.
///
/// The reason it is staged and renamed is that it is written on **every
/// launch**, so an interrupted write is not a remote possibility: it is
/// whatever was in flight when the machine went down. A truncated JSON file
/// parses as nothing, and nothing means the root list is ordered as though the
/// user had never launched anything.
///
/// **What this test does not prove is the atomicity itself.** Interrupting a
/// write at the right instant is not something a test can arrange here, and an
/// earlier version of this test looked like it proved it and did not: it wrote
/// the torn bytes to the staging path, which of course leaves the real file
/// alone whether or not anything is staged. It passed with the staging removed.
/// What is asserted is the part that is observable: the content survives a
/// round trip and no half-written file is left lying next to it.
#[test]
fn saving_the_ranking_history_leaves_nothing_half_written_behind() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let path = dir.path().join("frecency.json");

    let mut frecency = Frecency::default();
    frecency.record("code", NOW);
    frecency.record("code", NOW);
    frecency.save(&path).expect("saved");

    let back: Frecency = serde_json::from_str(&std::fs::read_to_string(&path).expect("readable"))
        .expect("the saved file parses");
    assert_eq!(back.len(), 1);

    assert!(
        !path.with_extension("json.partial").exists(),
        "the staging file outlived the save, so the next reader sees two files \
         and one of them is a half-written copy of the other"
    );

    // Twice, because the rename has to land on a file that already exists.
    frecency.record("other", NOW);
    frecency
        .save(&path)
        .expect("saved again over the previous file");

    let back: Frecency = serde_json::from_str(&std::fs::read_to_string(&path).expect("readable"))
        .expect("the second save parses");
    assert_eq!(
        back.len(),
        2,
        "the second save replaced rather than appended"
    );
}

/// A window competing on merit, not on which list it was appended to.
///
/// Windows used to come back from a command of their own and be appended after
/// the index results had already been capped at `SEARCH_LIMIT`. On a short
/// query the cap fills with weak matches, so a window whose title was an exact
/// match landed past the end of the list and was never seen. Two lists
/// concatenated is not a ranking; the cap is what made that visible.
///
/// Ranked in the same pass, the exact title wins wherever it came from.
#[test]
fn an_exact_window_title_outranks_a_scattered_command_match() {
    let mut crowd: Vec<CommandRecord> = (0..200)
        .map(|n| {
            command(
                &format!("app:{n}"),
                &format!("Notes Editor Update {n}"),
                "Apps",
            )
        })
        .collect();

    // The window, last in the corpus exactly as an appended list would be.
    crowd.push(command("window:1", "neu", "Terminal"));

    let results = search(&crowd, "neu", &Frecency::default(), NOW, 20);

    assert_eq!(
        results.first().map(|hit| hit.command.id.as_str()),
        Some("window:1"),
        "the exact title has to win from anywhere in the corpus, or being \
         appended after the cap is the same as not being there"
    );
}
