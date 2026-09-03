//! Running whatever the user picked.
//!
//! This was a two-hundred-line chain comparing an index entry's `mode` string
//! against eleven values. It is now a lookup: what kind of thing is this, what
//! does Enter do to that kind, do it. The behaviours themselves moved to
//! `crate::actions` unchanged.
//!
//! The point of the move is not tidiness. It is that pressing Enter, choosing
//! from the action panel, binding a shortcut and (later) a workflow step or a
//! tool an AI may call are now four ways into one implementation rather than
//! four implementations that drift.

use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::action::{ActionCtx, ActionRegistry, Outcome, Undo};
use crate::object::Object;
use crate::registry;
use crate::state::{now_seconds, CatalogState, PrefsState, RegistryState};

/// Runs a command from the root list.
///
/// Frecency is recorded before the action runs rather than after, so a command
/// that fails still counts as chosen. The user picked it; that is the signal
/// being learned, not whether it worked.
#[tauri::command]
pub(crate) async fn launch_command(
    app: AppHandle,
    state: State<'_, RegistryState>,
    id: String,
    // What was in the field when this was chosen, so Sill can learn the
    // user's own shorthand for it. Typing `ggm` and choosing Gmail says
    // something the id alone cannot: not "Gmail is popular" but "`ggm` means
    // Gmail". Optional, because a launch can come from places with no query.
    query: Option<String>,
) -> Result<LaunchedCommand, String> {
    // Everything a search could have offered, not just the index. A snippet,
    // a quicklink and Sill's own settings are all things the launcher shows
    // and none of them are in `commands`.
    let record = state
        .index()
        .everything()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| format!("no such command: {id}"))?;

    {
        let now = now_seconds();
        let query = query.clone();
        let id = id.clone();

        // Copy, change, swap. Nothing waits on this and nothing waits on the
        // write that follows it.
        let (path, text) = state.record(move |ranking| {
            ranking.frecency.record(&id, now);

            // The query as it was typed, not as it was matched. The shorthand
            // is the thing worth learning; the full name teaches nothing.
            if let Some(query) = query.as_deref() {
                ranking.frecency.record_query(query, &id, now);
                ranking.frecency.remember(query);
            }

            ranking.path.clone()
        });

        save_ranking_soon(&path, text);
    }

    let object = Object::from_record(&record)
        .ok_or_else(|| format!("{} is a kind of thing Sill cannot act on", record.title))?;

    let actions = app.state::<ActionRegistry>();
    let action = actions
        .primary(object.kind)
        .ok_or_else(|| format!("nothing is bound to Enter for {}", record.title))?;

    /*
     * The screen is about to belong to something else, so the launcher must
     * not put back what was in front before it.
     *
     * The rule `run_action` already follows, and it was missing here, which is
     * the older half of the same bug: dismissing restores the previous window,
     * so opening a quicklink or revealing a folder with Enter raced the
     * program it had just asked for. Launching an application survives that by
     * accident, because a new process takes the foreground after the restore
     * has run; anything that reuses a window already open loses every time.
     *
     * **Before the action, not after**, and that is the whole difference.
     * `restore_foreground` takes the handle rather than reading it, and an
     * action that dismisses the launcher itself has already spent it by the
     * time it returns. `run_action` gets away with forgetting afterwards only
     * because the actions it is used for leave the dismissal to the window.
     */
    if object.kind.hands_over_the_screen() {
        crate::summon::forget_foreground();
    }

    let outcome = actions
        .perform(&ActionCtx { app: app.clone() }, action, &object)
        .await?;

    /*
     * Where a switch ended up, so the row it was pressed on can show it.
     *
     * Read here rather than returned by the action because it is the
     * launcher's question, not the action's: the action's job was to flip the
     * thing. `ToggleSystem` has already dropped the cached reading, so this
     * sees the state after the change, and the reading it takes is the same
     * one the action's own dismiss decision used a moment earlier.
     */
    let toggle = if record.mode == "system" {
        crate::system::toggle_state(
            &record.entrypoint,
            &crate::system::live(&app.state::<crate::state::Fresh<crate::system::Live>>()),
        )
    } else {
        None
    };

    Ok(LaunchedCommand {
        // Empty rather than absent: the window has always read this as a
        // string and an extension command is the only thing that fills it.
        session: outcome.session.unwrap_or_default(),
        title: record.title,
        extension_title: record.extension_title,
        mode: record.mode,
        message: outcome.message,
        toggle,
    })
}

/// What the UI needs to show once a command is running.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LaunchedCommand {
    pub session: String,
    pub title: String,
    pub extension_title: String,
    /// "view" or "no-view"; the UI stays at the root list for no-view.
    pub mode: String,
    /// What the action said it did, in one line.
    pub message: String,
    /// Where a switch is now, for a row that stays on screen showing it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<bool>,
}

/**
What each built-in reaches, in the same vocabulary every other capability uses.

Its own function rather than a table inside the command, so a test can read it.
An empty slice means nothing is required, which is only ever right for a tag
the dispatch below refuses anyway; `verify:source` checks that every tag the
dispatch answers is named here, because two lists that must agree with nothing
making them agree is how the first version of this went wrong.
*/
pub fn builtin_needs(tag: &str) -> &'static [crate::action::Capability] {
    use crate::action::Capability::*;

    match tag {
        "Action.CopyToClipboard" => &[ClipboardWrite],
        "Action.OpenInBrowser" | "Action.Open" => &[ProcessLaunch],
        // Writes the clipboard and then presses keys in somebody else's
        // window, which is two different things to have agreed to.
        "Action.Paste" => &[ClipboardWrite, InputInjection],
        _ => &[],
    }
}

/**
Performs an action Raycast implements itself rather than handing to the
extension.

`Action.CopyToClipboard` and friends carry no `onAction`; they declare what
they want done through their props and the launcher is expected to do it.
Treating them as broken because they have no callback would silently kill the
most common action in the whole ecosystem.

## Why the session is a parameter

**These are the same capabilities the API layer gates, reached by another
door.** Every one of the twenty-two host methods asks `Permits` first; these
four did not, so an extension refused `Clipboard/copy` could render an
`Action.CopyToClipboard` and have the launcher do it, and `Action.Paste`
injects keystrokes into whatever is in front without anything having been
agreed to. The store's own capability screen says pasting is asked about, and
it was not.

So the window says which session drew the action, Rust turns that into the
extension's name, and the same `Permits` decides. The session is looked up
rather than trusted: the window sends an id, and the id means nothing except
through the host's own map of live sessions.
*/
#[tauri::command]
pub(crate) async fn perform_builtin(
    app: AppHandle,
    session: String,
    tag: String,
    props: Value,
) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    {
        use tauri::Manager;

        let host = app.state::<crate::state::HostState>();
        let extension = crate::host::extension_of(&host, &session).await;

        let Some(extension) = extension else {
            // No live session means nothing is asking on an extension's
            // behalf, and a built-in exists only to serve one.
            return Err("that action does not belong to a running extension".to_string());
        };

        host.api
            .permits()
            .allow(&extension, builtin_needs(&tag))
            .await?;
    }

    /// Raycast lets `content` be a string, a number, or a shaped object.
    fn text_of(value: Option<&Value>) -> Option<String> {
        match value? {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Object(map) => map
                .get("text")
                .or_else(|| map.get("html"))
                .or_else(|| map.get("file"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            _ => None,
        }
    }

    match tag.as_str() {
        "Action.CopyToClipboard" => {
            let content = text_of(props.get("content"))
                .ok_or_else(|| "that action carried nothing to copy".to_string())?;
            app.clipboard()
                .write_text(content)
                .map_err(|e| e.to_string())?;
            Ok("Copied".to_string())
        }

        "Action.OpenInBrowser" | "Action.Open" => {
            let target = props
                .get("url")
                .or_else(|| props.get("target"))
                .and_then(Value::as_str)
                .ok_or_else(|| "that action carried nothing to open".to_string())?;

            tauri_plugin_opener::open_url(target, None::<&str>).map_err(|e| e.to_string())?;
            Ok("Opened".to_string())
        }

        "Action.Paste" => {
            let content = text_of(props.get("content"))
                .ok_or_else(|| "that action carried nothing to paste".to_string())?;
            app.clipboard()
                .write_text(content)
                .map_err(|e| e.to_string())?;

            // It said "paste injection is not built yet" and only copied,
            // which was honest at the time and is no longer true: the same
            // synthetic input dictation has always used does this.
            crate::dictation::paste::deliver(&app);
            Ok("Pasted".to_string())
        }

        other => Err(format!("{other} is not a built-in Sill can perform")),
    }
}

/**
Writes the ranking history without holding anything up.

`RegistryState::record` hands back the serialised form, made while the writer
lock was held and the data was therefore consistent. Putting it on disk is a
different matter: it happens on the blocking pool, where a slow disk is
nobody's problem. It used to happen on the lock a search takes.

Losing a launch's ranking is not worth failing the launch over, which is why
this reports and returns rather than propagating.
*/
fn save_ranking_soon(path: &std::path::Path, text: Option<String>) {
    let Some(text) = text else {
        crate::say!("could not serialise the ranking history");
        return;
    };

    let path = path.to_path_buf();

    tauri::async_runtime::spawn_blocking(move || {
        if let Err(err) = crate::registry::Frecency::write(&path, &text) {
            crate::say!("could not save frecency: {err}");
        }
    });
}

/// An object the window is pointing at.
///
/// The window echoes back the fields Rust already sent it in a search result
/// rather than inventing any, which is what lets a file work: a file result
/// comes from Everything at query time and was never in the index, so there
/// is nothing to look it up in.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectRef {
    id: String,
    mode: String,
    /// The result's `entrypoint`: a path, a panel, a stored id, a value.
    target: String,
    title: String,
}

impl ObjectRef {
    fn into_object(self) -> Result<Object, String> {
        let kind = crate::object::ObjectKind::from_mode(&self.mode)
            .ok_or_else(|| format!("{} is a kind of thing Sill cannot act on", self.title))?;

        Ok(Object {
            kind,
            id: self.id,
            target: self.target,
            title: self.title,
            mode: self.mode,
        })
    }
}

/// Renames a file or folder, keeping it where it is.
///
/// A command rather than an action for one reason: the launcher has to ask for
/// the new name first, and an action is handed an object and acts. The asking
/// is most of what renaming is.
#[tauri::command]
pub(crate) async fn rename_path(path: String, to: String) -> Result<String, String> {
    let from = std::path::PathBuf::from(&path);
    let was = crate::files_ops::name_of(&from);

    let landed = tokio::task::spawn_blocking(move || crate::files_ops::rename(&from, &to))
        .await
        .map_err(|err| format!("could not rename that: {err}"))??;

    Ok(format!(
        "Renamed {was} to {}",
        crate::files_ops::name_of(&landed)
    ))
}

/// Moves a file or folder into another folder.
///
/// A command rather than an action for the reason renaming is one: the
/// launcher has to ask where first, and an action is handed an object and
/// acts. Picking the folder is most of what moving is.
///
/// Unlike renaming, this comes back with an undo. A move is the one file
/// operation that reverses exactly, and the token is two paths rather than
/// anything copied, so undoing a move of something enormous costs what undoing
/// a move of a text file costs.
#[tauri::command]
pub(crate) async fn move_path(
    app: AppHandle,
    state: State<'_, RegistryState>,
    path: String,
    folder: String,
) -> Result<Outcome, String> {
    let from = std::path::PathBuf::from(&path);
    let into = std::path::PathBuf::from(&folder);

    let name = crate::files_ops::name_of(&from);

    // Where it came out of, captured before the move, because afterwards
    // there is nothing left at the old path to ask.
    let came_from = from
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .ok_or_else(|| format!("{name} has nowhere to be put back"))?;

    let landed = {
        let from = from.clone();
        let into = into.clone();

        // Blocking: between two drives this copies, and that is as slow as
        // whatever is being moved is large.
        tokio::task::spawn_blocking(move || crate::files_ops::move_to(&from, &into))
            .await
            .map_err(|err| format!("could not move that: {err}"))??
    };

    /*
     * Remembered, so the next move offers it first.
     *
     * Kept in the same store that ranks everything else, under a prefix of its
     * own: a folder somebody moves things to is exactly the kind of thing
     * frecency is for, and a second store would be a second answer to "what do
     * you reach for" that could disagree with the first.
     *
     * After the move rather than before, so a destination that turned out to
     * be refused is not learned as one somebody uses.
     */
    {
        let now = now_seconds();
        let folder = folder.clone();

        // Off the lock, like every other write of this file. Losing which
        // folder was used is not worth failing a move over either.
        let (path, text) = state.record(move |ranking| {
            ranking.frecency.record(&format!("{MOVED_TO}{folder}"), now);
            ranking.path.clone()
        });

        save_ranking_soon(&path, text);
    }

    /*
     * Built here rather than by an action, and recorded here for the same
     * reason.
     *
     * Moving is a command because the destination is a question, and a
     * question is not something an action has anywhere to ask. That is a fair
     * exception, but it left this outside the activity log entirely: the move
     * could be taken back with Ctrl+Z and appeared nowhere in Advanced, so
     * anything that scrolled the launcher away lost it. Recording it here
     * costs one call and puts it where every other reversible thing lives.
     */
    let mut outcome = Outcome::undoable(
        format!("Moved {name} to {}", crate::files_ops::name_of(&into)),
        Undo::MovePath {
            path: landed.to_string_lossy().to_string(),
            back_to: came_from,
            name: name.clone(),
        },
    );

    let ctx = ActionCtx { app };
    let id = crate::activity::record(&ctx, "Move to Folder", &name, &outcome);
    outcome.undone_by = id.filter(|id| *id != 0);

    Ok(outcome)
}

/// The folders something could be moved into, for a query.
///
/// Two lists behind one command, because they answer the same question at two
/// moments. With nothing typed it is the folders somebody is likely to mean:
/// the ones they have moved things to before, then the standard places. Once
/// they type it is a folder search, because the answer is a folder somewhere
/// on the machine and no fixed list can hold it.
///
/// The folder the thing is already in is never offered. It is the one
/// destination that cannot be right, and it would otherwise rank first for
/// somebody typing the name of where they are.
#[tauri::command]
pub(crate) async fn search_destinations(
    prefs: State<'_, PrefsState>,
    catalog: State<'_, CatalogState>,
    state: State<'_, RegistryState>,
    query: String,
    // What is being moved, so its own folder can be left out.
    source: String,
) -> Result<Vec<registry::SearchResult>, String> {
    let already_in = std::path::PathBuf::from(&source)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default();

    let recent = { state.ranking().frecency.recent_with_prefix(MOVED_TO, 8) };

    // Always. These are read off the disk rather than looked up, so a folder
    // made a minute ago is here even though no index has seen it yet, and a
    // folder somebody just made is exactly the one they are about to move
    // something into.
    let close_by = crate::files_ops::likely_destinations(recent, std::path::Path::new(&source));

    let typed = query.trim();

    let mut folders: Vec<String> = if typed.is_empty() {
        close_by
    } else {
        // Matched by the same rules the launcher matches everything else by,
        // so a folder answers to the same letters here as it does in a search.
        let needle: Vec<char> = typed.to_lowercase().chars().collect();

        let mut near: Vec<String> = close_by
            .into_iter()
            .filter(|folder| {
                let name = crate::files_ops::name_of(std::path::Path::new(folder));
                registry::match_name(&needle, &name).is_some()
            })
            .collect();

        let settings = prefs.inner.lock().await.files.clone();
        let wanted = settings.max_results as usize;

        // Then the index, for everywhere else on the machine. Sill's own only:
        // the whole-volume indexer answers with files as well and cannot be
        // asked for folders alone, so it would spend a search returning things
        // that cannot be a destination.
        let found = catalog
            .inner
            .load()
            .search(typed, wanted, &settings.only_in);

        near.extend(
            found
                .into_iter()
                .filter(|hit| hit.is_dir)
                .map(|hit| hit.path),
        );
        near
    };

    // One row per folder, however many ways it arrived.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    folders.retain(|folder| seen.insert(folder.to_lowercase()));

    Ok(folders
        .into_iter()
        .filter(|folder| folder != &already_in)
        .map(|folder| registry::SearchResult::from_record(registry::destination_record(&folder)))
        .collect())
}

/// What a folder chosen as a destination is remembered under.
///
/// Its own prefix so these never collide with a command id, and so the ones
/// worth offering again can be found without walking everything ever launched.
pub(crate) const MOVED_TO: &str = "moved-to:";

/// Reads the words out of the last picture copied.
///
/// The row in the list and the key bound to it both end here, through the same
/// action and the same idea of which picture is meant. Screenshot with the
/// shortcut Windows already has, then ask for the words.
#[tauri::command]
pub(crate) async fn extract_text_from_last_image(app: AppHandle) -> Result<String, String> {
    let object = crate::bindings::last_image(&app)?;

    let registry = app.state::<ActionRegistry>();
    let action = registry
        .get("sill.extractText")
        .ok_or_else(|| "text recognition is not available".to_string())?;

    let outcome = registry
        .perform(&ActionCtx { app: app.clone() }, action, &object)
        .await?;
    Ok(outcome.message)
}

/// What can be done to the selected result.
///
/// Keyed on the mode rather than looked up by id, because the answer depends
/// only on what kind of thing it is, and because a file result is not in any
/// index to be looked up in.
#[tauri::command]
pub(crate) fn actions_for(
    actions: State<'_, ActionRegistry>,
    mode: String,
) -> Vec<crate::action::ActionInfo> {
    let out = crate::object::ObjectKind::from_mode(&mode)
        .map(|kind| actions.describe(kind))
        .unwrap_or_default();
    out
}

/// Runs one action against one object.
///
/// Frecency is deliberately not recorded here. It learns what you open, and
/// copying a path or opening a containing folder is looking at something
/// rather than reaching for it. Enter still records, through `launch_command`.
#[tauri::command]
pub(crate) async fn run_action(
    app: AppHandle,
    action: String,
    object: ObjectRef,
) -> Result<crate::action::Outcome, String> {
    let object = object.into_object()?;

    let registry = app.state::<ActionRegistry>();
    let chosen = registry
        .get(&action)
        .ok_or_else(|| format!("no such action: {action}"))?;

    if !chosen.accepts(object.kind) {
        // Not an error the user caused, so it names both halves: this arrives
        // when the window's idea of the selection has drifted from Rust's.
        return Err(format!(
            "{} cannot be done to {}",
            chosen.title(),
            object.title
        ));
    }

    let outcome = registry
        .perform(&ActionCtx { app: app.clone() }, chosen, &object)
        .await?;

    // The screen now belongs to something else, so the launcher must not put
    // it back on the way out. Decided from the kind rather than from the
    // action, because it is a fact about what was acted on: every way of
    // reaching a window ends with that window in front.
    if object.kind.hands_over_the_screen() {
        crate::summon::forget_foreground();
    }

    Ok(outcome)
}

/// Reverses an action that said it could be reversed.
#[tauri::command]
pub(crate) async fn undo_action(
    app: AppHandle,
    undo: crate::action::Undo,
) -> Result<String, String> {
    crate::action::undo(&ActionCtx { app: app.clone() }, &undo)
}

/// Counts a use of something the window opened by itself.
///
/// Two results are handled entirely in the window and never reach
/// `launch_command`: the clipboard history, which becomes a view rather than
/// a launch, and a quicklink with a hole in it, which takes over the field
/// instead. **Both were therefore invisible to ranking.** `sill:clipboard`
/// had never been recorded once, however often it was opened, so it could
/// never rise in the root list and nothing typed at it could ever be learned.
///
/// Separate from `launch_command` rather than folded into it because these
/// genuinely are not launches; what they share is only that they count.
#[tauri::command]
pub(crate) async fn record_use(
    state: State<'_, RegistryState>,
    id: String,
    query: Option<String>,
    // Whether the query belongs in the history that Up walks back through.
    //
    // The history is what was typed at the root. A query typed into the emoji
    // picker taught something useful about emoji and nothing about the root
    // list, and offering it back there would recall a search that now finds
    // nothing. Defaults to true, because every other caller is the root.
    history: Option<bool>,
) -> Result<(), String> {
    let now = now_seconds();

    let (path, text) = state.record(move |ranking| {
        ranking.frecency.record(&id, now);

        if let Some(query) = query.as_deref() {
            ranking.frecency.record_query(query, &id, now);
            if history.unwrap_or(true) {
                ranking.frecency.remember(query);
            }
        }

        ranking.path.clone()
    });

    save_ranking_soon(&path, text);

    Ok(())
}

/// What was typed before, most recent first.
///
/// For walking back through past queries in an empty field. Only queries that
/// reached something: a launcher offering back the half-finished strings
/// somebody abandoned would mostly be offering them their mistakes.
#[tauri::command]
pub(crate) async fn query_history(state: State<'_, RegistryState>) -> Result<Vec<String>, String> {
    Ok(state.ranking().frecency.history().to_vec())
}
