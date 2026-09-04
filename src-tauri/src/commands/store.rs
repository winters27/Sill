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

use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::PrefsState;
use crate::store::{self, catalog, install, Browse, Query};

/// What an install says about itself while it runs.
///
/// One name, here, because the window listens for it and Rust emits it, and a
/// string spelled twice is the pair that stops agreeing.
pub const INSTALL_PROGRESS: &str = "store:install";

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
) -> Result<std::sync::Arc<catalog::Catalog>, String> {
    if !refresh {
        if let Some(held) = state.held() {
            return Ok(held);
        }
    }

    // Read once, shared from here on. The `Arc` is made before anything else
    // sees it, so the catalogue is never copied: not to hold it, and not to
    // answer with it.
    let fetched = std::sync::Arc::new(catalog::load(&crate::state::data_dir(app), refresh).await?);
    state.hold(fetched.clone());
    start_idle_watchdog(app.clone());
    Ok(fetched)
}

/// Drops the catalogue once nothing has reached for it in a while.
///
/// Started when a catalogue is first held and returns when it fires, so a
/// machine that never opens the store never runs this timer at all. The same
/// shape as [`crate::host::start_host_watchdog`], which is the pattern this
/// codebase already settled on for "keep it warm, but not forever".
///
/// Guarded so two opens do not start two of them.
fn start_idle_watchdog(app: AppHandle) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WATCHING: AtomicBool = AtomicBool::new(false);

    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(store::IDLE_CHECK).await;

            let state = app.state::<store::StoreState>();

            let Some(idle) = state.idle_for() else {
                // Somebody let go of it already. Nothing left to watch.
                WATCHING.store(false, Ordering::SeqCst);
                return;
            };

            if idle < store::IDLE_TIMEOUT {
                continue;
            }

            if state.forget() {
                crate::say!(
                    "store catalogue idle for {}s; letting it go",
                    idle.as_secs()
                );
            }

            WATCHING.store(false, Ordering::SeqCst);
            return;
        }
    });
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
    let home = store::extensions_home(&crate::state::data_dir(&app));
    let pins = store::pins(&home);

    Ok(store::browse(
        &catalog.listings,
        &installed_but_unlisted(&home, &catalog.listings, &pins),
        |name: &str| pins.get(name).cloned(),
        &query,
        catalog.fetched_at,
    ))
}

/// Listings for what is installed here that Raycast's index does not carry.
///
/// An extension built from a folder is not in the catalogue and never will be,
/// and one installed from the store can be withdrawn from it afterwards.
/// Browsing ran over the catalogue alone, so both were **absent from the
/// Installed tab of the screen whose job is managing installed extensions**
/// while running perfectly well in the launcher.
///
/// Built from the index Sill wrote at install time rather than from a manifest
/// on disk, because the index is what the launcher itself runs from: a row
/// here and a row in the launcher describe the same thing by construction.
fn installed_but_unlisted(
    home: &std::path::Path,
    listings: &[store::Listing],
    pins: &std::collections::HashMap<String, store::Origin>,
) -> Vec<store::Listing> {
    use std::collections::HashMap;

    let known: std::collections::HashSet<&str> =
        listings.iter().map(|it| it.name.as_str()).collect();

    let mut commands: HashMap<String, Vec<store::ListedCommand>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for record in crate::registry::load_index(&store::index_file(home)) {
        if known.contains(record.extension.as_str()) {
            continue;
        }

        if !commands.contains_key(&record.extension) {
            order.push(record.extension.clone());
        }

        commands
            .entry(record.extension.clone())
            .or_default()
            .push(store::ListedCommand {
                name: record.command,
                title: record.title,
                description: record.subtitle,
                mode: record.mode,
            });
    }

    order
        .into_iter()
        .map(|extension| {
            let origin = pins.get(&extension);
            store::Listing::of_installed(
                &extension,
                &extension,
                origin.map(|it| it.revision.as_str()).unwrap_or_default(),
                commands.remove(&extension).unwrap_or_default(),
            )
        })
        .collect()
}

/// Says the store has been left.
///
/// **It parks the catalogue rather than dropping it.** Dropping it here was
/// the first version and it is what made reopening the store feel like opening
/// it for the first time: leaving and coming back a few seconds later paid for
/// a megabyte and a half of JSON to be read and parsed again.
///
/// The clock is what decides now. Closing stops it being touched, and the
/// watchdog lets go five minutes later if nobody comes back, so a launcher
/// sitting idle overnight is still holding nothing.
#[tauri::command]
pub(crate) async fn store_close(_state: State<'_, store::StoreState>) -> Result<(), String> {
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
    grants: State<'_, std::sync::Arc<crate::exthost::grants::Granted>>,
    name: String,
) -> Result<install::Done, String> {
    let esbuild =
        crate::extension_install::esbuild_exe(&app).ok_or(crate::extension_install::NO_ESBUILD)?;
    let data_dir = crate::state::data_dir(&app);

    // Found here rather than inside the install, for the same reason esbuild
    // is: finding Node means running it, and this is the layer that holds the
    // answer.
    let node = crate::host::node_exe(&app.state::<crate::state::HostState>().node)
        .ok_or(crate::host::NO_NODE)?;

    // Off the UI thread: npm is a subprocess that takes seconds and esbuild is
    // one more per command.
    //
    // The progress goes out as an event rather than coming back with the
    // result, because it is a series of things happening rather than an
    // answer. Rule 6, and the same shape `dictation::SetupProgress` already
    // uses for the other download somebody waits on.
    let reporting = app.clone();
    let done = tauri::async_runtime::spawn_blocking(move || {
        install::finish_reporting(&data_dir, &esbuild, &node, &name, &|progress| {
            if let Err(err) = reporting.emit(INSTALL_PROGRESS, &progress) {
                crate::say!("could not say how the install is going: {err}");
            }
        })
    })
    .await
    .map_err(|err| format!("the install did not finish: {err}"))??;

    // **The join.** What the screen showed is what gets granted, keyed by the
    // extension's own name because that is what the worker asks about.
    //
    // Done here rather than in `finish`, which takes paths and knows nothing
    // about services. Anything not on this list is still asked for on a card
    // the first time it happens.
    let granting = crate::store::capability::granted_by(&done.capabilities);
    if !granting.is_empty() {
        grants.grant(&done.installed.extension, &granting);
        crate::say!(
            "granted {} to {}: {}",
            granting.len(),
            done.installed.extension,
            granting
                .iter()
                .map(crate::exthost::permission::plainly)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

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

/// Removes an installed extension, and everything it was allowed to reach.
///
/// Transport, like everything else in this file. The removal itself is
/// `sill.store.remove` in the action registry, and going through
/// [`crate::action::ActionRegistry::perform`] is what puts it in the activity
/// log: it used to do the work here, so an extension removed from the settings
/// panel or the store's own key appeared nowhere afterwards.
#[tauri::command]
pub(crate) async fn store_uninstall(app: AppHandle, extension: String) -> Result<String, String> {
    let object = crate::object::Object {
        kind: crate::object::ObjectKind::StoreListing,
        id: format!("store:{extension}"),
        target: extension.clone(),
        title: extension,
        mode: "store-listing".to_string(),
    };

    let registry = app.state::<crate::action::ActionRegistry>();
    let action = registry
        .get("sill.store.remove")
        .ok_or("removing an extension is not available")?;

    let outcome = registry
        .perform(
            &crate::action::ActionCtx::new(app.clone()),
            action.as_ref(),
            &object,
        )
        .await?;

    Ok(outcome.message)
}

// ------------------------------------------------------- installed, in full

/// One command an installed extension contributes.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstalledCommand {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub mode: String,
    /// Whether Sill can run it, asked of the type that decides.
    pub runnable: bool,
}

/// One permission, and whether this extension has it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionState {
    pub capability: crate::action::Capability,
    /// What it lets the extension do, in the words the card uses.
    pub plainly: &'static str,
    pub granted: bool,
}

/// Everything the settings screen needs about one installed extension.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Installed {
    pub extension: String,
    pub title: String,
    pub commands: Vec<InstalledCommand>,
    /// `store`, `folder`, or empty when nothing recorded it.
    pub source: String,
    pub revision: String,
    pub path: String,
    pub installed_at: i64,
    pub permissions: Vec<PermissionState>,
}

/// What is installed, what it can run, and what it is allowed to reach.
///
/// One call rather than four. The screen needs the commands, the provenance
/// and the permissions together, and asking separately would be three
/// round trips per extension for a panel that is opened deliberately.
///
/// **It reaches no network.** The index is a file, the origins are files
/// beside the bundles, and the grants are held in memory. Opening settings
/// must not fetch a catalogue.
#[tauri::command]
pub(crate) async fn installed_extensions(
    app: AppHandle,
    grants: State<'_, std::sync::Arc<crate::exthost::grants::Granted>>,
) -> Result<Vec<Installed>, String> {
    let home = store::extensions_home(&crate::state::data_dir(&app));
    let index = crate::registry::load_index(&store::index_file(&home));

    // Grouped by extension, keeping the index's order inside each one.
    let mut order: Vec<String> = Vec::new();
    let mut by_extension: std::collections::HashMap<String, Vec<InstalledCommand>> =
        std::collections::HashMap::new();

    for record in index {
        // Only what a host would run. The same test `diagnostics` uses, so the
        // two panels cannot disagree about what counts as an extension.
        if crate::exthost::CommandMode::from_manifest(&record.mode).is_none()
            && record.mode != "menu-bar"
        {
            continue;
        }

        if !by_extension.contains_key(&record.extension) {
            order.push(record.extension.clone());
        }

        by_extension
            .entry(record.extension.clone())
            .or_default()
            .push(InstalledCommand {
                runnable: crate::exthost::CommandMode::from_manifest(&record.mode).is_some(),
                id: record.id,
                title: record.title,
                subtitle: record.subtitle,
                mode: record.mode,
            });
    }

    let offerable = crate::store::capability::grantable();

    Ok(order
        .into_iter()
        .map(|extension| {
            let held = grants.held(&extension);
            let origin = store::origin_of(&home, &extension);

            Installed {
                title: origin
                    .as_ref()
                    .map(|_| extension.clone())
                    .unwrap_or_else(|| extension.clone()),
                commands: by_extension.remove(&extension).unwrap_or_default(),
                source: origin
                    .as_ref()
                    .map(|it| it.source.clone())
                    .unwrap_or_default(),
                revision: origin
                    .as_ref()
                    .map(|it| it.revision.clone())
                    .unwrap_or_default(),
                path: origin
                    .as_ref()
                    .map(|it| it.path.clone())
                    .unwrap_or_default(),
                installed_at: origin.as_ref().map(|it| it.installed_at).unwrap_or(0),
                permissions: offerable
                    .iter()
                    .map(|capability| PermissionState {
                        capability: *capability,
                        plainly: crate::exthost::permission::plainly(capability),
                        granted: held.contains(capability),
                    })
                    .collect(),
                extension,
            }
        })
        .collect())
}

// ------------------------------------------------------------- preferences

/// One setting an extension declares, and what it is currently set to.
///
/// The declaration and the value together, in one row, because a screen that
/// draws a control needs both and asking twice would be two calls per setting.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExtensionPreference {
    /// Which command it belongs to, or empty when the extension declares it.
    ///
    /// The scope is the whole difference between one API key serving nine
    /// commands and nine of them, so it is shown rather than flattened away.
    pub command: String,
    /// What the command is called, for the heading the row sits under.
    pub command_title: String,
    pub name: String,
    /// `textfield`, `password`, `checkbox`, `dropdown`, or whatever else a
    /// manifest wrote. Unknown draws as a text field.
    pub kind: String,
    pub title: String,
    pub description: String,
    pub required: bool,
    /// The choices, for a dropdown.
    pub choices: Vec<crate::extension_install::Choice>,
    /// What it will answer with as things stand.
    pub value: serde_json::Value,
    /// Whether that came from the manifest rather than from somebody.
    pub is_default: bool,
}

/// Every setting one installed extension has, with what it is set to.
///
/// Read from the index rather than from the manifest, because the manifest is
/// in a staging directory that was deleted after the build. The declarations
/// were recorded at install for exactly this.
///
/// **A password is never sent.** Its value is sealed on disk and the row says
/// only whether one is set, because a settings window that can display an API
/// key is a settings window somebody can read over a shoulder.
#[tauri::command]
pub(crate) async fn extension_preferences(
    app: AppHandle,
    extension: String,
) -> Result<Vec<ExtensionPreference>, String> {
    let data_dir = crate::state::data_dir(&app);
    let home = store::extensions_home(&data_dir);
    let held = crate::exthost::preferences::load(&data_dir);

    let mut rows: Vec<ExtensionPreference> = Vec::new();

    for record in crate::registry::load_index(&store::index_file(&home)) {
        if record.extension != extension {
            continue;
        }

        let Some(declared) = record.manifest.as_ref() else {
            continue;
        };

        let effective = crate::exthost::preferences::effective(
            &record.preferences,
            held.in_scope(&crate::exthost::preferences::extension_scope(&extension)),
            held.in_scope(&crate::exthost::preferences::command_scope(
                &extension,
                &record.command,
            )),
        );

        for preference in &declared.preferences {
            // An extension's own setting is carried on every one of its
            // commands, so it gets one row rather than nine. A command's own
            // gets a row per command, which is what it is.
            let command = if declared.own.contains(&preference.name) {
                record.command.clone()
            } else {
                String::new()
            };

            if rows
                .iter()
                .any(|row| row.command == command && row.name == preference.name)
            {
                continue;
            }

            let set_here = held
                .in_scope(&if command.is_empty() {
                    crate::exthost::preferences::extension_scope(&extension)
                } else {
                    crate::exthost::preferences::command_scope(&extension, &command)
                })
                .and_then(|it| it.get(&preference.name))
                .is_some();

            let kind = preference
                .kind
                .clone()
                .unwrap_or_else(|| "textfield".into());
            let secret = kind == "password";

            rows.push(ExtensionPreference {
                command_title: if command.is_empty() {
                    String::new()
                } else {
                    record.title.clone()
                },
                command,
                name: preference.name.clone(),
                title: preference
                    .title
                    .clone()
                    .or_else(|| preference.label.clone())
                    .unwrap_or_else(|| preference.name.clone()),
                description: preference.description.clone().unwrap_or_default(),
                required: preference.required,
                choices: preference.data.clone(),
                // A secret is reported as set or not, never as itself.
                value: if secret {
                    serde_json::Value::Bool(set_here)
                } else {
                    effective
                        .get(&preference.name)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null)
                },
                is_default: !set_here,
                kind,
            });
        }
    }

    Ok(rows)
}

/// Sets one, or clears it when the value is empty.
///
/// Written straight through to disk rather than held. There is no in-memory
/// copy of this to keep in step, which is one fewer thing that can disagree,
/// and a settings screen that saves on change is what the rest of this window
/// already does.
#[tauri::command]
pub(crate) async fn set_extension_preference(
    app: AppHandle,
    extension: String,
    command: String,
    name: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let data_dir = crate::state::data_dir(&app);
    let home = store::extensions_home(&data_dir);

    // The declaration decides whether this is a secret, so it is looked up
    // rather than trusted from the window: a window that could say "this is
    // not a password" is a window that could ask for one to be stored in the
    // clear.
    let declared: Vec<crate::extension_install::Preference> =
        crate::registry::load_index(&store::index_file(&home))
            .into_iter()
            .filter(|record| record.extension == extension)
            .filter_map(|record| record.manifest)
            .flat_map(|declared| declared.preferences)
            .collect();

    if !declared.iter().any(|it| it.name == name) {
        return Err(format!("{extension} has no setting called {name}"));
    }

    let scope = if command.is_empty() {
        crate::exthost::preferences::extension_scope(&extension)
    } else {
        crate::exthost::preferences::command_scope(&extension, &command)
    };

    let mut held = crate::exthost::preferences::load(&data_dir);
    held.set(&scope, &name, value, &declared);
    crate::exthost::preferences::save(&data_dir, &held)
}

/// Gives one permission to one extension.
///
/// The other half of `revoke_extension_grant`, and the reason the refusal
/// message could not be acted on: it says "Grant it in Settings, under
/// Extensions" and until this existed there was nothing there that could.
///
/// It matters most for what the card cannot ask about. A permission needed at
/// `require` is needed while the module loads, which is synchronous and has no
/// RPC to hang a question on, so the extension dies before anything can be
/// offered. For those this is the only way in.
#[tauri::command]
pub(crate) async fn grant_extension_permission(
    grants: State<'_, std::sync::Arc<crate::exthost::grants::Granted>>,
    extension: String,
    capability: crate::action::Capability,
) -> Result<(), String> {
    grants.grant(&extension, &[capability]);
    Ok(())
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
pub(crate) async fn store_ready(
    host: tauri::State<'_, crate::state::HostState>,
) -> Result<bool, String> {
    Ok(crate::host::node_exe(&host.node).is_some())
}
