//! Reading and writing Sill's own preferences, and the window that edits them.

use crate::{
    apply_autostart, apply_dictation, apply_tray, apply_window_size, rebind_capture, rebind_summon,
    rebind_switcher, same_dictation,
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::PrefsState;
use crate::{clipboard, preferences, settings_index, snippets, summon};

/// The one report about pictures that would not convert.
const LOCK_TROUBLE: &str = "clipboard-lock";

/// Converts the pictures already in the history when the setting changes.
///
/// Nothing is deleted either way and nothing becomes unreadable: a picture is
/// rewritten in place, and one that will not convert is left exactly as it
/// was. A blob carries its own marker, so a database holding both kinds reads
/// correctly whatever happens here.
///
/// A picture that will not unlock was copied under a different Windows
/// account. Said out loud rather than swallowed, because the alternative is a
/// row that draws as an image and shows nothing, with no explanation anywhere.
fn convert_clipboard_pictures(app: &AppHandle, history: &clipboard::monitor::Clipboard, on: bool) {
    let store = history.store();
    let outcome = if on {
        store.seal_pictures()
    } else {
        store.unseal_pictures()
    };
    drop(store);

    match outcome {
        Ok((converted, 0)) => {
            if converted > 0 {
                crate::say!(
                    "{converted} stored clipboard pictures were {}",
                    if on { "locked" } else { "unlocked" }
                );
            }
            crate::status::resolved(app, LOCK_TROUBLE);
        }
        Ok((_, failed)) => crate::status::report(
            app,
            LOCK_TROUBLE,
            format!(
                "{failed} pictures in the clipboard history were copied under a different \
                 Windows account and could not be changed. They are still there and still \
                 unreadable from this one."
            ),
            Some("clipboard"),
        ),
        Err(err) => crate::status::report(
            app,
            LOCK_TROUBLE,
            format!("Sill could not change how stored pictures are kept: {err}"),
            Some("clipboard"),
        ),
    }
}

#[tauri::command]
pub(crate) async fn get_preferences(
    state: State<'_, PrefsState>,
) -> Result<preferences::Preferences, String> {
    Ok(state.inner.lock().await.clone())
}

/// Saves preferences and applies whatever can change without a restart.
///
/// The hotkey and the backdrop take effect immediately; source and file search
/// changes are read on the next query or scan. Anything needing a restart says
/// so in the UI rather than pretending.
#[tauri::command]
pub(crate) async fn set_preferences(
    app: AppHandle,
    state: State<'_, PrefsState>,
    prefs: preferences::Preferences,
) -> Result<(), String> {
    let previous = {
        let mut current = state.inner.lock().await;
        let previous = current.clone();
        *current = prefs.clone();
        previous
    };

    prefs.save(&state.path).map_err(|e| e.to_string())?;

    if previous.appearance.visible_rows != prefs.appearance.visible_rows
        || previous.appearance.window_width != prefs.appearance.window_width
    {
        apply_window_size(&app, &prefs.appearance);
    }

    if previous.general.open_at_login != prefs.general.open_at_login {
        apply_autostart(&app, prefs.general.open_at_login);
    }

    if previous.general.show_in_tray != prefs.general.show_in_tray {
        apply_tray(&app, prefs.general.show_in_tray);
    }

    if let Some(expander) = app.try_state::<snippets::expander::Expander>() {
        expander.set_enabled(prefs.snippets.expand_keywords);
        expander.set_tap_binding(crate::tap_binding(&prefs.taps));
        expander.set_hyper(prefs.hyper.key);

        // Switched off means taken out, not told to ignore everything. A
        // low-level keyboard hook is called for every keystroke on the
        // machine, in every application; staying in that path in order to do
        // nothing is exactly the cost rule 23 refuses.
        //
        // Two features share the hook now, so the question is whether either
        // wants it. Asked of the expander, which is the one place that knows,
        // rather than of the preferences here and again at startup.
        if expander.wanted() {
            snippets::expander::watch(&app, &expander);
        } else {
            snippets::expander::stop(&expander);
        }
    }

    if let Some(history) = app.try_state::<clipboard::monitor::Clipboard>() {
        history.set_rules(clipboard::monitor::Rules {
            enabled: prefs.clipboard.enabled,
            keep_images: prefs.clipboard.keep_images,
            ignored_apps: prefs.clipboard.ignored_apps.clone(),
            secrets: prefs.clipboard.secrets,
            retain_days: prefs.clipboard.retain_days,
            max_entries: prefs.clipboard.max_entries,
            encrypt_images: prefs.clipboard.encrypt_images,
        });

        // Turning the lock on covers what is already stored, and turning it
        // off leaves nothing behind that only one Windows account can open.
        // Doing neither would make the setting a promise about the future
        // only, so the pictures somebody wanted protected, the ones already
        // there, would be the ones still in the clear.
        //
        // Only when it changed. This runs on every settings write, and reading
        // every stored picture to find nothing to do is not free.
        if previous.clipboard.encrypt_images != prefs.clipboard.encrypt_images {
            convert_clipboard_pictures(&app, &history, prefs.clipboard.encrypt_images);
        }

        // Switched off means stopped. The watcher owns a thread and a hidden
        // window and is woken by every copy on the machine, whether or not it
        // does anything with what it sees.
        if prefs.clipboard.enabled {
            clipboard::monitor::watch(&app, &history);
        } else {
            clipboard::monitor::stop(&history);
        }
    }

    if !same_dictation(&previous.dictation, &prefs.dictation) {
        apply_dictation(&app, &prefs.dictation);
    }

    if previous.aliases != prefs.aliases {
        // Rebuilt rather than reloaded per query. Ranking asks about aliases
        // once per candidate on every keystroke.
        let aliases = crate::registry::Aliases::new(&prefs.aliases);
        app.state::<crate::state::RegistryState>()
            .update_index(move |index| index.aliases = aliases);
    }

    if previous.bindings != prefs.bindings {
        crate::bindings::apply(&app, &previous.bindings, &prefs.bindings);
    }

    if previous.hotkey.summon != prefs.hotkey.summon {
        rebind_summon(&app, &previous.hotkey.summon, &prefs.hotkey.summon);
    }

    if previous.hotkey.switcher != prefs.hotkey.switcher {
        rebind_switcher(&app, &previous.hotkey.switcher, &prefs.hotkey.switcher);
    }

    if previous.hotkey.capture != prefs.hotkey.capture {
        rebind_capture(&app, &previous.hotkey.capture, &prefs.hotkey.capture, false);
    }

    if previous.hotkey.capture_screen != prefs.hotkey.capture_screen {
        rebind_capture(
            &app,
            &previous.hotkey.capture_screen,
            &prefs.hotkey.capture_screen,
            true,
        );
    }

    // Read on every summon rather than at startup, so moving this takes the
    // launcher to the other screen now.
    if let Some(placement) = app.try_state::<crate::placement::Placement>() {
        placement.set(prefs.appearance.summon_on);
    }

    // Read when focus is lost rather than wired once, so this one takes
    // effect now instead of at the next start. See `DismissOnBlur`.
    if let Some(blur) = app.try_state::<crate::DismissOnBlur>() {
        blur.set(prefs.hotkey.dismiss_on_blur);
    }

    if previous.appearance.backdrop != prefs.appearance.backdrop
        || previous.appearance.tint_alpha != prefs.appearance.tint_alpha
    {
        crate::apply_backdrops(&app, prefs.appearance.backdrop, prefs.appearance.tint_alpha);
    }

    // Everything that decides what is *in* the index, rather than how the
    // index is drawn. Each of these used to be read once and never again, so
    // the panel said one thing and search did another until a rebuild or a
    // restart. Compared rather than applied unconditionally: a scan is a
    // PowerShell round trip and every shortcut on the machine, and the
    // appearance slider must not pay for it.
    let redo = preferences::Redo::between(&previous, &prefs);

    if redo.sources {
        crate::reload_index(&app);
    }

    if redo.scripts {
        crate::reload_scripts(&app);
    }

    if let Some(roots) = redo.file_roots {
        // The watcher first and then the walk, which is what `index_folder`
        // does for the drive switches beside this list. Moving the watcher
        // matters as much as the rebuild: left where it was it would go on
        // waking the index for a folder nobody asked about, and say nothing
        // about the folder somebody just named.
        if let (Some(catalog), Some(watching)) = (
            app.try_state::<crate::state::CatalogState>(),
            app.try_state::<crate::state::Watching>(),
        ) {
            watching.re_root(catalog.inner().clone(), roots.clone());
            catalog.rebuild(roots);
        }
    }

    // The launcher window re-reads whatever it renders from.
    let _ = app.emit("sill://preferences-changed", &prefs);
    Ok(())
}

/// Opens the settings window, creating it the first time.
///
/// A separate window rather than a view inside the launcher: settings are read
/// and edited slowly, while the launcher is built to disappear the moment it
/// loses focus.
#[tauri::command]
pub(crate) async fn open_settings(app: AppHandle, section: Option<String>) -> Result<(), String> {
    // A section is carried in the query so a deep link lands where it means
    // to. Without it "About" would open settings at whatever was last shown.
    let route = match section.as_deref() {
        Some(name) if !name.is_empty() => format!("settings?section={name}"),
        _ => "settings".to_string(),
    };

    if let Some(existing) = app.get_webview_window("settings") {
        // Unminimised first: `show` on a minimised window leaves it minimised,
        // so asking for settings again did nothing visible.
        let _ = existing.unminimize();
        let _ = existing.show();
        let _ = existing.set_focus();
        if let Some(name) = section {
            let _ = existing.emit("sill://settings-section", name);
        }
        summon::forget_foreground();
        return Ok(());
    }

    let window =
        tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App(route.into()))
            .title("Settings")
            // Room for a 244px sidebar plus a settings pane that does not wrap its own
            // descriptions. Anything narrower and the right pane reads as a column.
            .inner_size(1180.0, 800.0)
            .min_inner_size(940.0, 620.0)
            .resizable(true)
            // Frameless and transparent, so the page draws its own title bar and the
            // same glass the launcher uses. A default title bar next to a glass body
            // looks like two different applications.
            .decorations(false)
            .transparent(true)
            .center()
            .focused(true)
            .build()
            .map_err(|e| e.to_string())?;

    let _ = window.set_focus();

    // The launcher must not hand the screen back to whatever was in front
    // before it. See the same note in `open_ask`.
    summon::forget_foreground();

    let appearance = {
        let prefs = app.state::<PrefsState>();
        let guard = prefs.inner.lock().await;
        (guard.appearance.backdrop, guard.appearance.tint_alpha)
    };

    summon::apply_backdrop(&window, appearance.0, appearance.1);

    Ok(())
}

/// Sill's own settings, for the settings window's filter box.
///
/// Read from the same catalogue the launcher searches, so the two can never
/// disagree about what exists or which panel it is in.
#[tauri::command]
pub(crate) fn list_own_settings() -> Vec<settings_index::Setting> {
    settings_index::SETTINGS.to_vec()
}

/// Accelerators another application already owns.
///
/// Read by the settings window so a key that could not be bound says so, in
/// the row that set it, rather than working silently in the log and nowhere
/// else. Windows does not say which application took it, so neither does this.
#[tauri::command]
pub(crate) async fn hotkey_conflicts(app: AppHandle) -> Vec<String> {
    app.try_state::<crate::HotkeyConflicts>()
        .map(|conflicts| conflicts.all())
        .unwrap_or_default()
}

/// Everything Sill is quietly not doing.
///
/// Read by the settings window, which is the surface with room to say what is
/// wrong and where to fix it. The tray only ever gets a line.
#[tauri::command]
pub(crate) async fn status_troubles(app: AppHandle) -> Vec<crate::status::Trouble> {
    app.try_state::<crate::status::Status>()
        .map(|status| status.all())
        .unwrap_or_default()
}

/// One thing a window asked Rust for and did not get.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Unreadable {
    /// Which window, so it can withdraw its own reports without touching
    /// anybody else's.
    surface: crate::status::Surface,
    /// The thing, named the way a sentence would name it, so the message reads
    /// as English rather than as a command name.
    what: String,
    reason: String,
    /// The settings panel that holds the control this is about, so the band
    /// showing it can offer to go there. Empty when there is no such panel.
    section: String,
}

/// Records that a window could not read something it needs.
///
/// Every one of these calls used to end in `.catch(() => [])`, which turns a
/// refusal into an empty list and then draws that list as though it were the
/// answer: no search engines, no browsers on this machine, no key conflicts.
/// Tauri denies a command to a window missing from `capabilities/default.json`
/// **silently**, which is exactly how the tray menu once shipped dead, so this
/// is the failure most likely to be behind an empty settings pane.
///
/// The window still gets its fallback and still draws. The difference is that
/// somewhere on screen says the list is not the truth.
#[tauri::command]
pub(crate) async fn note_unreadable(app: AppHandle, failed: Unreadable) {
    crate::status::unreadable(
        &app,
        failed.surface,
        &failed.what,
        &failed.reason,
        &failed.section,
    );
}

/// Forgets what a window last failed to read, because it is about to try again.
///
/// A group rather than one at a time: the settings window re-reads all of them
/// on every open, so whatever it found last time is stale before the first
/// answer arrives. Clearing them individually would mean the window
/// remembering which ones it had reported, which is a second copy of state
/// Rust already holds.
#[tauri::command]
pub(crate) async fn forget_unreadable(app: AppHandle, surface: crate::status::Surface) {
    crate::status::readable_again(&app, surface);
}

/// Gives a command a name of the user's own, or takes one away.
///
/// An empty alias removes it. One command has at most one name and one name
/// points at one command, so setting either half replaces whatever held it:
/// two commands answering to the same word would make that word useless, and
/// silently keeping the older claim would look like the new one was ignored.
#[tauri::command]
pub(crate) async fn set_alias(
    app: AppHandle,
    prefs: State<'_, crate::state::PrefsState>,
    command: String,
    alias: String,
) -> Result<crate::preferences::Preferences, String> {
    let wanted = alias.trim().to_lowercase();

    let next = {
        let mut current = prefs.inner.lock().await;
        current
            .aliases
            .retain(|a| a.command != command && (wanted.is_empty() || a.alias != wanted));

        if !wanted.is_empty() {
            current.aliases.push(crate::registry::Alias {
                alias: wanted,
                command,
            });
        }

        current.clone()
    };

    set_preferences(app, prefs, next.clone()).await?;
    Ok(next)
}

/// One indexed thing, as the settings list shows it.
///
/// Deliberately not a [`crate::registry::SearchResult`]: that type answers
/// "what did this query find", and this answers "what is in the index and how
/// do you reach it". The three ways to reach something are an alias, a key,
/// and being in the list at all, which is why they are three fields here and
/// not three unrelated screens.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndexRow {
    pub id: String,
    pub title: String,
    /// The index mode, which the window turns into a readable kind.
    pub mode: String,
    /// A file to draw an icon from.
    pub icon: Option<String>,
    pub alias: Option<String>,
    /// The accelerator bound to opening this, if any.
    pub hotkey: Option<String>,
    /// Switched off individually, so it never appears in the launcher.
    pub hidden: bool,
}

/// How many rows the settings list is given at once.
///
/// The index is around fifteen hundred entries and this is a browsable list,
/// not a search. Sending all of it would be a third of a megabyte for a screen
/// that shows twenty rows, which is the payload mistake the audit measured
/// once already. The filter narrows in Rust and the count of what was left out
/// is shown, so nothing is silently truncated.
const ROWS: usize = 200;

/// Everything in the index, for the settings list.
#[tauri::command]
pub(crate) async fn index_rows(
    state: State<'_, crate::state::RegistryState>,
    prefs: State<'_, crate::state::PrefsState>,
    query: String,
    // An index mode, or nothing for all of them.
    mode: Option<String>,
) -> Result<IndexPage, String> {
    let (aliases, bindings, hidden) = {
        let prefs = prefs.inner.lock().await;
        (
            crate::registry::Aliases::new(&prefs.aliases),
            prefs.bindings.clone(),
            prefs.sources.hidden.clone(),
        )
    };

    let index = state.index();

    Ok(rows_for(
        index
            .commands
            .iter()
            .chain(index.snippets.iter())
            .chain(index.quicklinks.iter())
            .chain(index.own_settings.iter()),
        &aliases,
        &bindings,
        &hidden,
        &query,
        mode.as_deref(),
    ))
}

/// The list itself, without the plumbing that fetches its inputs.
///
/// Separated so it can be tested at all: the command needs Tauri state and a
/// running app, and none of the decisions in here do.
pub(crate) fn rows_for<'a>(
    commands: impl IntoIterator<Item = &'a crate::registry::CommandRecord>,
    aliases: &crate::registry::Aliases,
    bindings: &[crate::bindings::Binding],
    hidden: &[String],
    query: &str,
    mode: Option<&str>,
) -> IndexPage {
    let needle = query.trim().to_lowercase();
    let wanted = mode.filter(|m| *m != "all");

    let mut matched: Vec<&crate::registry::CommandRecord> = commands
        .into_iter()
        .filter(|c| wanted.is_none_or(|m| c.mode == m))
        .filter(|c| needle.is_empty() || c.title.to_lowercase().contains(&needle))
        .collect();

    // Alphabetical, not by rank. This is a list to find a known thing in, and
    // frecency order would move rows between visits for no reason the person
    // browsing it can see.
    matched.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));

    let total = matched.len();
    let rows = matched
        .into_iter()
        .take(ROWS)
        .map(|command| IndexRow {
            alias: aliases.for_command(&command.id).map(str::to_string),
            hotkey: bindings
                .iter()
                .find(|b| {
                    matches!(&b.source, crate::bindings::Source::Command { id } if id == &command.id)
                })
                .map(|b| b.accelerator.clone()),
            hidden: hidden.iter().any(|id| id == &command.id),
            id: command.id.clone(),
            title: command.title.clone(),
            mode: command.mode.clone(),
            icon: command
                .icon
                .clone()
                .or_else(|| Some(command.entrypoint.clone()))
                .filter(|icon| !icon.is_empty()),
        })
        .collect();

    IndexPage { rows, total }
}

/// A page of the settings list, and how many matched in total.
///
/// The total travels with it so the window can say "200 of 1,502" rather than
/// quietly showing the first two hundred as though that were all of them.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndexPage {
    pub rows: Vec<IndexRow>,
    pub total: usize,
}

/// Binds a key to opening one indexed thing, or unbinds it.
///
/// Writes an ordinary binding with `Source::Command`, which the shortcut
/// router already understands, rather than a second kind of hotkey. The
/// Shortcuts panel keeps showing it: one model, two ways in.
#[tauri::command]
pub(crate) async fn set_command_hotkey(
    app: AppHandle,
    prefs: State<'_, crate::state::PrefsState>,
    command: String,
    accelerator: String,
) -> Result<crate::preferences::Preferences, String> {
    let wanted = accelerator.trim().to_string();

    let next = {
        let mut current = prefs.inner.lock().await;

        // One command has at most one key, and one key means at most one
        // thing. Setting either half replaces whatever held it, because two
        // things answering to one key means whichever registered second
        // silently does nothing.
        current.bindings.retain(|b| {
            let same_command =
                matches!(&b.source, crate::bindings::Source::Command { id } if id == &command);
            !same_command && (wanted.is_empty() || b.accelerator != wanted)
        });

        if !wanted.is_empty() {
            current.bindings.push(crate::bindings::Binding {
                accelerator: wanted,
                action: crate::bindings::PRIMARY.to_string(),
                source: crate::bindings::Source::Command { id: command },
                replace: false,
                argument: None,
            });
        }

        current.clone()
    };

    set_preferences(app, prefs, next.clone()).await?;
    Ok(next)
}

/// Switches one indexed entry off, or back on.
#[tauri::command]
pub(crate) async fn set_hidden(
    app: AppHandle,
    prefs: State<'_, crate::state::PrefsState>,
    command: String,
    hidden: bool,
) -> Result<crate::preferences::Preferences, String> {
    let next = {
        let mut current = prefs.inner.lock().await;
        current.sources.hidden.retain(|id| id != &command);
        if hidden {
            current.sources.hidden.push(command);
        }
        current.clone()
    };

    set_preferences(app, prefs, next.clone()).await?;
    Ok(next)
}

/// Keeps one entry at the top of the root list, or stops.
///
/// Appended rather than inserted, so the order is the order things were
/// pinned. Somebody who pins five things has arranged five things, and
/// deciding the order for them would undo that.
///
/// Unpinning and pinning again therefore moves an entry to the end, which is
/// how every pinned list behaves and is the only re-ordering there is until
/// something can drag them.
#[tauri::command]
pub(crate) async fn set_pinned(
    app: AppHandle,
    prefs: State<'_, crate::state::PrefsState>,
    command: String,
    pinned: bool,
) -> Result<crate::preferences::Preferences, String> {
    let next = {
        let mut current = prefs.inner.lock().await;
        current.sources.pinned.retain(|id| id != &command);
        if pinned {
            current.sources.pinned.push(command);
        }
        current.clone()
    };

    set_preferences(app, prefs, next.clone()).await?;
    Ok(next)
}

/// The keyboard reference, built from the keys that actually run.
///
/// Assembled here rather than in the window so it reads from the same three
/// sources the keys come from: the movement preset, the action shortcuts and
/// the summon key. A written list would be wrong the first time one of them
/// changed, and the person reading it would have no way to tell.
#[tauri::command]
pub(crate) async fn keyboard_reference(
    actions: State<'_, crate::action::ActionRegistry>,
    prefs: State<'_, crate::state::PrefsState>,
) -> Result<Vec<crate::keysheet::KeySection>, String> {
    let (summon, navigation, keys) = {
        let current = prefs.inner.lock().await;
        (
            current.hotkey.summon.clone(),
            current.navigation.clone(),
            current.action_keys.clone(),
        )
    };

    let moving: Vec<(String, String, bool)> = crate::navigation::Move::ALL
        .into_iter()
        .map(|movement| {
            (
                crate::navigation::effective(&navigation, movement),
                movement.title().to_string(),
                navigation.overrides.contains_key(&movement),
            )
        })
        .collect();

    // Every action that carries a key, from every list it can appear on, with
    // the same clash rule the panel uses. Collected by id so an action shown
    // on two kinds is one line.
    let mut acting: std::collections::BTreeMap<String, (String, String, bool, bool)> =
        std::collections::BTreeMap::new();

    for kind in crate::object::ObjectKind::ALL {
        let shown: Vec<(String, String, Option<crate::action_keys::Shortcut>)> = actions
            .describe(*kind, &keys)
            .into_iter()
            .map(|a| (a.id.to_string(), a.title.to_string(), a.shortcut))
            .collect();

        let contested: std::collections::BTreeSet<String> = crate::action_keys::conflicts(&shown)
            .into_iter()
            .map(|clash| clash.id)
            .collect();

        for (id, title, shortcut) in shown {
            let Some(shortcut) = shortcut else { continue };
            let changed = keys.overrides.contains_key(&id);
            let clash = contested.contains(&id);

            acting
                .entry(id)
                .and_modify(|line| line.3 |= clash)
                .or_insert((shortcut.chord(), title, changed, clash));
        }
    }

    let acting: Vec<(String, String, bool, bool)> = acting.into_values().collect();

    Ok(crate::keysheet::reference(&summon, &moving, &acting))
}

/// The terminal profiles this machine offers.
///
/// Asked for when the list is opened, not on a keystroke: it reads a settings
/// file and runs `wsl.exe`, and neither is work to do because somebody typed a
/// letter.
#[tauri::command]
pub(crate) async fn terminal_profiles() -> Vec<crate::terminals::Profile> {
    tokio::task::spawn_blocking(crate::terminals::available)
        .await
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{Binding, Source, PRIMARY};
    use crate::registry::{Alias, Aliases, CommandRecord};

    fn entry(id: &str, title: &str, mode: &str) -> CommandRecord {
        CommandRecord {
            id: id.into(),
            extension: "app".into(),
            extension_title: "App".into(),
            command: title.into(),
            title: title.into(),
            subtitle: String::new(),
            description: String::new(),
            mode: mode.into(),
            entrypoint: format!("C:/{title}.exe"),
            keywords: Vec::new(),
            icon: None,
            panel: None,
            preferences: serde_json::Value::Null,
            manifest: None,
            toggle: None,
        }
    }

    fn corpus() -> Vec<CommandRecord> {
        vec![
            entry("app:zed", "Zed", "app"),
            entry("app:code", "Visual Studio Code", "app"),
            entry("app:arc", "arc", "app"),
            entry("exe:7z", "7z.exe", "exe"),
            entry("sill:clipboard", "Clipboard History", "builtin"),
        ]
    }

    fn page(query: &str, mode: Option<&str>) -> IndexPage {
        rows_for(corpus().iter(), &Aliases::default(), &[], &[], query, mode)
    }

    #[test]
    fn rows_are_alphabetical_and_case_does_not_split_them() {
        // A list to find a known thing in. Ranking order would move rows
        // between visits for no reason the person browsing can see, and
        // sorting on raw bytes would file every lowercase name after Z.
        let titles: Vec<String> = page("", None).rows.into_iter().map(|r| r.title).collect();

        assert_eq!(
            titles,
            vec![
                "7z.exe",
                "arc",
                "Clipboard History",
                "Visual Studio Code",
                "Zed"
            ]
        );
    }

    #[test]
    fn the_kind_filter_and_the_text_filter_both_apply() {
        assert_eq!(page("", Some("app")).total, 3);
        assert_eq!(page("z", Some("app")).total, 1);

        // "all" is not a mode; it means do not filter by one.
        assert_eq!(page("", Some("all")).total, page("", None).total);
    }

    #[test]
    fn the_total_counts_everything_that_matched_not_what_was_sent() {
        // The window says "200 of 1,502" from this. If the total were the
        // length of the page, a list that stopped at two hundred would look
        // like two hundred was all there was.
        let many: Vec<CommandRecord> = (0..500)
            .map(|n| entry(&format!("app:{n}"), &format!("Thing {n:03}"), "app"))
            .collect();

        let page = rows_for(many.iter(), &Aliases::default(), &[], &[], "", None);

        assert_eq!(page.total, 500);
        assert_eq!(page.rows.len(), ROWS);
        assert!(page.total > page.rows.len());
    }

    #[test]
    fn a_row_carries_the_alias_the_key_and_whether_it_is_switched_off() {
        // The three ways to reach something, on one row. They were in three
        // unrelated places before, which is the whole reason for this list.
        let commands = corpus();

        let aliases = Aliases::new(&[Alias {
            alias: "code".into(),
            command: "app:code".into(),
        }]);
        let bindings = vec![Binding {
            accelerator: "Ctrl+Alt+C".into(),
            action: PRIMARY.into(),
            source: Source::Command {
                id: "app:code".into(),
            },
            replace: false,
            argument: None,
        }];
        let hidden = vec!["exe:7z".to_string()];

        let rows = rows_for(commands.iter(), &aliases, &bindings, &hidden, "", None).rows;

        let code = rows.iter().find(|r| r.id == "app:code").expect("listed");
        assert_eq!(code.alias.as_deref(), Some("code"));
        assert_eq!(code.hotkey.as_deref(), Some("Ctrl+Alt+C"));
        assert!(!code.hidden);

        let seven = rows.iter().find(|r| r.id == "exe:7z").expect("listed");
        assert!(seven.hidden, "a switched-off entry must still be listed");
        assert_eq!(seven.alias, None);
        assert_eq!(seven.hotkey, None);
    }

    #[test]
    fn a_key_bound_to_something_else_is_not_reported_on_this_row() {
        // Bindings are a flat list and most of them are not about a command
        // at all: a transform on the selection has no command id. Reading the
        // wrong one would show a key on a row that does not have it.
        let commands = corpus();
        let bindings = vec![
            Binding {
                accelerator: "Ctrl+Alt+U".into(),
                action: "sill.text.upper".into(),
                source: Source::Selection,
                replace: true,
                argument: None,
            },
            Binding {
                accelerator: "Ctrl+Alt+Z".into(),
                action: PRIMARY.into(),
                source: Source::Command {
                    id: "app:zed".into(),
                },
                replace: false,
                argument: None,
            },
        ];

        let rows = rows_for(
            commands.iter(),
            &Aliases::default(),
            &bindings,
            &[],
            "",
            None,
        )
        .rows;

        assert_eq!(
            rows.iter()
                .find(|r| r.id == "app:zed")
                .unwrap()
                .hotkey
                .as_deref(),
            Some("Ctrl+Alt+Z")
        );
        assert!(
            rows.iter().all(|r| r.hotkey.is_none() || r.id == "app:zed"),
            "a selection binding was attributed to a row"
        );
    }

    #[test]
    fn a_switched_off_entry_is_still_in_the_list() {
        // It has to stay findable, or switching it back on means remembering
        // that it existed. The launcher hides it; this list does not.
        let commands = corpus();
        let hidden = vec!["app:zed".to_string()];

        let rows = rows_for(commands.iter(), &Aliases::default(), &[], &hidden, "", None).rows;

        let zed = rows
            .iter()
            .find(|r| r.id == "app:zed")
            .expect("still listed");
        assert!(zed.hidden);
    }
}

/// Every chord that moves around the launcher, and what it means.
///
/// Resolved in Rust so the settings screen and the key handler cannot hold two
/// opinions about what Ctrl+N does. The window normalises a key event into one
/// chord string and looks it up, rather than testing eleven movements against
/// every press.
#[tauri::command]
pub(crate) async fn navigation_chords(
    prefs: State<'_, crate::state::PrefsState>,
) -> Result<std::collections::BTreeMap<String, crate::navigation::Move>, String> {
    let navigation = prefs.inner.lock().await.navigation.clone();
    Ok(crate::navigation::chords(&navigation))
}

/// What each movement resolves to, for the settings rows.
#[tauri::command]
pub(crate) async fn navigation_keys(
    prefs: State<'_, crate::state::PrefsState>,
) -> Result<Vec<NavigationKey>, String> {
    let navigation = prefs.inner.lock().await.navigation.clone();

    Ok(crate::navigation::Move::ALL
        .into_iter()
        .map(|movement| NavigationKey {
            id: movement,
            title: movement.title(),
            chord: crate::navigation::effective(&navigation, movement),
            overridden: navigation.overrides.contains_key(&movement),
        })
        .collect())
}

/// One movement, as a settings row shows it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavigationKey {
    pub id: crate::navigation::Move,
    pub title: &'static str,
    /// What actually happens, not what was preferred. See `navigation::effective`.
    pub chord: String,
    /// Whether this was set by hand rather than coming from the preset.
    pub overridden: bool,
}

/// Every action a key can be given, and the key it has.
///
/// Built from the same registry the action panel and Enter use, so a transform
/// added in Rust becomes bindable without this file or the panel changing.
///
/// **The conflict is worked out here, not on screen.** Whether two chords
/// clash depends on which actions appear together, and that is an answer only
/// the registry has: Copy Path and Close Window can share a key because a file
/// and a window are never the same row. The panel draws what this says.
#[tauri::command]
pub(crate) async fn action_shortcuts(
    actions: State<'_, crate::action::ActionRegistry>,
    prefs: State<'_, crate::state::PrefsState>,
) -> Result<Vec<ActionShortcut>, String> {
    let keys = prefs.inner.lock().await.action_keys.clone();

    // Every clash, from every list an action can appear on. An action shown on
    // two kinds is contested if it is contested on either of them.
    let mut clashes: std::collections::BTreeMap<String, crate::action_keys::Conflict> =
        std::collections::BTreeMap::new();

    for kind in crate::object::ObjectKind::ALL {
        let shown: Vec<(String, String, Option<crate::action_keys::Shortcut>)> = actions
            .describe(*kind, &keys)
            .into_iter()
            .map(|a| (a.id.to_string(), a.title.to_string(), a.shortcut))
            .collect();

        for clash in crate::action_keys::conflicts(&shown) {
            clashes.entry(clash.id.clone()).or_insert(clash);
        }
    }

    let mut rows: Vec<ActionShortcut> = actions
        .all()
        .into_iter()
        .map(|(id, title, default)| {
            let shortcut = crate::action_keys::effective(&keys, id, default);

            ActionShortcut {
                id,
                title,
                chord: shortcut.map(|s| s.chord()).unwrap_or_default(),
                overridden: keys.overrides.contains_key(id),
                contested: clashes.get(id).map(|c| c.other.clone()),
            }
        })
        .collect();

    // Alphabetical, because registration order is an implementation detail and
    // this is a list somebody scans for a name.
    rows.sort_by(|a, b| a.title.cmp(b.title));
    Ok(rows)
}

/// One action, as a settings row shows it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionShortcut {
    pub id: &'static str,
    pub title: &'static str,
    /// The accelerator, or empty for an action with no key.
    pub chord: String,
    /// Whether this was set by hand rather than shipped.
    pub overridden: bool,
    /// The other action that wants this chord and gets it, when there is one.
    pub contested: Option<String>,
}

/// The skin tones, each shown as a hand rather than named.
///
/// Built in Rust because the swatch is the emoji itself and the set of tones
/// is a fact about Unicode, not about the window. Naming them in words is both
/// awkward and less clear than the thing: nobody picks "medium-light" off a
/// list, they pick the one that looks right.
#[tauri::command]
pub(crate) async fn emoji_tones() -> Vec<ToneChoice> {
    crate::emoji::Tone::ALL
        .into_iter()
        .map(|tone| ToneChoice {
            id: tone,
            swatch: tone.swatch(),
        })
        .collect()
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToneChoice {
    pub id: crate::emoji::Tone,
    pub swatch: String,
}
