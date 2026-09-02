//! Reading and writing Sill's own preferences, and the window that edits them.

use crate::{
    apply_autostart, apply_dictation, apply_tray, apply_window_size, rebind_capture,
    rebind_summon, rebind_switcher, same_dictation,
};

use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::PrefsState;
use crate::{clipboard, preferences, settings_index, snippets, summon};

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
        });
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

    if previous.appearance.backdrop != prefs.appearance.backdrop
        || previous.appearance.tint_alpha != prefs.appearance.tint_alpha
    {
        crate::apply_backdrops(
            &app,
            prefs.appearance.backdrop,
            prefs.appearance.tint_alpha,
        );
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
            },
            Binding {
                accelerator: "Ctrl+Alt+Z".into(),
                action: PRIMARY.into(),
                source: Source::Command {
                    id: "app:zed".into(),
                },
                replace: false,
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
