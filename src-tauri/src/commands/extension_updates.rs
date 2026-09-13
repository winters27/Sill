//! Keeping installed extensions current, and saying so on the row.
//!
//! Transport and orchestration. Which extensions are behind, what an update is
//! allowed to do without asking, and where the answer is kept all live in
//! [`crate::store::updates`]; this is the layer that owns the seams that module
//! deliberately does not reach through: the preferences, the services, the
//! staging lock and the event.
//!
//! ## The two presses that became one
//!
//! An update is an install at a newer commit, so it goes through the same two
//! steps: fetch and read, then show what the code reaches, then build. The
//! second step exists because **a version can gain the ability to run programs
//! that somebody would otherwise have waved through**, and skipping it to save
//! a keypress would be selling exactly the thing the screen was built to
//! protect.
//!
//! So the screen is not skipped, it is **answered in advance**.
//! [`crate::store::Origin::capabilities`] records what was agreed to at install
//! time. If the new version reaches nothing that is not already on that list,
//! there is no question to put to anybody and the update applies straight
//! through. If it reaches something new, the update stops and says so, and that
//! one extension is finished in the store where the screen can show what grew.
//!
//! That is what makes one press honest rather than merely short.
//!
//! ## Why applying does not open the catalogue
//!
//! Everything a fetch needs is recorded by the install or established by the
//! check, so [`crate::store::Listing::to_update`] assembles it from what is
//! already here. Reaching for the catalogue would put seven requests and 3.8 MB
//! behind a keypress whose whole promise is that it is one keypress.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::PrefsState;
use crate::store::{
    self, catalog, install,
    updates::{self, Behind, Doing, ExtensionUpdates, Standing},
};

/// What the row reads, and the one name both sides spell it by.
pub const UPDATES_CHANGED: &str = "sill://extension-updates";

/// Tells every window, when there is something new to tell them.
///
/// A plain `emit` rather than one scoped to a label. The scoping rule is for
/// messages about one window; this is a fact about the machine, and the
/// launcher and the settings window both want it.
fn announce(app: &AppHandle, standing: Standing) {
    if let Err(err) = app.emit(UPDATES_CHANGED, standing) {
        crate::say!("could not say what extensions are out of date: {err}");
    }
}

/// The standing right now, for a window that has just opened.
///
/// Reads the file on the first call of a run, which is what lets the row appear
/// on the first summon after a restart without a single request.
#[tauri::command]
pub(crate) async fn extension_updates(
    app: AppHandle,
    updates: State<'_, ExtensionUpdates>,
) -> Result<Standing, String> {
    Ok(updates.read(&crate::state::data_dir(&app)))
}

/// Asks the store whether anything installed here is behind.
///
/// Called on every summon and almost always returns having done nothing: the
/// answer is good for [`updates::ASK_AGAIN_AFTER`], and a machine with nothing
/// installed from the store never reaches the network at all.
///
/// Answers nothing, like [`crate::commands::update::check_for_update`]. What it
/// finds goes out as an event, because two windows want it and one of them may
/// not exist yet.
#[tauri::command]
pub(crate) async fn check_extension_updates(app: AppHandle, force: bool) {
    check(app, force).await;
}

/// The check itself, so the summon path and the auto-apply path share one.
pub async fn check(app: AppHandle, force: bool) {
    let data_dir = crate::state::data_dir(&app);
    let now = crate::state::now_seconds();

    let (check_on, auto_on) = {
        let held = app.state::<PrefsState>().inner.clone();
        let prefs = held.lock().await;
        (prefs.store.check_updates, prefs.store.auto_update)
    };

    // The switch is read before the throttle so that turning it off is
    // immediate rather than taking effect after the next check that was
    // already due.
    if !check_on && !force {
        return;
    }

    if !app
        .state::<ExtensionUpdates>()
        .worth_asking(&data_dir, now, force)
    {
        return;
    }

    let home = store::extensions_home(&data_dir);
    let installed = store::installed(&home);

    // The cached catalogue, if there is one, and never a fetch. It is only
    // consulted for the handle of an extension installed before the handle was
    // recorded, and fetching 3.8 MB to answer that would be the whole reason
    // this module exists, undone.
    let cached = catalog::read_cache(&catalog::cache_path(&data_dir));
    let asking = updates::askable(&installed, |slug| {
        cached.as_ref().and_then(|catalog| {
            catalog
                .listings
                .iter()
                .find(|listing| listing.name == slug)
                .map(|listing| (listing.author.clone(), listing.folder.clone()))
        })
    });

    // Nothing installed from the store. No socket is opened, and the timestamp
    // still moves so this is not re-derived on every summon.
    if asking.is_empty() {
        let updates = app.state::<ExtensionUpdates>();
        if updates.record(&data_dir, Vec::new(), now) {
            announce(&app, updates.read(&data_dir));
        }
        return;
    }

    if app.state::<ExtensionUpdates>().set_doing(Doing::Checking) {
        // Deliberately not announced. A check that finds nothing must leave no
        // trace on the row, and one that finds something is drawn as what it
        // found rather than as the looking.
    }

    let mut behind = match ask_the_store(&app, &data_dir, &asking).await {
        Ok(behind) => behind,
        Err(err) => {
            crate::say!("could not check for extension updates: {err}");
            app.state::<ExtensionUpdates>().set_doing(Doing::Nothing);
            return;
        }
    };

    titled(&home, &mut behind);

    let updates = app.state::<ExtensionUpdates>();
    updates.set_doing(Doing::Nothing);

    let found = !behind.is_empty();
    if updates.record(&data_dir, behind, now) {
        announce(&app, updates.read(&data_dir));
    }

    if auto_on && found {
        apply(app, None).await;
    }
}

/// Asks about each extension, or about the whole catalogue when there are
/// enough of them that the catalogue is cheaper.
///
/// Sequential, for the reason [`catalog::fetch`] takes its seven pages one at a
/// time: firing a batch at somebody else's index saves a few seconds and reads
/// as a scrape.
async fn ask_the_store(
    app: &AppHandle,
    data_dir: &std::path::Path,
    asking: &[updates::Askable],
) -> Result<Vec<Behind>, String> {
    if asking.len() > updates::ASK_EACH_UP_TO {
        return from_catalog(app, data_dir, asking).await;
    }

    let client = crate::dictation::fetch::client();
    let mut behind = Vec::new();

    for one in asking {
        let published = updates::published(&client, &one.author, &one.listing).await?;

        match updates::compare(&one.origin, published.as_deref()) {
            updates::Asked::Newer(to) => behind.push(found(one, to)),
            updates::Asked::Current => {}
            updates::Asked::Unlisted => {
                crate::say!(
                    "the store no longer lists {} under {}, so it will not be updated",
                    one.listing,
                    one.author
                );
            }
        }
    }

    Ok(behind)
}

/// The same answer, read out of a refreshed catalogue.
///
/// Past [`updates::ASK_EACH_UP_TO`] installs this is the cheaper of the two,
/// and it has a second virtue: the store's own copy is refreshed by it, so
/// somebody with a lot of extensions opens the store to a current shelf.
async fn from_catalog(
    app: &AppHandle,
    data_dir: &std::path::Path,
    asking: &[updates::Askable],
) -> Result<Vec<Behind>, String> {
    let fetched = catalog::load(data_dir, true).await?;

    // The store holds whatever it is given, so a refresh made here is not
    // thrown away and then paid for again when somebody opens the store.
    app.state::<store::StoreState>()
        .hold(std::sync::Arc::new(fetched.clone()));

    Ok(asking
        .iter()
        .filter_map(|one| {
            let listing = fetched
                .listings
                .iter()
                .find(|listing| listing.name == one.listing)?;

            match updates::compare(&one.origin, Some(&listing.revision)) {
                updates::Asked::Newer(to) => Some(found(one, to)),
                _ => None,
            }
        })
        .collect())
}

/// One answer, shaped for the row.
fn found(one: &updates::Askable, to: String) -> Behind {
    Behind {
        extension: one.directory.clone(),
        listing: one.listing.clone(),
        author: one.author.clone(),
        folder: one.folder.clone(),
        // A placeholder until [`titled`] runs. A directory name is a slug and
        // reads like one, and this string ends up in a sentence somebody reads.
        title: one.directory.clone(),
        from: one.origin.revision.clone(),
        to,
    }
}

/// Gives each answer the name the extension calls itself.
///
/// Out of the index rather than out of the catalogue, because the index is a
/// file that is already on this machine and describes what is installed on it.
/// The catalogue would be a better-looking title fetched over the network, for
/// something that is already here.
///
/// Read once for the whole list rather than per extension: the index holds one
/// record per command, so an extension with nine commands would otherwise be
/// nine scans of the same file.
fn titled(home: &std::path::Path, behind: &mut [Behind]) {
    if behind.is_empty() {
        return;
    }

    let index = crate::registry::load_index(&store::index_file(home));

    for one in behind.iter_mut() {
        if let Some(record) = index.iter().find(|it| it.extension == one.extension) {
            if !record.extension_title.is_empty() {
                one.title = record.extension_title.clone();
            }
        }
    }
}

/// Applies what can be applied without asking anybody anything.
///
/// `extension` names one, or nothing for every one that is behind. Answers
/// immediately and reports through [`UPDATES_CHANGED`]: npm and esbuild are
/// tens of seconds per extension and the launcher will be long gone.
#[tauri::command]
pub(crate) async fn apply_extension_updates(
    app: AppHandle,
    extension: Option<String>,
) -> Result<(), String> {
    apply(app, extension).await;
    Ok(())
}

async fn apply(app: AppHandle, only: Option<String>) {
    let data_dir = crate::state::data_dir(&app);

    let wanted: Vec<Behind> = app
        .state::<ExtensionUpdates>()
        .read(&data_dir)
        .behind
        .into_iter()
        .filter(|it| only.as_ref().is_none_or(|name| *name == it.extension))
        .collect();

    if wanted.is_empty() {
        return;
    }

    // **The lock, and the reason it exists.** `install::prepare` opens by
    // deleting the whole staging directory. Starting a batch while somebody is
    // deciding on an install in the store would delete the source they are
    // looking at; starting one while another batch is between its two steps
    // would delete the tree npm is running inside.
    let store_state = app.state::<store::StoreState>();
    if let Err(holder) = store_state.hold_staging(store::Staging::Background) {
        crate::say!("not applying extension updates: {holder:?} is already installing");
        return;
    }

    let updates = app.state::<ExtensionUpdates>();
    updates.start_batch();

    let total = wanted.len();
    for (at, one) in wanted.iter().enumerate() {
        updates.set_doing(Doing::Applying {
            title: one.title.clone(),
            done: at + 1,
            total,
        });
        announce(&app, updates.read(&data_dir));

        match apply_one(&app, &data_dir, one).await {
            Ok(true) => updates.applied(&data_dir, &one.extension),
            Ok(false) => updates.needs_a_look(&one.title),
            Err(err) => {
                crate::say!("could not update {}: {err}", one.extension);
                updates.did_not_work(&one.title);
            }
        }
    }

    store_state.release_staging(store::Staging::Background);
    install::discard(&data_dir);

    updates.set_doing(Doing::Nothing);

    // **Once, after the batch.** `reload_index` is a full scan of the machine,
    // a PowerShell round trip and every Start Menu shortcut on it. One per
    // applied extension would be that, five times, to learn something that was
    // already true after the first.
    crate::reload_index(&app);

    announce(&app, updates.read(&data_dir));
}

/// One update. `Ok(false)` means it needs somebody to look at it.
async fn apply_one(
    app: &AppHandle,
    data_dir: &std::path::Path,
    one: &Behind,
) -> Result<bool, String> {
    let listing = store::Listing::to_update(
        &one.listing,
        &one.folder,
        &one.author,
        &one.to,
        &one.title,
    );

    let token = {
        let held = app.state::<PrefsState>().inner.clone();
        let prefs = held.lock().await;
        prefs
            .store
            .github_token
            .clone()
            .filter(|it| !it.trim().is_empty())
    };

    // **Reported, not discarded.** This is the same event the store's own
    // install reports on, so the bar and the line that draw it already exist
    // and the launcher row reuses both. Throwing it away is what made pressing
    // the row look like the launcher had died: the work was real and running,
    // and nothing on screen said so.
    let reporting = app.clone();
    let say = move |progress: crate::extension_install::Progress| {
        if let Err(err) = reporting.emit(crate::commands::store::INSTALL_PROGRESS, &progress) {
            crate::say!("could not say how the update is going: {err}");
        }
    };

    // Step one. Downloads and reads; nothing is executed and npm has not run.
    let prepared = install::prepare(data_dir, &listing, token.as_deref(), &say).await?;

    let home = store::extensions_home(data_dir);
    let granted = store::origin_of(&home, &one.extension)
        .map(|origin| origin.capabilities)
        .unwrap_or_default();

    let reaches: Vec<String> = prepared
        .capabilities
        .iter()
        .map(|it| it.id.clone())
        .collect();

    let new_asks = updates::newly_asks(&granted, &reaches);
    if !new_asks.is_empty() {
        crate::say!(
            "{} now reaches {} and was not updated on its own",
            one.extension,
            new_asks.join(", ")
        );
        // Discarded rather than left staged. The next extension in this batch
        // would delete it anyway, and the store re-fetches when somebody opens
        // the screen that can show what grew.
        install::discard(data_dir);
        return Ok(false);
    }

    // **Put it down before replacing what it runs out of.** A live worker left
    // on the old bundle keeps drawing with its assets gone, which is a command
    // that looks fine and no longer works.
    quiesce(app, &one.extension).await;

    let esbuild =
        crate::extension_install::esbuild_exe(app).ok_or(crate::extension_install::NO_ESBUILD)?;
    let node = crate::host::node_exe(
        &app.state::<crate::state::HostState>().node,
        crate::host::bundled_node(app),
    )
    .ok_or(crate::host::NO_NODE)?;

    // npm is a subprocess of seconds and esbuild is one per command, so it goes
    // off the async runtime rather than holding it for the length of a build.
    let owned = data_dir.to_path_buf();
    let name = one.listing.clone();
    let done = tauri::async_runtime::spawn_blocking(move || {
        install::finish_reporting(&owned, &esbuild, &node, &name, &say)
    })
    .await
    .map_err(|err| format!("the update did not finish: {err}"))??;

    // The same join an install makes: what was recorded is what gets granted.
    // A subset of what was already held, by the check above, so this is
    // normally a no-op and is here so that it cannot quietly stop being one.
    let granting = store::capability::granted_by(&done.capabilities);
    if !granting.is_empty() {
        app.state::<std::sync::Arc<crate::exthost::grants::Granted>>()
            .grant(&done.installed.extension, &granting);
    }

    Ok(true)
}

/// Unloads every session of one extension, if the host is even running.
///
/// Best effort on purpose. A session that will not unload is not a reason to
/// refuse an update: the swap leaves the previous version in place when it
/// cannot proceed, so the worst case is an update that does not happen rather
/// than an extension that is damaged.
async fn quiesce(app: &AppHandle, extension: &str) {
    let host = {
        let held = app.state::<crate::state::HostState>().inner.clone();
        let held = held.lock().await;
        held.clone()
    };

    let Some(host) = host else {
        return;
    };

    for session in host.session_ids_of(extension) {
        if let Err(err) = host.unload(&session).await {
            crate::say!("could not put {extension} down before updating it: {err}");
        }
    }
}
