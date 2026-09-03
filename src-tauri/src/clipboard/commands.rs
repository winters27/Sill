//! The command layer for clipboard history.

use base64::Engine;
use tauri::{AppHandle, Manager, State};

use crate::clipboard::kind::Kind;
use crate::clipboard::monitor::{now_seconds, Clipboard};
use crate::clipboard::store::Entry;

/// How many rows a query returns. The list is virtualized, but a query that
/// walked a hundred thousand rows on every keystroke would not be.
const LIMIT: usize = 400;

/// One entry plus everything the preview pane shows about it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    #[serde(flatten)]
    entry: Entry,
    /// A PNG data URI, for an image entry.
    image: Option<String>,
    /// The source application's icon, as a data URI.
    ///
    /// Resolved here rather than stored: an icon changes when the application
    /// updates, and the path is the durable thing to keep.
    app_icon: Option<String>,
}

#[tauri::command]
pub fn clipboard_search(
    clipboard: State<'_, Clipboard>,
    query: String,
    kind: Option<String>,
) -> Result<Vec<Entry>, String> {
    let filter = kind.as_deref().filter(|k| *k != "all").map(Kind::from_str);
    let out = clipboard
        .store()
        .search(&query, filter, LIMIT)
        .map_err(|e| e.to_string());
    out
}

/// One entry in full, with its image when it has one.
///
/// Separate from the list because a listing of four hundred rows must not
/// carry four hundred screenshots with it.
#[tauri::command]
pub fn clipboard_entry(
    clipboard: State<'_, Clipboard>,
    icons: State<'_, crate::icons::Icons>,
    id: i64,
) -> Result<Option<Detail>, String> {
    // The window asks for exactly the row it is showing, once, as the
    // selection settles. That makes this the one place that already knows
    // which entry somebody is looking at, and the count cap needs to know so
    // it does not delete that row out from under them. Nothing new crosses
    // the boundary to say so.
    clipboard.now_viewing(id);

    let store = clipboard.store();
    let Some(entry) = store.get(id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };

    let image = if entry.kind == Kind::Image {
        store.blob(id).map_err(|e| e.to_string())?.map(|bytes| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(bytes)
            )
        })
    } else {
        None
    };

    let app_icon = entry
        .app_path
        .as_deref()
        .and_then(|path| icons.data_uri(path));

    Ok(Some(Detail {
        entry,
        image,
        app_icon,
    }))
}

/// Puts an entry back on the clipboard and pastes it into whatever has focus.
///
/// The launcher is dismissed first: it is frontmost right now, and a paste
/// aimed at it would land in the search field.
#[tauri::command]
pub async fn clipboard_paste(
    app: AppHandle,
    clipboard: State<'_, Clipboard>,
    id: i64,
    paste: bool,
    // Put back only the text, dropping any formatting that was kept. This is
    // the reason formatting is worth storing at all: without it, "paste as
    // plain text" is not an option anybody can offer, because everything
    // already is plain.
    //
    // The window sends `plainText`; Tauri matches it to this name.
    plain_text: Option<bool>,
) -> Result<(), String> {
    let plain = plain_text.unwrap_or(false);

    // What this entry actually is, decided in one place. An image row's text
    // is a caption, so anything that reads `entry.text` and copies it hands
    // back the words "Image 1920x1080" instead of the picture.
    let payload = {
        let store = clipboard.store();
        let payload = crate::clipboard::write::payload_for(&store, id, plain)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "That entry is no longer in the history".to_string())?;
        store.touch(id, now_seconds()).map_err(|e| e.to_string())?;
        payload
    };

    // The watcher would otherwise see Sill's own write and move the entry to
    // the top of the history, reordering the list under the user's hands.
    clipboard.ignore_next();

    let mut board = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    crate::clipboard::write::put(&mut board, &payload)?;

    if !paste {
        return Ok(());
    }

    if let Some(window) = app.get_webview_window("main") {
        crate::summon::hide(&window);
    }

    // The same settle the dictation paste needs: writing and immediately
    // pasting races the target application's read of the clipboard.
    std::thread::sleep(std::time::Duration::from_millis(60));
    crate::dictation::paste::chord();
    Ok(())
}

#[tauri::command]
pub fn clipboard_pin(clipboard: State<'_, Clipboard>, id: i64, pinned: bool) -> Result<(), String> {
    clipboard
        .store()
        .set_pinned(id, pinned)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_delete(clipboard: State<'_, Clipboard>, id: i64) -> Result<(), String> {
    clipboard.store().delete(id).map_err(|e| e.to_string())
}

/// Empties the history. Pinned entries survive unless `everything`.
#[tauri::command]
pub fn clipboard_clear(clipboard: State<'_, Clipboard>, everything: bool) -> Result<usize, String> {
    clipboard
        .store()
        .clear(everything)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_count(clipboard: State<'_, Clipboard>) -> Result<i64, String> {
    clipboard.store().count().map_err(|e| e.to_string())
}

/// What was last declined for looking like a credential, if anything.
#[tauri::command]
pub fn clipboard_last_skipped(
    clipboard: State<'_, Clipboard>,
) -> Option<crate::clipboard::monitor::Skipped> {
    clipboard.last_skipped()
}

/// Records what is on the clipboard now, credential-looking or not.
///
/// The way back when the detector was wrong. Nothing was held anywhere to make
/// this work: the entry is still on the clipboard, so this reads it again.
#[tauri::command]
pub fn clipboard_keep_current(
    app: tauri::AppHandle,
    clipboard: State<'_, Clipboard>,
) -> Result<(), String> {
    crate::clipboard::monitor::keep_current(&app, &clipboard)
}

/// Several entries joined into one piece of text.
///
/// Built from ids rather than in the window from rows it already has, because
/// the list the user picked from is not necessarily the list still on screen:
/// a filter, a new copy or a deletion can have changed it in between. Reading
/// the entries again means the result is what was chosen.
#[tauri::command]
pub fn clipboard_merge(
    clipboard: State<'_, Clipboard>,
    ids: Vec<i64>,
    separator: String,
) -> Result<String, String> {
    clipboard
        .store()
        .merge(&ids, &separator)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "nothing left to merge".to_string())
}

// ------------------------------------------------------------- collections

#[tauri::command]
pub fn clipboard_collections(
    clipboard: State<'_, Clipboard>,
) -> Result<Vec<crate::clipboard::store::Collection>, String> {
    clipboard.store().collections().map_err(|e| e.to_string())
}

/// Makes a collection, or returns the one already called that.
#[tauri::command]
pub fn clipboard_create_collection(
    clipboard: State<'_, Clipboard>,
    name: String,
) -> Result<i64, String> {
    if name.trim().is_empty() {
        return Err("a collection needs a name".to_string());
    }

    clipboard
        .store()
        .create_collection(&name, now_seconds())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_rename_collection(
    clipboard: State<'_, Clipboard>,
    id: i64,
    name: String,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("a collection needs a name".to_string());
    }

    clipboard
        .store()
        .rename_collection(id, &name)
        .map_err(|e| e.to_string())
}

/// Removes a collection. The entries in it are untouched.
#[tauri::command]
pub fn clipboard_delete_collection(clipboard: State<'_, Clipboard>, id: i64) -> Result<(), String> {
    clipboard
        .store()
        .delete_collection(id)
        .map_err(|e| e.to_string())
}

/// Puts entries into a collection, keeping the order they were given in.
#[tauri::command]
pub fn clipboard_add_to_collection(
    clipboard: State<'_, Clipboard>,
    collection: i64,
    ids: Vec<i64>,
) -> Result<usize, String> {
    clipboard
        .store()
        .add_to_collection(collection, &ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_remove_from_collection(
    clipboard: State<'_, Clipboard>,
    collection: i64,
    id: i64,
) -> Result<(), String> {
    clipboard
        .store()
        .remove_from_collection(collection, id)
        .map_err(|e| e.to_string())
}

/// What is in a collection, in the order it was arranged.
#[tauri::command]
pub fn clipboard_collection_entries(
    clipboard: State<'_, Clipboard>,
    collection: i64,
) -> Result<Vec<Entry>, String> {
    clipboard
        .store()
        .collection_entries(collection)
        .map_err(|e| e.to_string())
}
