//! The launcher cannot ask for state that is not there yet.
//!
//! Tauri creates the windows declared in `tauri.conf.json` and only then calls
//! the `setup` hook. The launcher's webview is therefore already loading its
//! page while `setup` runs, so anything managed inside that hook is something
//! the page can ask for before it exists. Tauri's answer then is "state not
//! managed for field prefs on command search_commands", the search returns
//! nothing, and the root list stays empty until the next keystroke.
//!
//! That is how it shipped. Preferences were managed near the end of `setup`,
//! after the saved file index had been read back, and a start that took 3.5
//! seconds rather than the usual 1.5 was slow enough for the page to get in
//! first. Nothing about the fault was the page's, and it gets worse as more is
//! done during setup.
//!
//! This is an ordering test and not a timing one on purpose. Reproducing the
//! race would mean racing a real webview, and a pass would only mean this
//! machine happened to be fast enough today. Reading the ordering off the
//! source says the thing that is actually true: the state is in place before
//! there is anything that could ask for it.

use std::path::Path;

/// The commands the launcher asks before it can draw a list.
///
/// `+page.svelte` asks for all of these while mounting, except
/// `file_search_missing`, which it asks on every summon. That includes the
/// first summon, which can land while `setup` is still running. None of them
/// can wait: what comes back is the root list, the window's own appearance, and
/// whether file search has anything to say.
const FIRST_QUESTIONS: &[(&str, &str)] = &[
    ("commands/search.rs", "search_commands"),
    ("commands/search.rs", "file_search_missing"),
    ("commands/settings.rs", "get_preferences"),
    ("commands/settings.rs", "navigation_chords"),
    ("commands/launch.rs", "query_history"),
    ("commands/launch.rs", "actions_for"),
    ("commands/ai.rs", "ai_ready"),
];

/// States the builder manages under the name of a value rather than a type.
///
/// `.manage(actions::builtins())` names the function that builds the registry,
/// so looking for the type finds nothing. Being on the builder is earlier than
/// everything else here anyway: the state is inside the `App` before `build`
/// hands it back. Naming them is what lets the commands that use them stay in
/// the list above rather than being quietly left out of the check.
const NAMED_BY_VALUE: &[&str] = &["ActionRegistry"];

/// A file from the crate's own source.
fn src(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()))
}

/// Whether a stretch of source names this type.
///
/// Bounded on both sides, so looking for `State` does not find `PrefsState`.
fn mentions(source: &str, name: &str) -> bool {
    let part = |c: char| c.is_alphanumeric() || c == '_';

    source.match_indices(name).any(|(at, _)| {
        source[..at].chars().next_back().is_none_or(|c| !part(c))
            && source[at + name.len()..]
                .chars()
                .next()
                .is_none_or(|c| !part(c))
    })
}

/// The source between two markers, the second searched for after the first.
fn between(text: &str, from: &str, to: &str) -> String {
    let start = text
        .find(from)
        .unwrap_or_else(|| panic!("`{from}` is in lib.rs"));
    let end = text[start..]
        .find(to)
        .unwrap_or_else(|| panic!("`{to}` comes after `{from}` in lib.rs"))
        + start;

    text[start..end].to_string()
}

/// An item, from its first line to the brace that closes it.
///
/// Items close on a brace in the first column, which is what `rustfmt`
/// guarantees and what makes this a few lines rather than a parser.
fn item_from(text: &str, marker: &str) -> String {
    let at = text
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` is in lib.rs"));
    let rest = &text[at..];
    let end = rest.find("\n}").map_or(rest.len(), |close| close + 1);

    rest[..end].to_string()
}

/// A function's parameter list, brackets included.
fn signature(text: &str, name: &str) -> String {
    let marker = format!("fn {name}(");
    let at = text
        .find(&marker)
        .unwrap_or_else(|| panic!("{name} is declared in the file it was listed under"));
    let open = at + marker.len() - 1;

    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return text[at..open + offset + 1].to_string();
                }
            }
            _ => {}
        }
    }

    panic!("{name}'s parameter list is closed");
}

/// What a function does, from the end of its signature to its closing brace.
fn body(text: &str, name: &str) -> String {
    let signature = signature(text, name);
    let at = text.find(&signature).expect("just found") + signature.len();
    let rest = &text[at..];
    let end = rest.find("\n}").map_or(rest.len(), |close| close + 1);

    rest[..end].to_string()
}

/// The type named between here and the `>` that closes it, by its last part.
fn until_close(after: &str) -> Option<String> {
    let mut depth = 0usize;

    for (at, ch) in after.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' if depth == 0 => {
                return after[..at]
                    .trim()
                    .rsplit("::")
                    .next()
                    .map(|last| last.trim().to_string())
                    .filter(|last| !last.is_empty());
            }
            '>' => depth -= 1,
            _ => {}
        }
    }

    None
}

/// Every state a stretch of source resolves, named by the last part of its type.
///
/// Both forms count, and the second is the worse one. A missing `State<'_, T>`
/// in a signature is refused with a message the frontend can show; a missing
/// `state::<T>()` in a body panics. `try_state` is deliberately not matched: it
/// answers `None`, and the code around it is written for that.
fn states_resolved(source: &str) -> Vec<String> {
    let mut out = Vec::new();

    let mut rest = source;
    while let Some(at) = rest.find("State<'_,") {
        let after = &rest[at + "State<'_,".len()..];
        out.extend(until_close(after));
        rest = after;
    }

    let mut rest = source;
    while let Some(at) = rest.find("state::<") {
        let taken = &rest[..at];
        let after = &rest[at + "state::<".len()..];
        if !taken.ends_with("try_") {
            out.extend(until_close(after));
        }
        rest = after;
    }

    out.sort();
    out.dedup();
    out
}

/// Everything the launcher's first questions need is managed before a window
/// exists to ask them.
#[test]
fn the_first_questions_are_answerable_before_the_first_window() {
    let lib = src("lib.rs");

    // The two places that are ahead of every window. `manage_before_windows`
    // runs between `build` and `run`, and the builder chain runs before `build`
    // has even produced an app.
    let before_windows = item_from(&lib, "fn manage_before_windows(");
    let builder = between(&lib, "tauri::Builder::default()", ".setup(|app| {");

    assert!(
        before_windows.contains("app.manage("),
        "manage_before_windows manages nothing, so this test is not checking anything"
    );
    assert!(
        builder.contains(".manage("),
        "the builder manages nothing, so this test is not checking anything"
    );

    let mut checked = 0;

    for (file, command) in FIRST_QUESTIONS {
        let text = src(file);

        let mut wanted = states_resolved(&signature(&text, command));
        wanted.extend(states_resolved(&body(&text, command)));
        wanted.sort();
        wanted.dedup();

        assert!(
            !wanted.is_empty(),
            "{command} resolves no state at all, so listing it here checks nothing"
        );

        for state in wanted {
            assert!(
                mentions(&before_windows, &state)
                    || mentions(&builder, &state)
                    || NAMED_BY_VALUE.contains(&state.as_str()),
                "{command} resolves {state}, which is managed inside the setup hook.\n\
                 Tauri creates the windows before it calls that hook, so the launcher \
                 can ask this command for an answer before {state} exists, and the \
                 answer it gets is an error. Manage it in manage_before_windows or on \
                 the builder instead."
            );

            checked += 1;
        }
    }

    assert!(
        checked >= 4,
        "only {checked} states were found across {} commands, which reads as a parse \
         that stopped finding them rather than a codebase that stopped using them",
        FIRST_QUESTIONS.len()
    );
}

/// The state goes in during the one gap where no window exists.
///
/// `build` hands back an `App` and Tauri does not create a window until the
/// event loop reports itself ready, which is inside `run`. Anything managed in
/// between is in place before the first window, and nothing managed after `run`
/// can be, which the compiler already enforces: `run` takes the app by value.
#[test]
fn the_state_is_managed_between_building_and_running() {
    let lib = src("lib.rs");

    let built = lib
        .find(".build(tauri::generate_context!())")
        .expect("the app is built rather than run straight from the builder");
    let managed = lib
        .find("manage_before_windows(&app);")
        .expect("manage_before_windows is called");
    let ran = lib.find("app.run(").expect("the app is run");

    assert!(
        built < managed,
        "manage_before_windows is called somewhere ahead of the build, which includes \
         inside the setup hook. Tauri has created every window by the time that hook \
         runs. The call belongs between building the app and running it."
    );
    assert!(
        managed < ran,
        "manage_before_windows is called after run, by which point Tauri has created \
         every window"
    );
}
