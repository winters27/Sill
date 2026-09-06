//! Everything the launcher can do to itself or to the machine.

use crate::reload_index;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::registry::Frecency;
use crate::state::{data_dir, RegistryState};
use crate::{icons, log, summon};

/// Rescans every enabled source.
///
/// Returns as soon as the scan is queued rather than waiting for it: the
/// launcher keeps answering from the old index and re-queries when
/// `sill://registry-updated` lands.
#[tauri::command]
pub(crate) fn rebuild_index(app: AppHandle) {
    reload_index(&app);
}

/// Summons the launcher, optionally with something to run on arrival.
///
/// The notification-area menu is a separate window, so choosing an entry there
/// is an intent expressed from outside the launcher. The command is a thin
/// adapter over `summon::show_with`; which screen "clipboard" means is the
/// page's business, not Rust's.
#[tauri::command]
pub(crate) fn summon_with(app: AppHandle, command: Option<String>) {
    summon::show_with(&app, command);
}

/// Opens the log in whatever reads a text file.
#[tauri::command]
pub(crate) fn open_log() -> Result<(), String> {
    let path = log::path().ok_or_else(|| "The log has not been opened".to_string())?;
    let path = crate::reach::target(&path.to_string_lossy())?;

    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

/// Reveals the folder holding preferences, the index cache and the log.
#[tauri::command]
pub(crate) fn open_data_folder(app: AppHandle) -> Result<(), String> {
    let dir = data_dir(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let dir = crate::reach::target(&dir.to_string_lossy())?;

    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

/// Forgets which entries have been launched, so ranking starts over.
#[tauri::command]
pub(crate) async fn clear_usage_history(registry: State<'_, RegistryState>) -> Result<(), String> {
    let (path, text) = registry.record(|ranking| {
        ranking.frecency = Frecency::default();
        ranking.path.clone()
    });

    let text = text.ok_or_else(|| "could not write the ranking history".to_string())?;
    crate::registry::Frecency::write(&path, &text).map_err(|e| e.to_string())
}

/// The icon for a launchable, as a data URI.
///
/// Requested lazily per row rather than resolved for the whole index: a
/// machine has hundreds of Start Menu entries and only a handful are ever on
/// screen. Results are cached, misses included.
#[tauri::command]
pub(crate) async fn app_icon(
    icons: State<'_, icons::Icons>,
    path: String,
) -> Result<Option<String>, String> {
    // A `Result` because the command takes a borrowed `State`, and Tauri
    // requires that of any async command that does. Nothing here fails.
    Ok(icons.data_uri(&path))
}

/// Closes Sill entirely.
///
/// A launcher is normally dismissed rather than quit, so this is deliberately
/// only reachable from the menu: there is no accidental path to it.
#[tauri::command]
pub(crate) fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Puts the launcher away. Bound to Escape in the UI.
#[tauri::command]
pub(crate) fn dismiss(window: tauri::WebviewWindow) {
    summon::hide(&window);
}

/// Puts the picking overlay over every screen.
///
/// Sized and placed in physical pixels, deliberately. The overlay has to line
/// up with the screen exactly or the rectangle somebody drags is not the
/// rectangle that gets copied, and logical pixels differ from physical ones by
/// the display's scaling. On a desk with a 150% display and a 100% one there is
/// no single scale that would work.
#[tauri::command]
pub(crate) async fn begin_capture(app: AppHandle) -> Result<(), String> {
    // Refused here as well as at the picture, which is not the same check
    // twice. The overlay is a full-screen thing somebody drags a rectangle
    // across, and putting it up in order to say no at the end of it would be
    // a worse way of saying the same word.
    crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;

    // Built on the first capture rather than declared, so an overlay nobody
    // uses this session costs nothing.
    let window = crate::lazy_windows::ensure(&app, "capture")?;

    let (left, top, width, height) = crate::capture::virtual_screen();
    if width <= 0 || height <= 0 {
        return Err("no screens were found".to_string());
    }

    // The launcher stays where it is, and in the picture if that is what
    // somebody is after; a screenshot of Sill is a screenshot like any other.
    // The overlay is about to take the keyboard, which would otherwise read
    // as clicking away.
    crate::keep_main_through_capture(&app);

    window
        .set_position(tauri::PhysicalPosition::new(left, top))
        .map_err(|err| format!("could not place the overlay: {err}"))?;
    window
        .set_size(tauri::PhysicalSize::new(width as u32, height as u32))
        .map_err(|err| format!("could not size the overlay: {err}"))?;

    /*
     * Woken before it is shown, and this line is not optional.
     *
     * A hidden window's renderer is made invisible twenty seconds after it
     * hides (`sleep.rs`), and a renderer made invisible does not paint and
     * does not take input. Showing this window without undoing that put an
     * always-on-top, transparent, virtual-screen-sized window over both
     * screens with nothing behind it: the pointer moved, sound played, and
     * nothing could be clicked or typed, including the Escape that closes
     * the overlay, because there was no page to receive it. The first
     * capture of a session worked, because the window was new; the second,
     * twenty seconds or more after the first, took the machine.
     */
    crate::sleep::wake(&window);
    window
        .show()
        .map_err(|err| format!("could not show the overlay: {err}"))?;

    // Focus, because the overlay reads Escape and the mouse. It is the one
    // window here that genuinely wants the keyboard.
    let _ = window.set_focus();
    #[cfg(windows)]
    if let Ok(handle) = window.hwnd() {
        crate::summon::force_foreground(windows::Win32::Foundation::HWND(
            handle.0 as *mut core::ffi::c_void,
        ));
    }

    Ok(())
}

/// The windows the picker can offer, topmost first.
///
/// Asked for once when the overlay opens rather than per pointer move: a
/// window list is a Win32 enumeration and the desk does not rearrange itself
/// while somebody is choosing.
///
/// The overlay itself is left out, or it would be the answer everywhere.
#[tauri::command]
pub(crate) async fn capture_targets(app: AppHandle) -> Result<Vec<CaptureTarget>, String> {
    let ours: Vec<isize> = ["capture", "main", "markup", "traymenu", "settings"]
        .iter()
        .filter_map(|label| app.get_webview_window(label))
        .filter_map(|window| window.hwnd().ok())
        .map(|handle| handle.0 as isize)
        .collect();

    Ok(crate::windowing::list()
        .into_iter()
        .filter(|window| !window.minimized && !ours.contains(&window.id))
        .map(|window| CaptureTarget {
            id: window.id,
            title: window.title,
            app: window.app,
            left: window.rect.x,
            top: window.rect.y,
            width: window.rect.width,
            height: window.rect.height,
        })
        .collect())
}

/// A window the picker can capture, in the screen's own pixels.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CaptureTarget {
    pub id: isize,
    pub title: String,
    pub app: String,
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

/// Copies one window, whole, even where something is sitting on top of it.
#[tauri::command]
pub(crate) async fn capture_window(app: AppHandle, id: isize) -> Result<String, String> {
    crate::lazy_windows::hide(&app, "capture");
    crate::capture_over(&app);
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    // Read again rather than trusting what the overlay was told: a handle can
    // be reused by a different window after the first one closes, and the one
    // it was holding may have moved while somebody was choosing.
    let found = crate::windowing::find(id).ok_or_else(|| "that window has gone".to_string())?;
    let rect = (
        found.rect.x,
        found.rect.y,
        found.rect.width,
        found.rect.height,
    );

    // Asked before the picture rather than after it, and asked of the one
    // thing that can say yes. `Allowed` cannot be made anywhere else, so this
    // line is not a check somebody remembered to write: without it the call
    // below does not compile.
    let allowed = crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;

    let shot = tokio::task::spawn_blocking(move || crate::capture::window(&allowed, id, rect))
        .await
        .map_err(|err| format!("the capture failed: {err}"))??;

    let size = format!("{}x{}", shot.width, shot.height);
    let named = if found.title.is_empty() {
        found.app
    } else {
        found.title
    };

    after_capture(&app, shot).await?;
    Ok(format!("Copied {named}, {size}"))
}

/// Copies one whole display.
#[tauri::command]
pub(crate) async fn capture_display(app: AppHandle, index: usize) -> Result<String, String> {
    crate::lazy_windows::hide(&app, "capture");
    crate::capture_over(&app);
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    let screen = crate::windowing::monitors()
        .into_iter()
        .find(|monitor| monitor.index == index)
        .ok_or_else(|| "there is no display with that number".to_string())?;

    let rect = screen.full;
    let allowed = crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;
    let shot = tokio::task::spawn_blocking(move || {
        crate::capture::region(&allowed, rect.x, rect.y, rect.width, rect.height)
    })
    .await
    .map_err(|err| format!("the capture failed: {err}"))??;

    let size = format!("{}x{}", shot.width, shot.height);
    after_capture(&app, shot).await?;

    Ok(format!("Copied display {}, {size}", index + 1))
}

/// Takes the overlay away without capturing anything.
#[tauri::command]
pub(crate) async fn cancel_capture(app: AppHandle) -> Result<(), String> {
    // Whoever was waiting for a choice hears that there will not be one:
    // dropping their sender is what their receiver reads as cancelled.
    forget_choice(&app);
    crate::lazy_windows::hide(&app, "capture");
    crate::capture_over(&app);

    Ok(())
}

/// What the overlay is up for.
///
/// One overlay, four askers. Copying a picture is the ordinary one. The
/// other three want the rectangle handed back to whoever put the overlay up
/// rather than copied anywhere: a region for the model to read, a pixel to
/// name the colour of, a code to decode. The overlay asks which on show,
/// because only the asker knows and a window cannot be given an argument
/// when it is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Purpose {
    #[default]
    Copy,
    Choose,
    Colour,
    Qr,
}

/// A rectangle of the screen, in the screen's own physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Region {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
}

/// Whoever is waiting for the overlay to hand a rectangle back.
///
/// One at a time, like [`Marking`]: there is one overlay, so a second asker
/// replaces the first, whose receiver then reads the dropped sender as a
/// cancellation rather than waiting on a choice nobody can make any more.
#[derive(Default)]
pub(crate) struct Choosing(
    pub std::sync::Mutex<Option<(Purpose, tokio::sync::oneshot::Sender<Region>)>>,
);

/// How long a choice is waited for before the overlay is taken down.
const CHOOSE_PATIENCE: std::time::Duration = std::time::Duration::from_secs(60);

fn forget_choice(app: &AppHandle) {
    if let Some(choosing) = app.try_state::<Choosing>() {
        if let Ok(mut held) = choosing.0.lock() {
            *held = None;
        }
    }
}

/// Which purpose the overlay is up for right now. Copying, unless somebody
/// is waiting for a choice.
#[tauri::command]
pub(crate) fn capture_purpose(choosing: State<'_, Choosing>) -> Purpose {
    choosing
        .0
        .lock()
        .ok()
        .and_then(|held| held.as_ref().map(|(purpose, _)| *purpose))
        .unwrap_or_default()
}

/// The overlay handing a rectangle back to whoever asked for one.
///
/// Hidden first and then waited on, exactly as `capture_area` does: the asker
/// is about to read the screen, and the overlay is a window like any other.
#[tauri::command]
pub(crate) async fn chose_area(
    app: AppHandle,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) -> Result<(), String> {
    crate::lazy_windows::hide(&app, "capture");
    crate::capture_over(&app);
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    let waiting = {
        let choosing = app.state::<Choosing>();
        let mut held = choosing
            .0
            .lock()
            .map_err(|_| "choosing slot poisoned".to_string())?;
        held.take()
    };

    let nobody = || "nobody was waiting for a choice".to_string();
    let (_, sender) = waiting.ok_or_else(nobody)?;
    sender
        .send(Region {
            left,
            top,
            width,
            height,
        })
        .map_err(|_| nobody())
}

/// Puts the overlay up for a purpose and waits for the rectangle.
///
/// Whoever calls this is somebody about to read the screen, so what comes
/// back is where to read, never a picture: the reading is theirs to take
/// under their own privacy check.
pub(crate) async fn choose_region(app: &AppHandle, purpose: Purpose) -> Result<Region, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();

    {
        let choosing = app.state::<Choosing>();
        let mut held = choosing
            .0
            .lock()
            .map_err(|_| "choosing slot poisoned".to_string())?;
        *held = Some((purpose, sender));
    }

    if let Err(why) = begin_capture(app.clone()).await {
        forget_choice(app);
        return Err(why);
    }

    match tokio::time::timeout(CHOOSE_PATIENCE, receiver).await {
        Ok(Ok(region)) => Ok(region),
        Ok(Err(_)) => Err("nothing was chosen".to_string()),
        Err(_) => {
            forget_choice(app);
            crate::lazy_windows::hide(app, "capture");
            Err("nothing was chosen within a minute".to_string())
        }
    }
}

#[cfg(test)]
mod choosing {
    use super::*;

    /// A second asker replaces the first, and the first hears it at once
    /// rather than waiting for a choice the overlay will never make for it.
    #[test]
    fn a_second_choice_replaces_the_first() {
        use tokio::sync::oneshot::error::TryRecvError;

        let choosing = Choosing::default();
        let (first, mut first_hears) = tokio::sync::oneshot::channel::<Region>();
        let (second, mut second_hears) = tokio::sync::oneshot::channel::<Region>();

        *choosing.0.lock().unwrap() = Some((Purpose::Colour, first));
        *choosing.0.lock().unwrap() = Some((Purpose::Qr, second));

        assert!(matches!(first_hears.try_recv(), Err(TryRecvError::Closed)));

        let (purpose, sender) = choosing.0.lock().unwrap().take().expect("the second");
        assert_eq!(purpose, Purpose::Qr);

        let region = Region {
            left: 10,
            top: 20,
            width: 30,
            height: 40,
        };
        sender.send(region).unwrap();
        assert_eq!(second_hears.try_recv().unwrap(), region);
    }

    #[test]
    fn with_nobody_waiting_the_purpose_is_copying() {
        let choosing = Choosing::default();
        let purpose = choosing
            .0
            .lock()
            .unwrap()
            .as_ref()
            .map(|(purpose, _)| *purpose)
            .unwrap_or_default();

        assert_eq!(purpose, Purpose::Copy);
    }
}

/// Copies a rectangle of the screen, in the screen's own physical pixels.
///
/// The overlay is hidden before anything is read, and the read waits a moment
/// for that to actually happen: the overlay is a window like any other and
/// would otherwise be in its own picture, dimming included.
#[tauri::command]
pub(crate) async fn capture_area(
    app: AppHandle,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) -> Result<String, String> {
    crate::lazy_windows::hide(&app, "capture");
    crate::capture_over(&app);

    // Hiding is a request to the compositor, not something that has happened
    // by the time the call returns. Without this the overlay's dimming is in
    // the picture, which looks like the capture darkened it.
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    let allowed = crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;
    let shot = tokio::task::spawn_blocking(move || {
        crate::capture::region(&allowed, left, top, width, height)
    })
    .await
    .map_err(|err| format!("the capture failed: {err}"))??;

    let size = format!("{}x{}", shot.width, shot.height);
    after_capture(&app, shot).await?;

    Ok(format!("Copied a {size} picture"))
}

/// Copies the whole of every screen at once.
#[tauri::command]
pub(crate) async fn capture_screen(app: AppHandle) -> Result<String, String> {
    // Nothing is hidden first. If the launcher is up, it is part of the
    // screen being copied, which is what "every screen" means.
    let (left, top, width, height) = crate::capture::virtual_screen();

    let allowed = crate::privacy::allow(&app.state::<crate::privacy::Privacy>())?;
    let shot = tokio::task::spawn_blocking(move || {
        crate::capture::region(&allowed, left, top, width, height)
    })
    .await
    .map_err(|err| format!("the capture failed: {err}"))??;

    let size = format!("{}x{}", shot.width, shot.height);
    after_capture(&app, shot).await?;

    Ok(format!("Copied a {size} picture"))
}

/// What happens to a picture once it has been taken.
///
/// One place, so the four ways of taking one cannot disagree about what
/// follows. The clipboard always gets it; whether the editor opens on top is
/// a setting, because somebody who marks up most of what they take should not
/// have to ask for it every time, and somebody who never does should not have
/// a window appear.
async fn after_capture(app: &AppHandle, shot: crate::capture::Shot) -> Result<(), String> {
    let png = shot.to_png()?;
    put_image_on_clipboard(app, shot)?;

    let wanted = {
        let prefs = app.state::<crate::state::PrefsState>();
        let held = prefs.inner.lock().await;
        held.screenshot.after
    };

    if wanted == crate::preferences::AfterCapture::Edit {
        // Straight from the picture rather than back out of the clipboard: the
        // history writes on its own thread and reading it back would be a race
        // with a listener.
        {
            let pending = app.state::<Marking>();
            let mut held = pending
                .0
                .lock()
                .map_err(|_| "markup slot poisoned".to_string())?;
            *held = Some(png);
        }

        if let Ok(window) = crate::lazy_windows::ensure(&app, "markup") {
            let _ = window.emit("sill://markup", ());
            crate::sleep::wake(&window);
            let _ = window.show();
            let _ = window.set_focus();

            #[cfg(windows)]
            if let Ok(handle) = window.hwnd() {
                crate::summon::force_foreground(windows::Win32::Foundation::HWND(
                    handle.0 as *mut core::ffi::c_void,
                ));
            }
        }
    }

    Ok(())
}

/// Hands a picture to the clipboard, which is where everything else finds it.
///
/// Deliberately not saved anywhere. The clipboard history already keeps
/// pictures, already prunes them and already knows how to show one, so a
/// second store of screenshots would be a second thing to manage and a second
/// place for them to pile up.
fn put_image_on_clipboard(app: &AppHandle, shot: crate::capture::Shot) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    // Rgba, which is what the clipboard plugin wants, and what the history
    // stores. The same swap `to_png` does.
    let mut rgba = shot.pixels;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }

    app.clipboard()
        .write_image(&tauri::image::Image::new(
            &rgba,
            shot.width as u32,
            shot.height as u32,
        ))
        .map_err(|err| format!("could not put that picture on the clipboard: {err}"))
}

/// The picture the markup window is currently working on.
///
/// Held rather than passed, because a window cannot be handed an argument when
/// it is shown. It asks for this once it is up.
///
/// One at a time on purpose: there is one markup window, so a second request
/// replaces the first rather than queueing behind it.
#[derive(Default)]
pub(crate) struct Marking(pub std::sync::Mutex<Option<Vec<u8>>>);

/// The row number of the last picture copied.
///
/// The one place that knows, shared with the key bindings, so two ways of
/// saying "the last picture" cannot mean two different pictures.
#[tauri::command]
pub(crate) async fn last_image_entry(app: AppHandle) -> Result<Option<i64>, String> {
    match crate::bindings::last_image(&app) {
        Ok(object) => Ok(object.id.parse().ok()),
        // Nothing copied yet is an ordinary state, not a failure.
        Err(_) => Ok(None),
    }
}

/// Opens the markup window on a picture from the clipboard history.
#[tauri::command]
pub(crate) async fn open_markup(app: AppHandle, entry: i64) -> Result<(), String> {
    let clipboard = app
        .try_state::<crate::clipboard::monitor::Clipboard>()
        .ok_or_else(|| "clipboard history is not running".to_string())?;

    let png = clipboard
        .store()
        .blob(entry)
        .map_err(|err| format!("could not read that entry: {err}"))?
        .ok_or_else(|| "there is no picture on that row".to_string())?;

    {
        let pending = app.state::<Marking>();
        let mut held = pending
            .0
            .lock()
            .map_err(|_| "markup slot poisoned".to_string())?;
        *held = Some(png);
    }

    let window = crate::lazy_windows::ensure(&app, "markup")?;

    crate::dismiss_main(&app);

    // Told after the picture is in place, so a window that is already open
    // swaps to the new one rather than showing the last.
    let _ = window.emit("sill://markup", ());

    crate::sleep::wake(&window);
    window
        .show()
        .map_err(|err| format!("could not show the markup window: {err}"))?;
    let _ = window.set_focus();

    #[cfg(windows)]
    if let Ok(handle) = window.hwnd() {
        crate::summon::force_foreground(windows::Win32::Foundation::HWND(
            handle.0 as *mut core::ffi::c_void,
        ));
    }

    Ok(())
}

/// The picture the markup window should be showing.
///
/// A data URI rather than bytes: it goes straight into an `img`, and the
/// alternative is the window decoding a byte array it was handed over IPC.
#[tauri::command]
pub(crate) async fn markup_image(app: AppHandle) -> Result<Option<String>, String> {
    use base64::Engine;

    let pending = app.state::<Marking>();
    let held = pending
        .0
        .lock()
        .map_err(|_| "markup slot poisoned".to_string())?;

    Ok(held.as_ref().map(|png| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        )
    }))
}

/// Takes the marked-up picture back, and puts it on the clipboard.
#[tauri::command]
pub(crate) async fn finish_markup(app: AppHandle, png: String) -> Result<String, String> {
    use base64::Engine;

    let bytes = png
        .strip_prefix("data:image/png;base64,")
        .ok_or_else(|| "that is not a picture".to_string())
        .and_then(|body| {
            base64::engine::general_purpose::STANDARD
                .decode(body)
                .map_err(|err| format!("that picture could not be read: {err}"))
        })?;

    // Back through the same decode the recogniser uses, so the clipboard gets
    // real pixels rather than a file the plugin would have to parse.
    let (bgra, width, height) = crate::ocr::bgra_from_png(&bytes)?;

    let shot = crate::capture::Shot {
        pixels: bgra,
        width,
        height,
    };

    put_image_on_clipboard(&app, shot)?;

    crate::lazy_windows::hide(&app, "markup");

    // The same tidy-up cancelling does, and for the same reason. Only
    // cancelling did it, so finishing a markup left a full screenshot in
    // memory for the rest of the run: on this machine that is a 2560 by 1440
    // PNG, and it is a picture of somebody's screen.
    forget_marking(&app);

    Ok(format!("Copied a marked-up {width}x{height} picture"))
}

/// Drops the picture waiting to be marked up.
///
/// It is a picture of somebody's screen and nothing needs it once the window
/// is gone. One function because both ways out of that window have to do it
/// and only one of them did.
fn forget_marking(app: &AppHandle) {
    if let Some(pending) = app.try_state::<Marking>() {
        if let Ok(mut held) = pending.0.lock() {
            *held = None;
        }
    }
}

/// Closes the markup window without keeping anything.
#[tauri::command]
pub(crate) async fn cancel_markup(app: AppHandle) -> Result<(), String> {
    crate::lazy_windows::hide(&app, "markup");
    forget_marking(&app);

    Ok(())
}

/// Which downloaded voices are present, for the Speech settings panel.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VoiceStatus {
    pub id: String,
    pub label: String,
    pub locale: String,
    pub note: String,
    pub installed: bool,
    /// What downloading this one costs, engine included when it is the first.
    pub bytes: u64,
}

#[tauri::command]
pub(crate) fn piper_voices(app: tauri::AppHandle) -> Vec<VoiceStatus> {
    crate::tts::piper::VOICES
        .iter()
        .map(|voice| {
            // The engine is fetched once and shared, so only the first
            // download pays for it. Saying "82 MB" beside every voice after
            // the first would be a number nobody is going to be charged.
            let engine_too = !crate::tts::piper::exe(&app).is_file();

            VoiceStatus {
                id: voice.id.to_string(),
                label: voice.label.to_string(),
                locale: voice.locale.to_string(),
                note: voice.note.to_string(),
                installed: crate::tts::piper::is_installed(&app, voice.id),
                bytes: crate::tts::piper::VOICE_BYTES
                    + if engine_too {
                        crate::tts::piper::ENGINE_BYTES
                    } else {
                        0
                    },
            }
        })
        .collect()
}

/// Downloads the speech engine and one voice, reporting progress as it goes.
///
/// Progress is an event rather than a return value, for the reason the model
/// download already is one: a bar that only moves when the call finishes is a
/// bar that never moves.
#[tauri::command]
pub(crate) async fn install_piper_voice(
    app: tauri::AppHandle,
    voice: String,
) -> Result<(), String> {
    use tauri::Emitter;

    let reporting = app.clone();
    crate::tts::piper::install(&app, &voice, move |fraction, stage| {
        let _ = reporting.emit(
            "sill://tts-download",
            serde_json::json!({ "fraction": fraction, "stage": stage }),
        );
    })
    .await
}

#[tauri::command]
pub(crate) fn remove_piper_voice(app: tauri::AppHandle, voice: String) -> Result<bool, String> {
    crate::tts::piper::remove(&app, &voice)
}

/// Says something in whichever voice is set up, for the settings panel's
/// preview button.
#[tauri::command]
pub(crate) async fn speak_sample(app: tauri::AppHandle, text: String) -> Result<(), String> {
    crate::tts::aloud(&app, &text).await
}

/// Speaks in one downloaded voice, whether or not it is the chosen one.
///
/// Its own command rather than reusing `speak_sample`, because the point of
/// the button is to hear a voice **before** picking it. Making somebody select
/// a voice to find out what it sounds like is the thing this avoids.
#[tauri::command]
pub(crate) async fn speak_piper_sample(
    app: tauri::AppHandle,
    voice: String,
    text: String,
) -> Result<(), String> {
    let wav = crate::tts::piper::speak(&app, &voice, &text).await?;
    crate::tts::play_bytes(&app, &wav)
}

/// What Sill has done this run, newest first.
#[tauri::command]
pub(crate) fn activity(app: tauri::AppHandle) -> Vec<crate::activity::Done> {
    use tauri::Manager;
    app.state::<crate::activity::Activity>().recent()
}

/// Takes back one thing, named by its entry rather than by its descriptor.
///
/// The window sends an id, never an undo token. A token is an instruction to
/// change the machine, and one held by the page is one that can be replayed
/// after the log has moved on.
#[tauri::command]
pub(crate) async fn undo_activity(app: tauri::AppHandle, id: u64) -> Result<String, String> {
    use tauri::Manager;

    let undo = app
        .state::<crate::activity::Activity>()
        .take(id)
        .ok_or("That cannot be taken back any more.")?;

    crate::action::undo(&crate::action::ActionCtx::new(app.clone()), &undo)
}

#[tauri::command]
pub(crate) fn clear_activity(app: tauri::AppHandle) {
    use tauri::Manager;
    app.state::<crate::activity::Activity>().clear();
}

/// What the machine is doing, for the readout.
///
/// Blocking: it opens a handle to every process. Off the async runtime so a
/// poll never holds up anything else the window asked for.
#[tauri::command]
pub(crate) async fn machine_reading(
    app: tauri::AppHandle,
) -> Result<crate::meter::Reading, String> {
    // Refused when nothing is on screen. The window layer stops polling when
    // it is told it was hidden, and this is the same rule where a second
    // caller cannot forget it: opening a handle to every process on the
    // machine for a gauge nobody can see is the exact cost this pair of
    // changes exists to remove.
    if !crate::summon::anything_visible(&app) {
        return Err("Nothing is on screen to show a reading in.".to_string());
    }

    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        app.state::<crate::meter::Meter>().read()
    })
    .await
    .map_err(|err| format!("could not read the machine: {err}"))
}

/// Forgets the previous reading, when the readout closes.
///
/// Without this the next reading would be measured against a sample from
/// whenever the view was last open, and an average over an hour would be shown
/// as what is happening now.
#[tauri::command]
pub(crate) fn forget_machine_reading(app: tauri::AppHandle) {
    use tauri::Manager;
    app.state::<crate::meter::Meter>().forget();
}

/// Looks a place up by name, for the weather widget's setting.
#[tauri::command]
pub(crate) async fn find_place(name: String) -> Result<crate::weather::Place, String> {
    crate::weather::find(&name).await
}

/// Throws confetti over every screen.
///
/// The window is sized to the whole virtual screen in physical pixels, the
/// way the capture overlay is, shown, and told to start. It asks to be put
/// away itself once every piece has fallen off the bottom.
#[tauri::command]
pub(crate) async fn throw_confetti(app: AppHandle) -> Result<(), String> {
    let window = crate::lazy_windows::ensure(&app, "confetti")?;

    let (left, top, width, height) = crate::capture::virtual_screen();
    if width <= 0 || height <= 0 {
        return Err("no screens were found".to_string());
    }

    window
        .set_position(tauri::PhysicalPosition::new(left, top))
        .map_err(|err| format!("could not place the confetti: {err}"))?;
    window
        .set_size(tauri::PhysicalSize::new(width as u32, height as u32))
        .map_err(|err| format!("could not size the confetti: {err}"))?;

    crate::sleep::wake(&window);
    window
        .show()
        .map_err(|err| format!("could not show the confetti: {err}"))?;

    // To that window and no other: `emit` reaches every window, and the
    // launcher has no use for this.
    app.emit_to("confetti", "sill://confetti", ())
        .map_err(|err| format!("could not start the confetti: {err}"))
}

/// The confetti window putting itself away once the last piece has fallen.
#[tauri::command]
pub(crate) async fn finish_confetti(app: AppHandle) -> Result<(), String> {
    crate::lazy_windows::hide(&app, "confetti");
    Ok(())
}

/// The cities the world clock shows, each with the name the widget's own
/// clock understands.
///
/// Asked once when the widget is drawn and again when the list changes, and
/// never on a tick: the ticking is the machine's own time formatted for a
/// zone the browser already knows, so a minute passing asks Rust nothing.
/// The zone table is read on the first ask and held for an hour.
#[tauri::command]
pub(crate) async fn world_clocks(
    app: AppHandle,
    zones: State<'_, crate::state::Fresh<std::sync::Arc<Vec<crate::zones::Zone>>>>,
) -> Result<Vec<crate::zones::Shown>, String> {
    let wanted = {
        let prefs = app.state::<crate::state::PrefsState>();
        let held = prefs.inner.lock().await;
        held.widgets.clocks.clone()
    };

    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let table = zones.get(|| std::sync::Arc::new(crate::zones::all()));

    Ok(crate::zones::shown(&wanted, &table, crate::zones::iana_of))
}

/// The current conditions where the user said.
///
/// Reads the place out of preferences rather than taking it as an argument, so
/// the window never has to hold a location and cannot ask about one that was
/// never set up.
#[tauri::command]
pub(crate) async fn weather_now(app: tauri::AppHandle) -> Result<crate::weather::Weather, String> {
    use tauri::Manager;

    // The same rule as the machine readout, and here it is a network call.
    if !crate::summon::anything_visible(&app) {
        return Err("Nothing is on screen to show the weather in.".to_string());
    }

    let (place, fahrenheit) = {
        let prefs = app.state::<crate::state::PrefsState>();
        let held = prefs.inner.lock().await;
        (held.widgets.place.clone(), held.widgets.fahrenheit)
    };

    if place.name.trim().is_empty() {
        return Err("No place is set. Choose one in Settings under Widgets.".to_string());
    }

    app.state::<crate::weather::Forecast>()
        .at(&place, fahrenheit)
        .await
}

/// Saves the current arrangement of windows under a name.
#[tauri::command]
pub(crate) fn save_workspace(app: tauri::AppHandle, name: String) -> Result<usize, String> {
    let name = name.trim();

    if name.is_empty() {
        return Err("Give the workspace a name.".to_string());
    }

    let works: Vec<crate::windowing::Rect> = crate::windowing::monitors()
        .into_iter()
        .map(|monitor| monitor.work)
        .collect();

    let profile = crate::profiles::capture(name, &crate::windowing::list(), &works);
    let count = profile.windows.len();

    if count == 0 {
        return Err("Nothing is open to arrange.".to_string());
    }

    let file = crate::profiles_store::path(&app);
    let all = crate::profiles_store::put(crate::profiles_store::load(&file), profile);

    crate::profiles_store::save(&file, &all)
        .map_err(|err| format!("could not save the workspace: {err}"))?;

    crate::reload_index(&app);
    Ok(count)
}

/// Rewrites a saved arrangement as named positions, where they fit.
///
/// Separate from saving because it is a decision, not a detail. A captured
/// arrangement is exactly where the windows were, which is right on the desk
/// it was captured on and approximate everywhere else. Converting it says
/// "what I meant was left half, right half", which is exact on any display
/// and no longer records the pixel widths somebody dragged to.
///
/// Answers how many windows became a named position, so the outcome can say
/// whether it did anything. A window that fits no slot keeps its rectangle,
/// so this never makes an arrangement worse, only more portable where it can
/// be.
#[tauri::command]
pub(crate) fn make_workspace_portable(
    app: tauri::AppHandle,
    name: String,
) -> Result<usize, String> {
    let file = crate::profiles_store::path(&app);
    let all = crate::profiles_store::load(&file);

    let profile = all
        .iter()
        .find(|one| one.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| format!("There is no workspace called \"{name}\"."))?;

    let works: Vec<crate::windowing::Rect> = crate::windowing::monitors()
        .into_iter()
        .map(|monitor| monitor.work)
        .collect();

    let portable = crate::profiles::to_slots(profile, &works);
    let named = portable
        .windows
        .iter()
        .filter(|placed| placed.slot.is_some())
        .count();

    let all = crate::profiles_store::put(all, portable);
    crate::profiles_store::save(&file, &all)
        .map_err(|err| format!("could not save the workspace: {err}"))?;

    Ok(named)
}

/// Puts a saved arrangement back.
#[tauri::command]
pub(crate) async fn restore_workspace(
    app: tauri::AppHandle,
    name: String,
) -> Result<usize, String> {
    let file = crate::profiles_store::path(&app);

    let profile = crate::profiles_store::load(&file)
        .into_iter()
        .find(|one| one.name.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(|| format!("There is no workspace called \"{name}\"."))?;

    /*
     * Whatever is closed is opened first, and then waited for.
     *
     * Restoring only the windows that happened to be open was most of an
     * arrangement: an arrangement is what was on the desk, and half of it is
     * usually shut. A window cannot be placed before it exists, so this starts
     * what is missing and waits, briefly and with a ceiling, for it to appear.
     *
     * Bounded and best effort on purpose. A program that is slow to start, or
     * that opens no window at all, must cost a couple of seconds and then let
     * the rest of the arrangement happen, rather than holding it up.
     */
    let wanted = crate::profiles::missing(&profile, &crate::windowing::list());

    // An arrangement is a saved file, and a saved file can be handed to
    // somebody. Restoring one must not be a way of running an address.
    for path in &wanted {
        let started = crate::reach::target(path).and_then(|path| {
            tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|err| err.to_string())
        });

        if let Err(err) = started {
            crate::say!("workspace: could not start {path}: {err}");
        }
    }

    if !wanted.is_empty() {
        wait_for_windows(&profile).await;
    }

    let works: Vec<crate::windowing::Rect> = crate::windowing::monitors()
        .into_iter()
        .map(|monitor| monitor.work)
        .collect();

    let moves = crate::profiles::plan(&profile, &crate::windowing::list(), &works);
    let mut moved = 0;

    for (id, rect, _) in &moves {
        // A window that has closed since the profile was made is skipped
        // rather than failing the rest: putting four of five windows back is
        // most of what was asked for.
        if crate::windowing::place(*id, *rect).is_ok() {
            moved += 1;
        }
    }

    Ok(moved)
}

/// Waits for the programs just started to put windows on screen.
///
/// Polled rather than subscribed to: there is no event for "a window somebody
/// might have meant appeared", and a poll that runs for at most a couple of
/// seconds after a deliberate action is not the kind of waking this codebase
/// refuses. It stops the moment every saved program has a window, so the
/// ceiling is only reached when something genuinely did not start.
async fn wait_for_windows(profile: &crate::profiles::Profile) {
    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(4);
    const EVERY: std::time::Duration = std::time::Duration::from_millis(250);

    let started = std::time::Instant::now();

    while started.elapsed() < PATIENCE {
        tokio::time::sleep(EVERY).await;

        if crate::profiles::missing(profile, &crate::windowing::list()).is_empty() {
            return;
        }
    }
}

/// Forgets one.
#[tauri::command]
pub(crate) fn forget_workspace(app: tauri::AppHandle, name: String) -> Result<(), String> {
    let file = crate::profiles_store::path(&app);

    let left: Vec<crate::profiles::Profile> = crate::profiles_store::load(&file)
        .into_iter()
        .filter(|one| !one.name.eq_ignore_ascii_case(name.trim()))
        .collect();

    crate::profiles_store::save(&file, &left)
        .map_err(|err| format!("could not save the workspaces: {err}"))?;

    crate::reload_index(&app);
    Ok(())
}

/// The rows whose subtitle is a measurement, and what it says now.
///
/// Answers with nothing when the launcher is not visible, which is the
/// window's signal to stop asking. See `crate::live` for why the refusal lives
/// there rather than in the timer.
#[tauri::command]
pub(crate) fn live_rows(app: tauri::AppHandle) -> Vec<crate::live::Live> {
    crate::live::rows(&app)
}
