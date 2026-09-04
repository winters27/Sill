//! The notes window's side of the boundary.
//!
//! Three adapters over [`crate::notes`] and one function that puts the window
//! on screen. Everything that decides lives there: what a note is called, which
//! ones a query asks for, and what happens to the file when it cannot be read.
//!
//! Nothing here caches. The service holds the list once it has been read and
//! these commands ask it, so the window and the launcher are looking at one
//! answer rather than two copies of it.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::notes::{Note, Notes};
use crate::state::{now_seconds, PrefsState};

/// The window every note is opened in. One, for a prototype.
const WINDOW: &str = "note";

/// What the window is told to open, once it exists.
///
/// A separate event from the window being built because the two happen in
/// either order: the first note builds the window and then says which note,
/// and every note after that finds a window already loaded and only has to
/// say. The page asks as well, on mount, for the same reason `FirstRun` does.
const OPENING: &str = "sill://note";

/**
Puts the notes window up on one note.

Built on demand rather than declared, which is what `lazy_windows` exists for:
a window nobody has opened costs a renderer, and this one is behind a switch
that is off, so declaring it would have every machine paying about 82 MB for a
prototype most of them have not switched on.
*/
pub fn show_note(app: &AppHandle, id: &str) -> Result<(), String> {
    let window = crate::lazy_windows::ensure(app, WINDOW)?;

    window
        .show()
        .map_err(|err| format!("could not show the notes window: {err}"))?;
    let _ = window.set_focus();

    // Scoped by label rather than broadcast. `emit` in Tauri 2 reaches every
    // window, and a launcher listening for this would be a launcher that
    // redrew itself whenever a note was opened.
    let _ = window.emit(OPENING, id.to_string());

    Ok(())
}

/// Whether notes are switched on.
///
/// Asked by every command here for the reason the actions ask it: these are
/// reachable by anything that can invoke, and a prototype behind a switch has
/// to be behind it on every path.
async fn switched_on(prefs: &State<'_, PrefsState>) -> Result<(), String> {
    if prefs.inner.lock().await.general.notes {
        return Ok(());
    }

    Err("Notes are switched off.".to_string())
}

/// One note, or nothing if it has been forgotten since the window opened.
#[tauri::command]
pub(crate) async fn note_read(
    app: AppHandle,
    prefs: State<'_, PrefsState>,
    id: String,
) -> Result<Option<Note>, String> {
    switched_on(&prefs).await?;
    Ok(app.state::<Notes>().one(&app, &id))
}

/**
Saves what is in the window.

Returns the note rather than nothing, because a note that did not exist a
moment ago now has an id and the window needs it: without that, a second save
of a note the window thought was new would make a second note.
*/
#[tauri::command]
pub(crate) async fn note_write(
    app: AppHandle,
    prefs: State<'_, PrefsState>,
    id: String,
    text: String,
) -> Result<Note, String> {
    switched_on(&prefs).await?;
    app.state::<Notes>().write(&app, &id, &text, now_seconds())
}

/// Removes one.
///
/// `false` when there was nothing to remove, which is not an error: two
/// deletes of the same note end in the state that was asked for.
#[tauri::command]
pub(crate) async fn note_forget(
    app: AppHandle,
    prefs: State<'_, PrefsState>,
    id: String,
) -> Result<bool, String> {
    switched_on(&prefs).await?;
    app.state::<Notes>().forget(&app, &id)
}
