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
pub fn clipboard_entry(clipboard: State<'_, Clipboard>, id: i64) -> Result<Option<Detail>, String> {
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
        .and_then(crate::icons::icon_data_uri);

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
) -> Result<(), String> {
    let (text, image) = {
        let store = clipboard.store();
        let entry = store
            .get(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "That entry is no longer in the history".to_string())?;
        let image = if entry.kind == Kind::Image {
            store.blob(id).map_err(|e| e.to_string())?
        } else {
            None
        };
        store.touch(id, now_seconds()).map_err(|e| e.to_string())?;
        (entry.text, image)
    };

    // The watcher would otherwise see Sill's own write and move the entry to
    // the top of the history, reordering the list under the user's hands.
    clipboard.ignore_next();

    let mut board = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match image {
        Some(png) => write_image(&mut board, &png)?,
        None => board.set_text(text).map_err(|e| e.to_string())?,
    }

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

/// Decodes a stored PNG and puts the pixels back on the clipboard.
fn write_image(board: &mut arboard::Clipboard, png: &[u8]) -> Result<(), String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buffer).map_err(|e| e.to_string())?;
    buffer.truncate(info.buffer_size());

    board
        .set_image(arboard::ImageData {
            width: info.width as usize,
            height: info.height as usize,
            bytes: buffer.into(),
        })
        .map_err(|e| e.to_string())
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
