//! The command layer for snippets.

use tauri::{AppHandle, State};

use crate::snippets::expander::Expander;
use crate::snippets::placeholder::{self, Context, Expansion};
use crate::snippets::store::{self, Snippet};

/// The snippets, in the order they are shown: most used first.
#[tauri::command]
pub fn list_snippets(app: AppHandle) -> Vec<Snippet> {
    let mut snippets = store::load(&store::path(&app));
    snippets.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));
    snippets
}

/// Adds or replaces one, refusing a keyword another snippet already uses.
#[tauri::command]
pub fn save_snippet(
    app: AppHandle,
    expander: State<'_, Expander>,
    snippet: Snippet,
) -> Result<(), String> {
    let file = store::path(&app);
    let mut snippets = store::load(&file);

    if !store::keyword_is_free(&snippets, &snippet.id, &snippet.keyword) {
        return Err(format!(
            "Another snippet already expands on “{}”",
            snippet.keyword.trim()
        ));
    }

    let mut snippet = snippet;
    if snippet.id.is_empty() {
        snippet.id = new_id();
    }
    if snippet.created == 0 {
        snippet.created = now_seconds();
    }

    store::upsert(&mut snippets, snippet);
    store::save(&file, &snippets).map_err(|e| e.to_string())?;

    // Both the hook and the search index cache these, and neither has a route
    // back to the file, so a change has to be pushed to both or a new snippet
    // is invisible to one of them.
    let _ = expander;
    crate::reload_snippets(&app);
    Ok(())
}

#[tauri::command]
pub fn delete_snippet(
    app: AppHandle,
    expander: State<'_, Expander>,
    id: String,
) -> Result<(), String> {
    let file = store::path(&app);
    let mut snippets = store::load(&file);
    snippets.retain(|snippet| snippet.id != id);

    store::save(&file, &snippets).map_err(|e| e.to_string())?;
    let _ = expander;
    crate::reload_snippets(&app);
    Ok(())
}

/// Writes every snippet to a file somebody chooses.
///
/// A dialog rather than a fixed location, because the point of an export is
/// that it goes somewhere the person can find it again.
#[tauri::command]
pub fn export_snippets(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let snippets = store::load(&store::path(&app));
    if snippets.is_empty() {
        return Err("There are no snippets to export.".to_string());
    }

    let chosen = app
        .dialog()
        .file()
        .set_title("Export snippets")
        .set_file_name("snippets.json")
        .add_filter("Snippets", &["json"])
        .blocking_save_file();

    // Nothing chosen is not a failure. Somebody opened the dialog and changed
    // their mind, which is an ordinary thing to do and needs no message.
    let Some(target) = chosen else {
        return Ok(None);
    };

    let path = target
        .into_path()
        .map_err(|err| format!("that location cannot be written to: {err}"))?;

    std::fs::write(&path, super::transfer::to_json(&snippets))
        .map_err(|err| format!("could not write that file: {err}"))?;

    Ok(Some(path.to_string_lossy().into_owned()))
}

/// Reads snippets from a file and folds them into the ones already here.
///
/// Additive, always. Whatever the file contains, every snippet already held is
/// still there afterwards: an import that could quietly delete somebody's
/// collection is not something to offer behind a single button.
#[tauri::command]
pub fn import_snippets(
    app: AppHandle,
    expander: State<'_, Expander>,
) -> Result<Option<super::transfer::Summary>, String> {
    use tauri_plugin_dialog::DialogExt;

    let chosen = app
        .dialog()
        .file()
        .set_title("Import snippets")
        .add_filter("Snippets", &["json"])
        .blocking_pick_file();

    let Some(source) = chosen else {
        return Ok(None);
    };

    let path = source
        .into_path()
        .map_err(|err| format!("that file cannot be read: {err}"))?;

    let text = std::fs::read_to_string(&path)
        .map_err(|err| format!("could not read that file: {err}"))?;

    let arriving = super::transfer::parse(&text)?;
    if arriving.is_empty() {
        return Err("That file has no snippets in it.".to_string());
    }

    let file = store::path(&app);
    let (merged, summary) = super::transfer::merge(&store::load(&file), arriving, now_seconds());

    store::save(&file, &merged).map_err(|err| err.to_string())?;
    let _ = expander;
    crate::reload_snippets(&app);

    Ok(Some(summary))
}

/// Fills in a snippet's placeholders, ready to paste or type.
#[tauri::command]
pub fn expand_snippet(app: AppHandle, id: String) -> Result<Expansion, String> {
    let file = store::path(&app);
    let mut snippets = store::load(&file);

    let snippet = snippets
        .iter_mut()
        .find(|snippet| snippet.id == id)
        .ok_or_else(|| "That snippet no longer exists".to_string())?;

    snippet.uses = snippet.uses.saturating_add(1);
    let content = snippet.content.clone();
    let _ = store::save(&file, &snippets);

    Ok(placeholder::expand(&content, &context(&app, &content)))
}

/// Expands a snippet and types it where the keyword was.
///
/// Typed rather than pasted, so the clipboard is left exactly as the user had
/// it. A snippet that silently replaced what was on the clipboard would be a
/// poor trade for a few milliseconds.
#[tauri::command]
pub fn type_snippet(
    app: AppHandle,
    expander: State<'_, Expander>,
    id: String,
    backspaces: usize,
) -> Result<(), String> {
    let expansion = expand_snippet(app, id)?;

    #[cfg(windows)]
    {
        crate::snippets::expander::replace(&expander, backspaces, &expansion.text);

        // After the text is in, walk the caret back to where the snippet
        // asked for it.
        if let Some(at) = expansion.cursor {
            let trailing = expansion.text.chars().count().saturating_sub(at);
            crate::snippets::expander::move_caret_back(trailing);
        }
    }

    #[cfg(not(windows))]
    let _ = (expander, backspaces, expansion);

    Ok(())
}

/// Everything the placeholders can ask about the world.
///
/// The clipboard is only read when the template mentions it: every expansion
/// paying a Win32 round trip for a placeholder almost none of them use would
/// be a poor trade.
///
/// Shared with quicklinks rather than copied, so both speak the same grammar
/// and gain the same tokens at the same time.
pub fn context(app: &AppHandle, template: &str) -> Context {
    let clipboard = if placeholder::needs_clipboard(template) {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().read_text().unwrap_or_default()
    } else {
        String::new()
    };

    let clock = local_clock();
    let (date, time) = (
        clock.format("YYYY-MM-DD"),
        clock.format("HH:mm"),
    );

    // Asked for only when the template says so. Reading a selection sends a
    // copy chord and takes the clipboard over for a moment, which is far too
    // rude to do on the chance that a snippet might have wanted it.
    let selection = if placeholder::needs_selection(template) {
        crate::selection::capture(app).unwrap_or_default()
    } else {
        String::new()
    };

    Context {
        clipboard,
        date,
        time,
        uuid: new_id(),
        // A snippet expands where the caret already is, so there is nowhere
        // to have asked for one. Quicklinks are the caller that fills this.
        query: String::new(),
        selection,
        clock,
    }
}

/// The local time, broken down.
///
/// Read from the system rather than through a date crate: Windows hands back a
/// broken-down local time directly, so the only work left is arranging the
/// numbers, and `Clock::format` is where that happens.
#[cfg(windows)]
fn local_clock() -> placeholder::Clock {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: fills an owned struct and takes nothing.
    let now = unsafe { GetLocalTime() };

    placeholder::Clock {
        year: now.wYear,
        month: now.wMonth,
        day: now.wDay,
        hour: now.wHour,
        minute: now.wMinute,
        second: now.wSecond,
        weekday: now.wDayOfWeek,
    }
}

#[cfg(not(windows))]
fn local_clock() -> placeholder::Clock {
    placeholder::Clock::default()
}

/// A short unique id, for a snippet and for `{uuid}`.
///
/// Built from the clock and the address of a fresh allocation rather than
/// pulling in a uuid crate: this needs to be unique among a person's few
/// dozen snippets, not globally unique across the internet.
pub fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let boxed = Box::new(0u8);
    let address = Box::into_raw(boxed) as usize;
    // SAFETY: the pointer came from `Box::into_raw` and is reclaimed once.
    unsafe {
        drop(Box::from_raw(address as *mut u8));
    }

    format!("{nanos:x}{address:x}")
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_do_not_repeat() {
        let ids: std::collections::HashSet<String> = (0..500).map(|_| new_id()).collect();
        assert_eq!(ids.len(), 500, "every id should be distinct");
    }

    #[cfg(windows)]
    #[test]
    fn the_date_and_time_are_the_shapes_the_placeholders_promise() {
        let now = local_clock();
        let date = now.format("YYYY-MM-DD");
        let time = now.format("HH:mm");

        assert_eq!(date.len(), 10, "YYYY-MM-DD, got {date}");
        assert_eq!(date.matches('-').count(), 2);
        assert_eq!(time.len(), 5, "HH:MM, got {time}");
        assert_eq!(time.matches(':').count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn the_clock_is_read_from_the_machine_rather_than_invented() {
        // A default `Clock` formats as year zero, which would look like a
        // working date placeholder while being nothing of the kind.
        let now = local_clock();

        assert!(now.year >= 2024, "year came back as {}", now.year);
        assert!((1..=12).contains(&now.month), "month {}", now.month);
        assert!((1..=31).contains(&now.day), "day {}", now.day);
        assert!(now.hour < 24 && now.minute < 60 && now.second < 60);
        assert!(now.weekday < 7, "weekday {}", now.weekday);
    }
}
