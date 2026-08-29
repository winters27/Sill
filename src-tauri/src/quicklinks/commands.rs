//! What the launcher and the settings window can ask of quicklinks.

use tauri::AppHandle;

use crate::quicklinks::resolve;
use crate::quicklinks::store::{self, Quicklink};
use crate::snippets::commands::context;

/// Everything saved, newest first.
#[tauri::command]
pub fn list_quicklinks(app: AppHandle) -> Vec<Quicklink> {
    let dir = store::data_dir(&app);
    store::load(&store::path(&dir))
}

/// Adds or replaces one, and returns the saved list.
///
/// Upsert rather than separate add and update, because the editor does not
/// know which it is doing: a link being edited and a link being created are
/// the same form with the same fields.
#[tauri::command]
pub fn save_quicklink(app: AppHandle, link: Quicklink) -> Result<Vec<Quicklink>, String> {
    let dir = store::data_dir(&app);
    let path = store::path(&dir);
    let mut links = store::load(&path);

    let mut link = link;
    if link.id.is_empty() {
        link.id = crate::snippets::commands::new_id();
        link.created = crate::now_seconds();
    }

    match links.iter().position(|existing| existing.id == link.id) {
        Some(at) => {
            // Kept from the stored copy: the editor never sees these, and
            // taking them from the form would reset a link's history every
            // time somebody fixed a typo in its name.
            link.uses = links[at].uses;
            link.created = links[at].created;
            links[at] = link;
        }
        None => links.insert(0, link),
    }

    store::save(&path, &links).map_err(|e| format!("Could not save the quicklink: {e}"))?;
    crate::reload_index(&app);
    Ok(links)
}

#[tauri::command]
pub fn delete_quicklink(app: AppHandle, id: String) -> Result<Vec<Quicklink>, String> {
    let dir = store::data_dir(&app);
    let path = store::path(&dir);
    let mut links = store::load(&path);
    links.retain(|link| link.id != id);

    store::save(&path, &links).map_err(|e| format!("Could not save the quicklinks: {e}"))?;
    crate::reload_index(&app);
    Ok(links)
}

/// Opens one, with `query` filling `{query}`.
///
/// Returns the resolved target so the caller can show what it opened, which
/// is the only way to tell a link that went somewhere unexpected from one
/// that did not open at all.
#[tauri::command]
pub fn open_quicklink(app: AppHandle, id: String, query: String) -> Result<String, String> {
    let dir = store::data_dir(&app);
    let path = store::path(&dir);
    let mut links = store::load(&path);

    let at = links
        .iter()
        .position(|link| link.id == id)
        .ok_or_else(|| "That quicklink no longer exists".to_string())?;

    let link = links[at].clone();
    let mut filled = context(&app, &link.link);
    filled.query = query;
    let target = resolve::resolve(&link.link, &filled);

    open(&target, &link.open_with)?;

    links[at].uses += 1;
    if let Err(err) = store::save(&path, &links) {
        // A use count is not worth failing an open over.
        crate::say!("could not record the quicklink use: {err}");
    }

    Ok(target)
}

/// Hands the target to a specific application, or to the shell.
fn open(target: &str, open_with: &str) -> Result<(), String> {
    if open_with.trim().is_empty() {
        return tauri_plugin_opener::open_url(target, None::<&str>)
            .or_else(|_| tauri_plugin_opener::open_path(target, None::<&str>))
            .map_err(|e| format!("Could not open {target}: {e}"));
    }

    // Named application: passed as an argument rather than through the shell,
    // so a target containing `&` or a space reaches the browser whole.
    std::process::Command::new(open_with)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open {target} with {open_with}: {e}"))
}
