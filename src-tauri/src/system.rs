//! The switches people summon a launcher to flip.
//!
//! Volume, mute, dark mode, the lock screen. Small things, reached for often,
//! and every one of them currently two or three clicks into a settings app.
//!
//! Deliberately not a settings surface of its own. These are commands in the
//! same list as everything else, so the way to turn the volume down is to type
//! what you want rather than to learn where Sill keeps it.

use serde::Serialize;

/// What the system is doing right now, for the rows that report it.
///
/// Read when the list is built rather than watched. None of it changes without
/// somebody doing something, and a launcher that polls the audio endpoint to
/// keep a row's subtitle fresh is exactly the idle cost rule 23 forbids.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// 0 to 100, or `None` when there is no audio endpoint to ask.
    pub volume: Option<u8>,
    pub muted: Option<bool>,
    pub dark: Option<bool>,
}

/// How far one step of the volume moves it.
///
/// A tenth, so five presses is half. Windows itself uses two percent per press
/// of a media key, which is right for a key you hold down and wrong for a
/// command somebody typed the name of.
pub const STEP: u8 = 10;

/// Where Windows keeps the light or dark choice.
///
/// Two values, and both matter: `AppsUseLightTheme` is what applications read,
/// and `SystemUsesLightTheme` is the taskbar and the start menu. Setting only
/// the first is the common mistake and leaves a machine half switched.
const THEME_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

// ---------------------------------------------------------------- reading

/// What the volume is now.
#[cfg(windows)]
pub fn volume() -> Option<u8> {
    with_endpoint(|volume| unsafe {
        let level = volume.GetMasterVolumeLevelScalar()?;
        Ok((level * 100.0).round() as u8)
    })
}

#[cfg(not(windows))]
pub fn volume() -> Option<u8> {
    None
}

/// Whether sound is muted.
#[cfg(windows)]
pub fn muted() -> Option<bool> {
    with_endpoint(|volume| unsafe { Ok(volume.GetMute()?.as_bool()) })
}

#[cfg(not(windows))]
pub fn muted() -> Option<bool> {
    None
}

/// Whether Windows is in dark mode.
#[cfg(windows)]
pub fn dark() -> Option<bool> {
    // Nothing set is light, which is the Windows default, and is what a
    // machine that has never been changed reports.
    Some(
        read_theme("AppsUseLightTheme")
            .map(|light| light == 0)
            .unwrap_or(false),
    )
}

#[cfg(not(windows))]
pub fn dark() -> Option<bool> {
    None
}

/// Everything at once, for building the rows.
pub fn state() -> State {
    State {
        volume: volume(),
        muted: muted(),
        dark: dark(),
    }
}

// ---------------------------------------------------------------- changing

/// Sets the volume, as a percentage.
///
/// Clamped rather than refused. A command that asks for 120 means "as loud as
/// it goes", and failing on it would be pedantry.
#[cfg(windows)]
pub fn set_volume(percent: u8) -> Result<u8, String> {
    let wanted = percent.min(100);

    with_endpoint(|volume| unsafe {
        volume.SetMasterVolumeLevelScalar(f32::from(wanted) / 100.0, std::ptr::null())?;
        Ok(wanted)
    })
    .ok_or_else(|| "There is no audio device to change.".to_string())
}

#[cfg(not(windows))]
pub fn set_volume(_percent: u8) -> Result<u8, String> {
    Err("Only Windows has this.".to_string())
}

/// Moves the volume by one step, up or down.
///
/// Saturating on purpose. At zero, "quieter" does nothing rather than wrapping
/// round to full volume, which is the kind of surprise that makes somebody
/// stop trusting a command.
pub fn nudge_volume(up: bool) -> Result<u8, String> {
    let now = volume().ok_or_else(|| "There is no audio device to change.".to_string())?;

    set_volume(stepped(now, up))
}

/// Where one step lands, given where it started.
///
/// Separate from the call that applies it, so the arithmetic can be checked
/// without an audio device and without changing anybody's volume. Saturating
/// at both ends: at zero, quieter does nothing rather than wrapping round to
/// full, which is one subtraction away and the kind of surprise that makes
/// somebody stop trusting a command.
pub fn stepped(now: u8, up: bool) -> u8 {
    if up {
        now.saturating_add(STEP).min(100)
    } else {
        now.saturating_sub(STEP)
    }
}

/// Mutes or unmutes, and says which it did.
#[cfg(windows)]
pub fn set_muted(on: bool) -> Result<bool, String> {
    with_endpoint(|volume| unsafe {
        volume.SetMute(on, std::ptr::null())?;
        Ok(on)
    })
    .ok_or_else(|| "There is no audio device to change.".to_string())
}

#[cfg(not(windows))]
pub fn set_muted(_on: bool) -> Result<bool, String> {
    Err("Only Windows has this.".to_string())
}

/// Switches Windows between light and dark.
///
/// Both values, because applications read one and the taskbar reads the other.
/// Setting only the first is the common mistake and leaves a machine looking
/// half switched, which reads as a bug in whatever did it.
#[cfg(windows)]
pub fn set_dark(on: bool) -> Result<bool, String> {
    let light: u32 = u32::from(!on);

    write_theme("AppsUseLightTheme", light)?;
    write_theme("SystemUsesLightTheme", light)?;

    Ok(on)
}

#[cfg(not(windows))]
pub fn set_dark(_on: bool) -> Result<bool, String> {
    Err("Only Windows has this.".to_string())
}

/// Locks the screen.
#[cfg(windows)]
pub fn lock() -> Result<(), String> {
    // SAFETY: takes nothing and returns whether it worked.
    unsafe { windows::Win32::System::Shutdown::LockWorkStation() }
        .map_err(|err| format!("could not lock the screen: {err}"))
}

#[cfg(not(windows))]
pub fn lock() -> Result<(), String> {
    Err("Only Windows has this.".to_string())
}

// ---------------------------------------------------------------- plumbing

/// Runs one call against the default output device.
///
/// COM is set up and torn down around each call rather than held. These happen
/// when somebody runs a command, which is rarely, and an apartment held open
/// for the life of the process to save a microsecond on a keypress is the
/// wrong trade.
#[cfg(windows)]
fn with_endpoint<T>(
    work: impl FnOnce(
        &windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume,
    ) -> windows::core::Result<T>,
) -> Option<T> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    // SAFETY: COM is initialised and uninitialised on the same thread around
    // the whole call, and every interface is released by its own Drop.
    unsafe {
        // An already initialised apartment answers with a failure code that is
        // not an error. Only the uninitialise has to match.
        let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        let result = (|| -> windows::core::Result<T> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            work(&volume)
        })();

        if initialised {
            CoUninitialize();
        }

        match result {
            Ok(value) => Some(value),
            Err(err) => {
                crate::say!("audio endpoint: {err}");
                None
            }
        }
    }
}

/// Whether this is a Windows client rather than a Windows Server.
///
/// The two ship different sets of control panel applets, so a check that a
/// system file is present answers a different question on each. `wscui.cpl`,
/// the Security and Maintenance applet, is the one that matters here: it is on
/// every client and on no server, so a catalog that names it is correct on the
/// machines Sill runs on and looks broken on a build agent.
///
/// `InstallationType` is the value Windows itself uses to say which it is. It
/// reads `Client` on 10 and 11 and `Server` on the server editions.
///
/// Only a test asks so far, so it is built only for tests rather than left
/// sitting as dead code in the shipped binary. Drop the `test` when something
/// in the product wants it.
#[cfg(all(windows, test))]
pub(crate) fn is_windows_client() -> bool {
    use windows::core::{h, HSTRING};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    // Room for "Client" or "Server" and the terminator, in UTF-16 bytes.
    let mut buffer = [0u8; 64];
    let mut size = buffer.len() as u32;

    // SAFETY: the buffer is owned here, its size is passed alongside it, and
    // both key names are literals that outlive the call.
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            h!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            h!("InstallationType"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };

    if code.is_err() {
        // A machine that will not say is treated as the ordinary case, because
        // refusing to check on an unreadable registry would turn one unknown
        // into a skipped test everywhere.
        return true;
    }

    let wide: Vec<u16> = buffer[..size as usize]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|unit| *unit != 0)
        .collect();

    HSTRING::from_wide(&wide)
        .to_string()
        .eq_ignore_ascii_case("client")
}

#[cfg(windows)]
fn read_theme(name: &str) -> Option<u32> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;

    // SAFETY: both buffers are owned and sized here, and the key name is a
    // literal that outlives the call.
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            PCWSTR(wide.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut std::ffi::c_void),
            Some(&mut size),
        )
    };

    code.is_ok().then_some(value)
}

#[cfg(windows)]
fn write_theme(name: &str, value: u32) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
        REG_DWORD,
    };

    let path: Vec<u16> = THEME_KEY.encode_utf16().chain(std::iter::once(0)).collect();
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut key = HKEY::default();

    // SAFETY: every buffer is owned here and outlives the calls, and the key
    // is closed on both paths out.
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
        .ok()
        .map_err(|err| format!("could not open the theme setting: {err}"))?;

        let written = RegSetValueExW(
            key,
            PCWSTR(wide.as_ptr()),
            None,
            REG_DWORD,
            Some(&value.to_le_bytes()),
        );

        let _ = RegCloseKey(key);

        written
            .ok()
            .map_err(|err| format!("could not change the theme: {err}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window asks about a row by its id, not by what the row runs.
    ///
    /// These are different strings and only one of them is a key. Asking with
    /// the other returns "not a switch", which is what an ordinary row also
    /// returns, so a whole list of switches quietly kept the state it had
    /// before it was pressed: no error, no log, nothing to notice.
    #[test]
    fn a_switch_is_found_by_the_id_the_window_knows_it_by() {
        let live = Live {
            muted: true,
            dark: false,
            ..Live::default()
        };

        let rows = [
            ("sill:system.mute", "system.mute"),
            ("sill:system.theme", "system.theme"),
            ("app:notepad", "C:/Windows/notepad.exe"),
        ];

        let asked = [
            "sill:system.mute".to_string(),
            "sill:system.theme".to_string(),
            // Not a switch, and not in the list either.
            "app:notepad".to_string(),
            "sill:nothing.like.this".to_string(),
        ];

        assert_eq!(
            states_for(rows.into_iter(), &asked, &live),
            vec![Some(true), Some(false), None, None],
        );
    }

    #[test]
    fn a_step_is_a_tenth() {
        // Five presses is half, which is what somebody typing "volume down"
        // repeatedly expects. Windows uses two percent for a media key, which
        // is right for a key held down and wrong for a named command.
        assert_eq!(STEP, 10);
        assert_eq!(100 / STEP, 10);
    }

    #[cfg(windows)]
    #[test]
    fn reading_the_volume_gives_a_percentage_or_nothing() {
        // A machine with no audio device is a real machine, and it must
        // answer "nothing" rather than zero, which is a volume.
        if let Some(level) = volume() {
            assert!(level <= 100, "volume came back as {level}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn dark_mode_is_always_answerable() {
        // Nothing set is light, which is the Windows default, so this has an
        // answer on a machine that has never been changed.
        assert!(dark().is_some());
    }

    #[test]
    fn a_step_moves_by_a_step() {
        assert_eq!(stepped(50, true), 50 + STEP);
        assert_eq!(stepped(50, false), 50 - STEP);
    }

    #[test]
    fn quieter_at_zero_does_nothing_rather_than_wrapping_round() {
        // Wrapping to full volume is one subtraction away, and it is the kind
        // of surprise that makes somebody stop trusting a command.
        assert_eq!(stepped(0, false), 0);
        assert_eq!(stepped(STEP - 1, false), 0);
    }

    #[test]
    fn louder_at_full_volume_stays_at_full_volume() {
        assert_eq!(stepped(100, true), 100);
        assert_eq!(stepped(101 - STEP, true), 100);
    }

    #[test]
    fn every_starting_point_lands_somewhere_sensible() {
        // No step from anywhere may leave the range, in either direction.
        for now in 0..=100u8 {
            for up in [true, false] {
                let landed = stepped(now, up);

                assert!(landed <= 100, "{now} went to {landed}");
                if up {
                    assert!(landed >= now, "up made it quieter");
                } else {
                    assert!(landed <= now, "down made it louder");
                }
            }
        }
    }

    /// The real endpoint, opt in.
    ///
    /// **Not run by default, on purpose.** These change the volume of whatever
    /// machine they run on. They put it back, but a panic between the two
    /// would leave somebody's speakers somewhere they did not choose, and a
    /// test suite that fiddles with the volume every time it runs is one
    /// people stop running.
    ///
    /// They are also one test rather than four, because the volume is a single
    /// system-wide thing and Rust runs tests in parallel: as four they set it
    /// to different values at the same time and read each other's answers.
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml --lib system -- --ignored
    /// ```
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn the_real_audio_endpoint_answers_and_changes() {
        let Some(before) = volume() else {
            return; // No audio device on this machine.
        };

        assert_eq!(set_volume(42).expect("set"), 42);
        assert_eq!(volume(), Some(42), "reading back did not give what was set");

        // "As loud as it goes" is what somebody means by an impossible number.
        assert_eq!(set_volume(200).expect("set"), 100);
        assert_eq!(volume(), Some(100));

        set_volume(50).expect("half");
        assert_eq!(nudge_volume(true).expect("up"), 50 + STEP);
        assert_eq!(nudge_volume(false).expect("down"), 50);

        let was_muted = muted().expect("mute is readable");
        set_muted(!was_muted).expect("toggled");
        assert_eq!(muted(), Some(!was_muted));
        set_muted(was_muted).expect("put back");

        set_volume(before).expect("put back");
        assert_eq!(volume(), Some(before), "the volume was left changed");
    }

    /// Dark mode, opt in for the same reason.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn switching_the_theme_sets_both_halves() {
        // Applications read one value and the taskbar reads the other. Setting
        // only the first leaves a machine looking half switched, which reads
        // as a bug in whatever did it.
        let before = dark().expect("readable");

        set_dark(!before).expect("switched");
        assert_eq!(dark(), Some(!before));
        assert_eq!(
            read_theme("AppsUseLightTheme"),
            read_theme("SystemUsesLightTheme"),
            "one half of the theme was left as it was"
        );

        set_dark(before).expect("put back");
        assert_eq!(dark(), Some(before));
    }
}

/// What the switches are set to right now.
///
/// Read together rather than one at a time, because a row that shows its state
/// needs all of them and the reads share the cost of opening COM once.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub muted: bool,
    pub dark: bool,
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    /// The endpoint id sound is going to.
    pub output: String,
}

/// How long a reading is trusted before another is taken.
///
/// A keystroke is not a reason to enumerate the radios: that is a WinRT call
/// and typing "bluetooth" is eight of them. Nothing here changes faster than a
/// person can notice, and pressing a switch refreshes it regardless, so a
/// second is generous.
const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(1);

static LAST: std::sync::Mutex<Option<(std::time::Instant, Live)>> = std::sync::Mutex::new(None);

/// The current state of every switch, read at most once a second.
///
/// Lazily: nothing calls this unless a system row is actually about to be
/// shown, so a search that matches no switch costs nothing at all.
pub fn live() -> Live {
    // Recovered rather than propagated: the lock is held across COM and
    // registry reads, and a poisoned cache must not panic every later search.
    let mut held = LAST.lock().unwrap_or_else(|e| e.into_inner());

    if let Some((taken, state)) = held.as_ref() {
        if taken.elapsed() < FRESH_FOR {
            return state.clone();
        }
    }

    let radios = crate::radios::radios();
    let state = Live {
        muted: muted().unwrap_or(false),
        dark: dark().unwrap_or(false),
        wifi: radios.iter().find(|r| r.kind == "wifi").map(|r| r.on),
        bluetooth: radios.iter().find(|r| r.kind == "bluetooth").map(|r| r.on),
        output: crate::audio::outputs()
            .into_iter()
            .find(|o| o.current)
            .map(|o| o.id)
            .unwrap_or_default(),
    };

    *held = Some((std::time::Instant::now(), state.clone()));
    state
}

/// Whether this row is a switch at all, by what it is rather than by its state.
///
/// Two different questions live close together here and mixing them up costs
/// an afternoon. **This one is about shape**: a volume nudge and the lock
/// screen are not switches whatever the machine is doing. [`toggle_state`] is
/// about the moment: it answers `None` for those, and also for a radio the
/// machine does not have, because there is no state to report.
///
/// Anything that needs to know whether a row draws a control, without a live
/// reading in hand, asks this.
pub fn is_switch(id: &str) -> bool {
    matches!(id, "system.mute" | "system.theme")
        || id.starts_with(crate::actions::RADIO)
        || id.starts_with(crate::actions::AUDIO_OUTPUT)
}

/// Which way a switch is set, given its id.
///
/// `None` for a system row that is not a switch. Volume up is a nudge and lock
/// is a door: neither has an on and an off, and drawing one as a control that
/// is currently "off" would be a lie about what pressing it does.
pub fn toggle_state(id: &str, live: &Live) -> Option<bool> {
    // Asked first, so the two answers cannot drift about what a switch is.
    if !is_switch(id) {
        return None;
    }

    match id {
        "system.mute" => Some(live.muted),
        "system.theme" => Some(live.dark),
        _ => {
            if let Some(kind) = id.strip_prefix(crate::actions::RADIO) {
                return match kind {
                    "wifi" => live.wifi,
                    "bluetooth" => live.bluetooth,
                    _ => None,
                };
            }

            // An output is one of a set rather than an on and an off, so it is
            // drawn as chosen or not: exactly one of them is true at a time.
            if let Some(device) = id.strip_prefix(crate::actions::AUDIO_OUTPUT) {
                return Some(device == live.output);
            }

            None
        }
    }
}

/// Where each of a set of switches is set, asked for by row id.
///
/// The window knows a row by its id, `sill:system.mute`. A switch is keyed by
/// its entrypoint, `system.mute`, and they are different strings. Asking
/// [`toggle_state`] with an id answers "not a switch", which is the same
/// answer an ordinary row gives, so nothing fails and no error is logged: the
/// row simply keeps showing the state it had before it was pressed. That is
/// the bug this function exists to make impossible, so the translation happens
/// once, here, rather than at each caller.
pub fn states_for<'a>(
    rows: impl Iterator<Item = (&'a str, &'a str)>,
    ids: &[String],
    live: &Live,
) -> Vec<Option<bool>> {
    let by_id: std::collections::HashMap<&str, &str> = rows.collect();

    ids.iter()
        .map(|id| {
            by_id
                .get(id.as_str())
                .and_then(|entrypoint| toggle_state(entrypoint, live))
        })
        .collect()
}

/// Throws the reading away, so the next one is taken fresh.
///
/// Called after a switch is pressed. Without it the row would show what it was
/// a moment ago for up to a second, which is exactly the moment somebody is
/// looking at it.
pub fn forget_live() {
    if let Ok(mut held) = LAST.lock() {
        *held = None;
    }
}
