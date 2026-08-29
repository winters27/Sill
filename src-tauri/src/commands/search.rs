//! Searching, and opening what was found.

use tauri::State;

use crate::state::{now_seconds, PrefsState, RegistryState};
use crate::{calculator, files, registry};

/// The root list, or what matches a query.
#[tauri::command]
pub(crate) async fn search_commands(
    state: State<'_, RegistryState>,
    prefs: State<'_, PrefsState>,
    query: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let excluded = prefs.inner.lock().await.sources.excluded.clone();
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
        now_seconds(),
        registry::SEARCH_LIMIT,
        &excluded,
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
    Ok(results.into_iter().map(Into::into).collect())
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
