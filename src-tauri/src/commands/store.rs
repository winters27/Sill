//! Browsing and installing from the extension store.
//!
//! Transport and nothing else. Everything about how the catalogue is fetched,
//! filtered, ranked and installed is in [`crate::store`]; these deserialise,
//! get the service, call it, and hand back something the window can draw.
//!
//! One command per thing somebody does, and a browse answers with the rows,
//! the categories, the counts and what is already installed in a single reply.
//! The alternative is the window asking four questions per keystroke, which is
//! the chatter rule 18 exists to stop.

use tauri::{AppHandle, State};

use crate::state::PrefsState;
use crate::store::{self, catalog, install, Browse, Query};

/// The GitHub token, if one has been set.
///
/// Read per call rather than held: it is one short string out of a lock that
/// is taken anyway, and caching a credential in a second place is how one ends
/// up outliving the setting that removed it.
async fn token_of(prefs: &PrefsState) -> Option<String> {
    prefs
        .inner
        .lock()
        .await
        .store
        .github_token
        .clone()
        .filter(|it| !it.trim().is_empty())
}

/// The catalogue, held if it is already held and fetched if it is not.
///
/// `refresh` is the store's own refresh action. Without it this never reaches
/// the network while the store is open, which is what makes typing in the
/// store cost nothing.
async fn catalog_of(
    app: &AppHandle,
    state: &store::StoreState,
    refresh: bool,
) -> Result<catalog::Catalog, String> {
    if !refresh {
        if let Some(held) = state.held() {
            return Ok(held);
        }
    }

    let fetched = catalog::load(&crate::state::data_dir(app), refresh).await?;
    state.hold(fetched.clone());
    Ok(fetched)
}

/// Everything one screen of the store needs.
#[tauri::command]
pub(crate) async fn store_browse(
    app: AppHandle,
    state: State<'_, store::StoreState>,
    query: Query,
    refresh: bool,
) -> Result<Browse, String> {
    let catalog = catalog_of(&app, &state, refresh).await?;

    // Read once per browse rather than once per row. Three thousand directory
    // probes on a keystroke is not a keystroke budget.
    let pins = store::pins(&store::extensions_home(&crate::state::data_dir(&app)));

    Ok(store::browse(
        &catalog.listings,
        |name: &str| pins.get(name).cloned(),
        &query,
        catalog.fetched_at,
    ))
}

/// Lets go of the catalogue, which is what closing the store does.
///
/// Its own command because the window is the only thing that knows the store
/// has been left. Two megabytes of somebody else's product listings must not
/// outlive the view that asked for them.
#[tauri::command]
pub(crate) async fn store_close(state: State<'_, store::StoreState>) -> Result<(), String> {
    state.forget();
    Ok(())
}

/// Step one of an install: fetch the source and report what it appears to do.
///
/// Nothing is executed here and nothing is installed. What comes back is shown
/// to the person deciding, and [`store_install`] is what happens if they say
/// yes.
#[tauri::command]
pub(crate) async fn store_prepare(
    app: AppHandle,
    state: State<'_, store::StoreState>,
    prefs: State<'_, PrefsState>,
    name: String,
) -> Result<install::Preparation, String> {
    let catalog = catalog_of(&app, &state, false).await?;

    let listing = catalog
        .listings
        .iter()
        .find(|listing| listing.name == name)
        .ok_or_else(|| format!("the store has no extension called {name}"))?
        .clone();

    let token = token_of(&prefs).await;

    install::prepare(&crate::state::data_dir(&app), &listing, token.as_deref()).await
}

/// Step two: install what was prepared.
///
/// Also what an update is. An update is this whole path at the newer commit,
/// including the screen that says what the new version reaches, because an
/// extension can gain the ability to run programs in a version somebody would
/// otherwise have accepted without looking.
#[cfg(windows)]
#[tauri::command]
pub(crate) async fn store_install(
    app: AppHandle,
    name: String,
) -> Result<install::Done, String> {
    let esbuild = crate::extension_install::esbuild_exe(&app)
        .ok_or(crate::extension_install::NO_ESBUILD)?;
    let data_dir = crate::state::data_dir(&app);

    // Off the UI thread: npm is a subprocess that takes seconds and esbuild is
    // one more per command.
    let done =
        tauri::async_runtime::spawn_blocking(move || install::finish(&data_dir, &esbuild, &name))
            .await
            .map_err(|err| format!("the install did not finish: {err}"))??;

    // The new commands are in the index file and nothing has read it yet.
    crate::reload_index(&app);

    Ok(done)
}

/// Throws away a prepared install nobody accepted.
#[tauri::command]
pub(crate) async fn store_discard(app: AppHandle) -> Result<(), String> {
    install::discard(&crate::state::data_dir(&app));
    Ok(())
}

/// Removes an installed extension.
#[tauri::command]
pub(crate) async fn store_uninstall(app: AppHandle, extension: String) -> Result<bool, String> {
    let handle = app.clone();

    let data_dir = crate::state::data_dir(&handle);

    let had =
        tauri::async_runtime::spawn_blocking(move || install::uninstall(&data_dir, &extension))
            .await
            .map_err(|err| format!("the removal did not finish: {err}"))??;

    crate::reload_index(&app);

    Ok(had)
}

/// What is installed and where each one came from.
///
/// For the settings panel, which shows provenance rather than a store. It
/// reaches no network at all: pins are small files beside the bundles, so a
/// panel that opens costs a directory listing.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Pinned {
    pub extension: String,
    pub source: String,
    pub revision: String,
    pub path: String,
    pub installed_at: i64,
}

#[tauri::command]
pub(crate) async fn store_pins(app: AppHandle) -> Result<Vec<Pinned>, String> {
    let home = store::extensions_home(&crate::state::data_dir(&app));

    let mut pinned: Vec<Pinned> = store::pins(&home)
        .into_iter()
        .map(|(name, origin)| Pinned {
            extension: name,
            source: origin.source,
            revision: origin.revision,
            path: origin.path,
            installed_at: origin.installed_at,
        })
        .collect();

    pinned.sort_by(|a, b| a.extension.cmp(&b.extension));

    Ok(pinned)
}

/// Whether this machine can install an extension at all.
///
/// Asked live, for the reason the settings panel asks live: somebody can
/// install Node while Sill is open, and the store is exactly where they would
/// try again afterwards. Named once here so the store does not reason about
/// Node in two places.
#[tauri::command]
pub(crate) async fn store_ready() -> Result<bool, String> {
    Ok(crate::host::node_exe().is_some())
}
