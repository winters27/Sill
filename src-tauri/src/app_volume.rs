//! How loud each program is, on its own.
//!
//! Windows has kept a separate volume per program since Vista and the only way
//! to reach it is the volume mixer, which is several clicks deep. Turning one
//! noisy tab down without turning the music down is a thing people want and
//! nobody has a shortcut for.
//!
//! ## What identifies a session
//!
//! Not the process id, which is different every time the program starts, and
//! not the name, which several programs share. Windows gives each session an
//! instance identifier that stays put for the life of that session, and that
//! is what a row carries so the action can find its way back to the right one.
//!
//! ## Why this is not in the index
//!
//! The index is built once at startup. Sessions come and go every time
//! something starts or stops playing, so an indexed row would be a list of
//! what happened to be making noise when Sill launched. They are enumerated
//! when they are asked for instead.

use serde::Serialize;

/// One program's own volume.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Windows' identifier for this session, which is what finds it again.
    pub id: String,
    /// What to call it in a row.
    pub name: String,
    /// Where its own slider sits, nought to one.
    pub volume: f32,
    pub muted: bool,
    /// The program this belongs to, which is where its icon comes from.
    pub path: String,
}

#[cfg(windows)]
mod platform {
    // Windows' own method names, which is the point: they have to line up with
    // something somebody else defined.
    #![allow(non_snake_case)]

    use super::Session;
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, S_OK};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AudioSessionStateExpired, IAudioSessionControl2, IAudioSessionManager2,
        IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    fn with_com<T>(work: impl FnOnce() -> windows::core::Result<T>) -> Result<T, String> {
        // SAFETY: initialised and uninitialised on the same thread around the
        // whole call, and every interface is released by its own Drop.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = work();

            if initialised {
                CoUninitialize();
            }

            result.map_err(|err| format!("the sound system refused: {err}"))
        }
    }

    /// The full path of a running process, or nothing if it has gone.
    ///
    /// The limited query right rather than the full one, because the limited
    /// one is granted across integrity levels and this only ever reads a path.
    fn path_of(pid: u32) -> Option<String> {
        // SAFETY: the handle is closed on every way out, and the buffer is
        // sized before the call and read only as far as the length reported.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;

            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
            .is_ok();

            let _ = CloseHandle(handle);

            ok.then(|| String::from_utf16_lossy(&buffer[..size as usize]))
        }
    }

    /// Reads a wide string the API allocated, and frees it.
    ///
    /// # Safety
    ///
    /// `text` must be a pointer the COM allocator owns, which is what every
    /// one of these getters returns.
    unsafe fn take(text: PWSTR) -> String {
        if text.is_null() {
            return String::new();
        }

        let out = text.to_string().unwrap_or_default();
        CoTaskMemFree(Some(text.0.cast()));
        out
    }

    /// Runs some work against every audio session on the current output.
    ///
    /// The enumeration is the expensive half and both listing and changing
    /// need it, so it is written once and the caller says what to do with each
    /// session it finds.
    fn each_session<T>(
        mut visit: impl FnMut(&IAudioSessionControl2, &ISimpleAudioVolume) -> Option<T>,
    ) -> Result<Vec<T>, String> {
        with_com(|| {
            // SAFETY: every pointer comes from the call above it and every
            // interface releases on Drop.
            unsafe {
                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
                let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                let sessions = manager.GetSessionEnumerator()?;

                let count = sessions.GetCount()?;
                let mut found = Vec::new();

                for at in 0..count {
                    let Ok(control) = sessions.GetSession(at) else {
                        continue;
                    };
                    let Ok(control) = control.cast::<IAudioSessionControl2>() else {
                        continue;
                    };

                    // A session whose program has exited still answers, with
                    // numbers that no longer mean anything. Drawing it offers
                    // a control over nothing.
                    if control.GetState() == Ok(AudioSessionStateExpired) {
                        continue;
                    }

                    let Ok(volume) = control.cast::<ISimpleAudioVolume>() else {
                        continue;
                    };

                    if let Some(one) = visit(&control, &volume) {
                        found.push(one);
                    }
                }

                Ok(found)
            }
        })
    }

    /// Every program that has a volume of its own right now.
    pub fn sessions() -> Vec<Session> {
        each_session(|control, volume| {
            // SAFETY: both interfaces are live for the length of this call,
            // and every string the getters hand over is freed by `take`.
            unsafe {
                let id = take(control.GetSessionInstanceIdentifier().ok()?);
                if id.is_empty() {
                    return None;
                }

                /*
                 * The system sounds session, which has no program of its own.
                 *
                 * Compared against `S_OK` rather than asked whether it
                 * succeeded. This returns a bare `HRESULT`, and it answers
                 * `S_OK` for yes and **`S_FALSE` for no**: both are successes,
                 * so `is_ok()` is true for every session there is. It was, and
                 * every program on the machine was labelled "System Sounds".
                 */
                let system = control.IsSystemSoundsSession() == S_OK;

                let path = if system {
                    String::new()
                } else {
                    path_of(control.GetProcessId().unwrap_or(0))?
                };

                let declared = take(control.GetDisplayName().unwrap_or_default());

                Some(Session {
                    name: if system {
                        "System Sounds".to_string()
                    } else {
                        super::name_for(&declared, &path)
                    },
                    volume: volume.GetMasterVolume().unwrap_or(1.0),
                    muted: volume.GetMute().map(|muted| muted.as_bool()).unwrap_or(false),
                    id,
                    path,
                })
            }
        })
        .unwrap_or_default()
    }

    /// Changes one session, found by its identifier.
    ///
    /// The change arrives as a closure so muting and setting a level share one
    /// enumeration and one place that decides a session has gone.
    fn change(
        id: &str,
        what: impl Fn(&ISimpleAudioVolume) -> windows::core::Result<()>,
    ) -> Result<(), String> {
        let touched = each_session(|control, volume| {
            // SAFETY: as in `sessions`.
            unsafe {
                let found = take(control.GetSessionInstanceIdentifier().ok()?);

                (found == id).then(|| what(volume).is_ok())
            }
        })?;

        match touched.first() {
            Some(true) => Ok(()),
            Some(false) => Err("the sound system refused that change".to_string()),
            // Not an error worth dressing up. The program stopped playing
            // between the row being drawn and the key being pressed.
            None => Err("that program is not playing anything any more".to_string()),
        }
    }

    pub fn set_muted(id: &str, muted: bool) -> Result<(), String> {
        // SAFETY: the interface is live for the length of the call. The null
        // context means "no particular event source", which is what a change
        // made from outside a mixer window is.
        change(id, |volume| unsafe {
            volume.SetMute(muted, std::ptr::null())
        })
    }

    pub fn set_volume(id: &str, level: f32) -> Result<(), String> {
        let level = level.clamp(0.0, 1.0);

        // SAFETY: as above.
        change(id, move |volume| unsafe {
            volume.SetMasterVolume(level, std::ptr::null())
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Session;

    pub fn sessions() -> Vec<Session> {
        Vec::new()
    }

    pub fn set_muted(_id: &str, _muted: bool) -> Result<(), String> {
        Err("a program's own volume needs Windows".to_string())
    }

    pub fn set_volume(_id: &str, _level: f32) -> Result<(), String> {
        Err("a program's own volume needs Windows".to_string())
    }
}

pub use platform::{set_muted, set_volume};

/*
 * The list, held for a moment.
 *
 * Enumerating costs about three milliseconds, which is nothing once and too
 * much on every keystroke. The list is asked for while somebody types a name
 * to filter it, so it is held for a second: typing eight letters enumerates
 * once rather than eight times, and a program that starts playing shows up on
 * the next keystroke after that.
 *
 * The same shape as the switch reading in `system`, and for the same reason.
 */
const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(1);

static LAST: std::sync::Mutex<Option<(std::time::Instant, Vec<Session>)>> =
    std::sync::Mutex::new(None);

/// Every program that has a volume of its own, read at most once a second.
pub fn sessions() -> Vec<Session> {
    // Recovered rather than propagated, for the reason `system::live` gives:
    // the lock spans COM calls and this runs on the search path.
    let mut held = LAST.lock().unwrap_or_else(|e| e.into_inner());

    if let Some((taken, list)) = held.as_ref() {
        if taken.elapsed() < FRESH_FOR {
            return list.clone();
        }
    }

    let list = platform::sessions();
    *held = Some((std::time::Instant::now(), list.clone()));
    list
}

/// Throws the list away, so the next reading is taken fresh.
///
/// Called after something has been changed. Without it a row would show what
/// it was a moment ago for up to a second, which is exactly the moment
/// somebody is looking at the row they just pressed.
pub fn forget_sessions() {
    if let Ok(mut held) = LAST.lock() {
        *held = None;
    }
}

/// What to call a session in a row.
///
/// A program may declare a display name and almost none of them do: the field
/// is usually empty, and when it is filled it is often a resource reference
/// like `@%SystemRoot%\System32\AudioSrv.Dll,-202` rather than anything a
/// person would read. So the program's own filename is the reliable answer and
/// the declared name is used only when it is a name.
pub fn name_for(declared: &str, path: &str) -> String {
    let declared = declared.trim();

    if !declared.is_empty() && !declared.starts_with('@') {
        return declared.to_string();
    }

    let stem = std::path::Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();

    if stem.is_empty() {
        return "Unknown".to_string();
    }

    // Only the first letter. "chrome" reads as a word rather than a program
    // without it, and "iTunes" and "obs64" are not improved by title casing.
    let mut letters = stem.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + letters.as_str(),
        None => stem,
    }
}

#[cfg(test)]
mod tests {
    use super::name_for;

    #[test]
    fn a_declared_name_is_used_when_it_is_a_name() {
        assert_eq!(name_for("Spotify", r"C:\x\spotify.exe"), "Spotify");
    }

    /// The field is a resource reference more often than a name.
    #[test]
    fn a_resource_reference_is_not_a_name() {
        assert_eq!(
            name_for(r"@%SystemRoot%\System32\AudioSrv.Dll,-202", r"C:\x\chrome.exe"),
            "Chrome",
        );
    }

    #[test]
    fn an_empty_declaration_falls_back_to_the_program() {
        assert_eq!(name_for("", r"C:\Program Files\Firefox\firefox.exe"), "Firefox");
        assert_eq!(name_for("   ", r"C:\x\obs64.exe"), "Obs64");
    }

    /// Only the first letter, so a name that is already cased keeps its shape.
    #[test]
    fn casing_past_the_first_letter_is_left_alone() {
        assert_eq!(name_for("", r"C:\x\iTunes.exe"), "ITunes");
        assert_eq!(name_for("", r"C:\x\VLC.exe"), "VLC");
    }

    #[test]
    fn nothing_at_all_still_gives_a_row_a_name() {
        assert_eq!(name_for("", ""), "Unknown");
    }
}
