//! Searching, and opening what was found.

use tauri::{AppHandle, State};

use crate::state::{now_seconds, CatalogState, PrefsState, RegistryState};
use crate::{browsers, calculator, files, registry, windowing};

/// The root list, or what matches a query.
#[tauri::command]
pub(crate) async fn search_commands(
    state: State<'_, RegistryState>,
    prefs: State<'_, PrefsState>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let (excluded, hidden) = {
        let prefs = prefs.inner.lock().await;
        (prefs.sources.excluded.clone(), prefs.sources.hidden.clone())
    };
    let registry = state.inner.lock().await;

    // Chained, not collected: both sides are borrowed and nothing is copied.
    let mut results = registry::search_excluding(
        registry
            .commands
            .iter()
            .chain(registry.snippets.iter())
            .chain(registry.quicklinks.iter())
            .chain(registry.own_settings.iter()),
        &query,
        &registry.frecency,
        &registry.aliases,
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded {
            terms: &excluded,
            ids: &hidden,
        },
    );

    // Above everything, because when a query IS a sum the answer is the only
    // thing wanted. `evaluate` returns nothing for the ninety-nine queries in
    // a hundred that are searches, so this costs those nothing.
    if let Some(answer) = calculator::evaluate(&query) {
        results.insert(0, registry::answer_record(&answer.text, &answer.input));
    }

    // Narrowed to what the window actually reads on the way out. The ranked
    // form carries the fields matching needs, which is most of the bytes and
    // none of the use once ranking is over.
    Ok(results
        .into_iter()
        .map(|ranked| {
            // Looked up here rather than carried through ranking: only the
            // rows that survive are drawn, and only drawn rows show a name.
            let alias = registry
                .aliases
                .for_command(&ranked.command.id)
                .map(str::to_string);
            let mut result: registry::SearchResult = ranked.into();
            result.alias = alias;
            result
        })
        .collect())
}

/// The open windows matching a query.
///
/// Separate from `search_commands` for the reason file search is separate: it
/// is a different corpus with a different lifetime. The index is scanned once
/// and cached; the desktop is enumerated fresh every time, because a window
/// list is wrong the moment anything is opened or closed.
///
/// Ranked by the same function as everything else. A window is a
/// `CommandRecord` for exactly as long as the ranking takes, so "chrome"
/// finds Chrome windows by the same rules that make it find Chrome.
#[tauri::command]
pub(crate) async fn search_windows(
    state: State<'_, RegistryState>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    // Blocking: enumeration is synchronous Win32 and touches every top-level
    // window on the desktop.
    let records = tokio::task::spawn_blocking(windowing::records)
        .await
        .unwrap_or_default();

    // An empty query is the switcher, and its order is already right.
    //
    // Enumeration walks the Z-order from the front, which is what recency
    // means for windows. Ranking an empty query sorts by frecency and then by
    // title, which would replace "the window you were just in" with "the
    // window with the shortest name". Alt-Tab's whole value is that first
    // entry, so it is left alone.

    if query.trim().is_empty() {
        return Ok(records
            .into_iter()
            .take(registry::SEARCH_LIMIT)
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let registry = state.inner.lock().await;

    // No exclusion terms. Those hide things from the index, and a window that
    // is open is a fact rather than a preference: hiding it would mean the
    // switcher cannot reach something the taskbar shows.
    let results = registry::search_excluding(
        records.iter(),
        &query,
        &registry.frecency,
        // A window is not in the index, so nothing can have been given a name
        // for one. An alias points at a command id that survives a restart.
        &registry::Aliases::default(),
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded::none(),
    );

    Ok(results.into_iter().map(Into::into).collect())
}

/// Emoji matching a query.
///
/// Its own corpus rather than part of the index. Three thousand seven hundred
/// entries would nearly quadruple a fifteen-hundred-entry index that is ranked
/// on every keystroke, so that typing "smile" could find an emoji as well as
/// an application. Behind its own command, they cost nothing until asked for.
#[tauri::command]
pub(crate) async fn search_emoji(
    state: State<'_, RegistryState>,
    prefs: State<'_, PrefsState>,
    query: String,
    // Whether these are being offered beside results that were asked for.
    //
    // Emoji volunteer themselves into the root list, so they have to earn the
    // room: a handful, and only where the user plainly named the thing. Loose
    // matching would put a smiley in the middle of every search, because there
    // are nearly two thousand of them and their names are ordinary words.
    //
    // The picker itself passes nothing, because there the emoji ARE the list.
    inline: Option<bool>,
) -> Result<Vec<registry::SearchResult>, String> {
    let tone = prefs.inner.lock().await.emoji.tone;

    let records = tokio::task::spawn_blocking(move || crate::emoji::records(tone))
        .await
        .unwrap_or_default();

    let registry = state.inner.lock().await;

    // An empty query lists them in their own order, which is by group and then
    // by how Unicode arranged them: smileys, people, animals, food. Ranking
    // that by frecency would scatter related emoji across the list.
    if query.trim().is_empty() {
        return Ok(records
            .into_iter()
            .map(registry::SearchResult::from_record)
            .collect());
    }

    let results = registry::search_excluding(
        records.iter(),
        &query,
        &registry.frecency,
        &registry.aliases,
        now_seconds(),
        registry::SEARCH_LIMIT,
        registry::Excluded::none(),
    );

    if !inline.unwrap_or(false) {
        return Ok(results.into_iter().map(Into::into).collect());
    }

    Ok(results
        .into_iter()
        .filter(|ranked| {
            registry::match_class_with_alias(
                &query,
                &ranked.command,
                registry.aliases.for_command(&ranked.command.id).unwrap_or(""),
            )
            .is_some_and(registry::is_strong)
        })
        .take(INLINE_EMOJI)
        .map(Into::into)
        .collect())
}

/// How many emoji may appear beside an ordinary search.
///
/// Few. They are volunteering rather than being asked for, and a row of them
/// pushing applications off the screen is worse than not offering any.
const INLINE_EMOJI: usize = 4;

/// Every display, for laying windows out.
#[tauri::command]
pub(crate) async fn list_monitors() -> Result<Vec<windowing::Monitor>, String> {
    Ok(windowing::monitors())
}

/// The program that opens a web address on this machine.
///
/// So the row offering to search the web can wear the mark of the browser it
/// will open, rather than Sill's. The row is not Sill doing something; it is
/// Sill handing the question to that program, and it should look like it.
#[tauri::command]
pub(crate) async fn default_browser() -> Result<Option<String>, String> {
    Ok(browsers::default_browser().map(|path| path.to_string_lossy().into_owned()))
}

/// The search engines Sill knows.
///
/// Named by Rust so the list exists once. A settings pane holding its own copy
/// is a second place to add an engine and a first place to forget one.
#[tauri::command]
pub(crate) async fn search_engines() -> Result<Vec<crate::websearch::Engine>, String> {
    Ok(crate::websearch::ENGINES.to_vec())
}

/// Which browsers are on this machine, named.
///
/// So the settings page can say what would be read rather than leaving somebody
/// to trust a switch. A feature that reads a browsing history should be able to
/// answer "whose?" before it is turned on.
///
/// Names only, and each one once: a browser with four profiles is still one
/// browser as far as the question goes.
#[tauri::command]
pub(crate) async fn browser_profiles() -> Result<Vec<KnownBrowser>, String> {
    let mut found: Vec<KnownBrowser> = Vec::new();

    for profile in browsers::profiles() {
        if found.iter().any(|known| known.name == profile.browser) {
            continue;
        }

        found.push(KnownBrowser {
            name: profile.browser,
            program: profile.program.map(|p| p.to_string_lossy().into_owned()),
        });
    }

    Ok(found)
}

/// A browser Sill found, and the program behind it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnownBrowser {
    pub name: String,
    /// So the pane can show the browser's own mark rather than describing it.
    pub program: Option<String>,
}

/// Pages a browser remembers, visited or saved.
///
/// Separate from `search_commands` for the same reason files are: it reads
/// files that belong to other programs and are large, so the window asks for it
/// behind a debounce and lets what Sill already knows appear first.
///
/// Copies live under Sill's own data directory rather than in the system
/// temporary folder. They are derived from somebody's browsing history, and
/// leaving that in a world-writable directory that nothing ever cleans is not
/// where it belongs.
#[tauri::command]
pub(crate) async fn search_browsers(
    app: AppHandle,
    state: State<'_, PrefsState>,
    query: String,
) -> Result<Vec<browsers::Hit>, String> {
    let settings = state.inner.lock().await.browsers.clone();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    let scratch = crate::state::data_dir(&app).join("browser-copies");
    let wanted = settings.max_results as usize;
    let want = browsers::Want {
        history: settings.history,
        bookmarks: settings.bookmarks,
    };

    // Reads and copies files, so it never runs on an async worker.
    tokio::task::spawn_blocking(move || browsers::search(&query, wanted, want, &scratch))
        .await
        .map_err(|err| format!("browser search failed: {err}"))
}

/// Files matching a query, from Everything.
///
/// Separate from `search_commands` rather than merged into it: this spawns a
/// process, so the UI debounces it and lets command results appear first.
#[tauri::command]
pub(crate) async fn search_files(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
    query: String,
) -> Result<Vec<files::FileHit>, String> {
    let settings = state.inner.lock().await.files.clone();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    let wanted = settings.max_results as usize;

    // Sill's own index first. It knows the folders somebody actually works in
    // and it answers in a few milliseconds without a second program being
    // installed, so it is the answer rather than the fallback.
    //
    // Narrowed by the same setting that narrows the other source. It says
    // "only show results in", and a filter that only applied to one of two
    // sources would be a setting that half worked, which is worse than one
    // that does not exist.
    let ours = catalog
        .inner
        .load()
        .search(query.trim(), wanted, &settings.only_in);

    // Then a whole-volume indexer, when one is running. It sees the rest of
    // the machine, which our index deliberately does not.
    let scoped = files::scope(&query, &settings.only_in);
    let theirs = tokio::task::spawn_blocking(move || {
        files::search_with(
            &scoped,
            wanted,
            settings.match_path,
            settings.match_case,
            settings.regex,
        )
    })
    .await
    .unwrap_or_default();

    Ok(merge(ours, theirs, wanted))
}

/// Puts two sets of file results together without repeating anything.
///
/// Ours first and in its own order, because it ranks with the same code as
/// every other row and a whole-volume indexer has its own idea of relevance
/// that does not agree. Theirs fills the rest, which is where anything outside
/// the indexed folders comes from.
///
/// Paths are compared case-insensitively: Windows does, and the same file
/// arriving from both sources under different capitalisation would otherwise
/// be listed twice.
pub fn merge(
    ours: Vec<files::FileHit>,
    theirs: Vec<files::FileHit>,
    limit: usize,
) -> Vec<files::FileHit> {
    let mut seen: std::collections::HashSet<String> =
        ours.iter().map(|hit| hit.path.to_lowercase()).collect();
    let mut out = ours;

    for hit in theirs {
        if out.len() >= limit {
            break;
        }

        if seen.insert(hit.path.to_lowercase()) {
            out.push(hit);
        }
    }

    out.truncate(limit);
    out
}

/// What is stopping file search from answering, if anything.
///
/// Asked when the launcher is summoned, not per keystroke. The answer only
/// changes when a program starts or stops, and rule 18 is about not paying for
/// answers nothing asked a new question about.
///
/// Returns nothing when file search is switched off, because then there is no
/// problem to report: somebody turned it off on purpose.
#[tauri::command]
pub(crate) async fn file_search_missing(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
) -> Result<Option<files::Missing>, String> {
    let enabled = state.inner.lock().await.files.enabled;

    Ok(files::missing(enabled, catalog.inner.load().len(), busy(&catalog)))
}

/// Whether the index is being rebuilt right now.
fn busy(catalog: &CatalogState) -> bool {
    catalog.building.load(std::sync::atomic::Ordering::Acquire)
}

/// Does whatever the thing standing in the way needs.
///
/// One command rather than two, because the launcher offers one row and the
/// row does the right thing. Which of the two it is was already decided by
/// [`files::missing`], and asking again here keeps the decision in one place.
///
/// The install runs in a console window somebody can see. A package manager
/// asks about agreements and can fail on a network, and a launcher that
/// swallowed all of that and reported nothing would be worse than one that
/// shows the same output a person would have seen typing it themselves.
#[tauri::command]
pub(crate) async fn start_file_search(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
) -> Result<String, String> {
    let enabled = state.inner.lock().await.files.enabled;
    let indexed = catalog.inner.load().len();

    match files::missing(enabled, indexed, busy(&catalog)) {
        Some(files::Missing::Indexing) => Ok("Still reading your files.".to_string()),
        None => Ok("File search is already working.".to_string()),
        Some(files::Missing::Asleep) => {
            files::start().map(|()| "Starting file search.".to_string())
        }
        Some(files::Missing::Absent) => {
            files::install().map(|()| "Installing file search.".to_string())
        }
    }
}

/// Every mounted drive, and whether Sill is indexing it.
///
/// Asked when the settings that show them are opened, never on a timer. A
/// drive appearing is something a person did, and they are looking at the
/// list when they do it.
#[tauri::command]
pub(crate) async fn list_drives(
    state: State<'_, PrefsState>,
) -> Result<Vec<crate::catalog::Drive>, String> {
    let roots = state.inner.lock().await.files.indexed_roots();

    Ok(crate::catalog::drives(&roots))
}

/// Starts or stops indexing one folder, and rebuilds either way.
///
/// One command for both directions because the settings offer one switch. What
/// it does is decided by what the folder is now, not by what the window
/// believed when it drew itself.
#[tauri::command]
pub(crate) async fn index_folder(
    state: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
    path: String,
    wanted: bool,
) -> Result<Vec<String>, String> {
    let path = path.trim().to_string();
    if path.is_empty() {
        return Err("No folder given.".to_string());
    }

    let roots = {
        let mut prefs = state.inner.lock().await;

        // Written into the list as it was given, but compared without case or
        // trailing separators, since `C:/` and `C:\` are the same folder and
        // adding both would index it twice.
        let already = prefs
            .files
            .roots
            .iter()
            .position(|root| crate::catalog::same_folder(root, &path));

        match (wanted, already) {
            (true, None) => prefs.files.roots.push(path),
            (false, Some(at)) => {
                prefs.files.roots.remove(at);
            }
            // Already as asked. Falling through would rebuild for nothing.
            _ => return Ok(prefs.files.roots.clone()),
        }

        // Empty means the home folder, which is not the same as indexing
        // nothing. Somebody who removes their last root means to stop, so it
        // is written down rather than left to be read as the default.
        if prefs.files.roots.is_empty() {
            prefs.files.index = false;
        } else {
            prefs.files.index = true;
        }

        // Reported rather than dropped. A change that cannot be written down
        // comes back on the next start, and silently indexing a folder
        // somebody removed is worse than saying the save failed.
        prefs
            .save(&state.path)
            .map_err(|err| format!("Could not save: {err}"))?;

        prefs.files.clone()
    };

    catalog.rebuild(roots.indexed_roots());

    Ok(roots.roots)
}

/// Opens a file or folder in its default application.
#[tauri::command]
pub(crate) async fn open_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}
