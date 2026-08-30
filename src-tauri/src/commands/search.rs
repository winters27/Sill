//! Searching, and opening what was found.

use tauri::State;

use crate::state::{now_seconds, PrefsState, RegistryState};
use crate::{calculator, files, registry, windowing};

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

/// Files matching a query, from Everything.
///
/// Separate from `search_commands` rather than merged into it: this spawns a
/// process, so the UI debounces it and lets command results appear first.
#[tauri::command]
pub(crate) async fn search_files(
    state: State<'_, PrefsState>,
    query: String,
) -> Result<Vec<files::FileHit>, String> {
    let settings = state.inner.lock().await.files.clone();

    if !settings.enabled {
        return Ok(Vec::new());
    }

    let query = files::scope(&query, &settings.only_in);

    let hits = tokio::task::spawn_blocking(move || {
        files::search_with(
            &query,
            settings.max_results as usize,
            settings.match_path,
            settings.match_case,
            settings.regex,
        )
    })
    .await
    .unwrap_or_default();

    Ok(hits)
}

/// Opens a file or folder in its default application.
#[tauri::command]
pub(crate) async fn open_path(path: String) -> Result<(), String> {
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| e.to_string())
}
