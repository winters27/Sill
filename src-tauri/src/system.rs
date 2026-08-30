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
const THEME_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

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
    Some(read_theme("AppsUseLightTheme").map(|light| light == 0).unwrap_or(false))
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

#[cfg(windows)]
fn read_theme(name: &str) -> Option<u32> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD,
    };

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
