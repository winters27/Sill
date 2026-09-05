//! Display resolution and refresh rate, changed from the list.
//!
//! `resolution` lists the modes the display in front can be set to, `display
//! 2 resolution` the second display's, and Enter sets one. The undo puts the
//! previous mode back, and so does a fifteen second wait with no answer,
//! which is the one safety a launcher has to add here: a mode the driver
//! accepts and the monitor does not is a black screen with an undo nobody
//! can see.
//!
//! Every mode offered came out of `EnumDisplaySettingsEx` for that device,
//! and the `DEVMODE` that is applied is the one it handed over, which is what
//! the documentation asks for. Only 32-bit modes are listed, because that is
//! all a desktop application built for Windows 8 or later may set.
//!
//! Enumerated when asked, never at rest: the gate is one word.

/// One mode a display can be in.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mode {
    /// The device name Windows uses, `\\.\DISPLAY1`.
    pub device: String,
    /// Which display this is, counting from one, as the list says it.
    pub display: usize,
    pub width: u32,
    pub height: u32,
    pub hz: u32,
    /// Whether the display is in this mode right now.
    pub current: bool,
}

/// How long a changed mode stands before it is put back unless somebody says
/// to keep it. Windows Settings waits the same.
pub const KEEP_WITHIN: std::time::Duration = std::time::Duration::from_secs(15);

/// How many modes one query shows.
const MOST_ROWS: usize = 24;

/// Only these words, and only first.
const ASKED_BY: &[&str] = &["resolution", "refresh", "hz", "display", "displays"];

/// What the query asks for: which display, and a filter on the mode.
///
/// `resolution` is the display in front, `display 2 resolution` or `display 2`
/// is the second, and any number after the words narrows to modes mentioning
/// it: `resolution 144` is the 144 Hz ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asked {
    /// Counting from one. `None` is whichever display the launcher is on.
    pub display: Option<usize>,
    pub filter: String,
}

pub fn asked(query: &str) -> Option<Asked> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let first = words.first()?.to_ascii_lowercase();

    if !ASKED_BY.contains(&first.as_str()) {
        return None;
    }

    let mut rest = &words[1..];
    let mut display = None;

    // `display 2 ...`: the number right after the word names the display.
    if first.starts_with("display") {
        if let Some(number) = rest.first().and_then(|word| word.parse::<usize>().ok()) {
            if number >= 1 {
                display = Some(number);
                rest = &rest[1..];
            }
        }
        // `display 2` on its own still means the resolutions; `display` alone
        // means the display in front, filtered by whatever follows.
        if let Some(word) = rest.first() {
            if ASKED_BY.contains(&word.to_ascii_lowercase().as_str()) {
                rest = &rest[1..];
            }
        }
    }

    Some(Asked {
        display,
        filter: rest.join(" ").to_ascii_lowercase(),
    })
}

/// `2560 x 1440, 144 Hz`.
pub fn said(mode: &Mode) -> String {
    format!("{} x {}, {} Hz", mode.width, mode.height, mode.hz)
}

/// The modes worth offering, from whatever the driver listed.
///
/// One row per distinct size and rate, the widest first and the fastest
/// rate first within a size, and the current one marked. Pure over the
/// triples so it can be checked without a display.
pub fn tidy(device: &str, display: usize, raw: Vec<(u32, u32, u32)>, current: Option<(u32, u32, u32)>) -> Vec<Mode> {
    let mut modes: Vec<(u32, u32, u32)> = raw;
    modes.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    modes.dedup();

    modes
        .into_iter()
        .map(|(width, height, hz)| Mode {
            device: device.to_string(),
            display,
            width,
            height,
            hz,
            current: current == Some((width, height, hz)),
        })
        .collect()
}

/// What a row carries so an action can find the mode again.
///
/// The device and the numbers, joined with a character no device name
/// holds. Read back by [`mode_from`], and the pair is round-tripped by a
/// test so a row cannot be written one way and read another.
pub fn target_of(mode: &Mode) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        mode.device, mode.display, mode.width, mode.height, mode.hz
    )
}

/// The mode a row's target names.
pub fn mode_from(target: &str) -> Result<Mode, String> {
    let parts: Vec<&str> = target.split('|').collect();
    let [device, display, width, height, hz] = parts.as_slice() else {
        return Err("that row does not name a display mode".to_string());
    };

    let number = |text: &str| -> Result<u32, String> {
        text.parse()
            .map_err(|_| "that row does not name a display mode".to_string())
    };

    Ok(Mode {
        device: device.to_string(),
        display: display.parse().map_err(|_| "that row does not name a display".to_string())?,
        width: number(width)?,
        height: number(height)?,
        hz: number(hz)?,
        current: false,
    })
}

/// The modes the query asks about, filtered by any number it mentioned.
///
/// A number matches a whole number: `144` is the 144 Hz modes and not every
/// 1440-high one. A word matches anywhere, so `hz` and `x` mean nothing
/// and cost nothing.
pub fn matched(asked: &Asked, modes: Vec<Mode>) -> Vec<Mode> {
    let words: Vec<&str> = asked.filter.split_whitespace().collect();

    modes
        .into_iter()
        .filter(|mode| {
            let numbers = [mode.width, mode.height, mode.hz];
            let text = said(mode).to_ascii_lowercase();

            words.iter().all(|word| match word.parse::<u32>() {
                Ok(number) => numbers.contains(&number),
                Err(_) => text.contains(word),
            })
        })
        .take(MOST_ROWS)
        .collect()
}

/// The modes a display can be set to, current one marked.
#[cfg(windows)]
pub fn modes(device: &str, display: usize) -> Vec<Mode> {
    let listed: Vec<(u32, u32, u32)> = each_devmode(device)
        .into_iter()
        .filter(|mode| mode.dmBitsPerPel == 32)
        .map(|mode| (mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency))
        .collect();

    tidy(device, display, listed, current_of(device))
}

#[cfg(not(windows))]
pub fn modes(_device: &str, _display: usize) -> Vec<Mode> {
    Vec::new()
}

/// Sets a display's mode, and answers with the mode it was in.
///
/// Tested with `CDS_TEST` before it is applied, applied with the registry
/// updated so it survives a sign-out, and a mode the driver says needs a
/// restart is reported as exactly that rather than as done.
#[cfg(windows)]
pub fn set(device: &str, wanted: &Mode) -> Result<Mode, String> {
    use windows::core::HSTRING;
    use windows::Win32::Graphics::Gdi::{
        ChangeDisplaySettingsExW, CDS_TEST, CDS_UPDATEREGISTRY, DISP_CHANGE_BADMODE,
        DISP_CHANGE_RESTART, DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFREQUENCY,
        DM_PELSHEIGHT, DM_PELSWIDTH,
    };

    let was = current_of(device).ok_or_else(|| "that display did not say what mode it is in".to_string())?;

    // The driver's own description of the mode, never a hand-built one.
    let mut devmode = each_devmode(device)
        .into_iter()
        .find(|mode| {
            mode.dmBitsPerPel == 32
                && mode.dmPelsWidth == wanted.width
                && mode.dmPelsHeight == wanted.height
                && mode.dmDisplayFrequency == wanted.hz
        })
        .ok_or_else(|| format!("{} is not a mode that display offers", said(wanted)))?;
    devmode.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL;

    let name = HSTRING::from(device);

    // SAFETY: the name and the mode outlive both calls, and `hwnd` is
    // documented as reserved and null.
    let tested = unsafe { ChangeDisplaySettingsExW(&name, Some(&devmode), None, CDS_TEST, None) };
    if tested != DISP_CHANGE_SUCCESSFUL {
        return Err(if tested == DISP_CHANGE_BADMODE {
            format!("The display refused {}", said(wanted))
        } else {
            format!("Windows would not set {} ({})", said(wanted), tested.0)
        });
    }

    let applied =
        unsafe { ChangeDisplaySettingsExW(&name, Some(&devmode), None, CDS_UPDATEREGISTRY, None) };

    match applied {
        DISP_CHANGE_SUCCESSFUL => Ok(Mode {
            device: device.to_string(),
            display: wanted.display,
            width: was.0,
            height: was.1,
            hz: was.2,
            current: false,
        }),
        DISP_CHANGE_RESTART => Err(format!(
            "{} takes effect after a restart, so it was not changed now",
            said(wanted)
        )),
        other => Err(format!("Could not set {} ({})", said(wanted), other.0)),
    }
}

#[cfg(not(windows))]
pub fn set(_device: &str, _wanted: &Mode) -> Result<Mode, String> {
    Err("only on Windows".to_string())
}

/// The display a monitor index refers to, counting from one, as Windows
/// names it for the display settings calls.
///
/// `EnumDisplayDevices` numbers adapters in the same order `EnumDisplayMonitors`
/// lists monitors on ordinary desks, and the attached ones are the ones with
/// a mode to set.
#[cfg(windows)]
pub fn devices() -> Vec<(usize, String)> {
    use windows::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};

    use windows::Win32::Graphics::Gdi::DISPLAY_DEVICE_ATTACHED_TO_DESKTOP;

    let mut out = Vec::new();
    let mut index = 0u32;

    loop {
        let mut device = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };

        // SAFETY: the struct declares its own size, and a null name asks for
        // the adapters in order.
        let found = unsafe { EnumDisplayDevicesW(None, index, &mut device, 0) };
        if !found.as_bool() {
            break;
        }
        index += 1;

        if device.StateFlags.0 & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP.0 == 0 {
            continue;
        }

        let end = device
            .DeviceName
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(device.DeviceName.len());
        out.push((out.len() + 1, String::from_utf16_lossy(&device.DeviceName[..end])));
    }

    out
}

#[cfg(not(windows))]
pub fn devices() -> Vec<(usize, String)> {
    Vec::new()
}

#[cfg(windows)]
fn each_devmode(device: &str) -> Vec<windows::Win32::Graphics::Gdi::DEVMODEW> {
    use windows::core::HSTRING;
    use windows::Win32::Graphics::Gdi::{
        EnumDisplaySettingsExW, DEVMODEW, ENUM_DISPLAY_SETTINGS_FLAGS, ENUM_DISPLAY_SETTINGS_MODE,
    };

    let name = HSTRING::from(device);
    let mut out = Vec::new();
    let mut index = 0u32;

    loop {
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };

        // SAFETY: the struct declares its own size and the name outlives
        // the call.
        let found = unsafe {
            EnumDisplaySettingsExW(
                &name,
                ENUM_DISPLAY_SETTINGS_MODE(index),
                &mut mode,
                ENUM_DISPLAY_SETTINGS_FLAGS(0),
            )
        };
        if !found.as_bool() {
            break;
        }

        out.push(mode);
        index += 1;

        // A driver listing thousands of modes is a driver with a problem this
        // list is not going to solve.
        if index > 4_096 {
            break;
        }
    }

    out
}

#[cfg(windows)]
fn current_of(device: &str) -> Option<(u32, u32, u32)> {
    use windows::core::HSTRING;
    use windows::Win32::Graphics::Gdi::{
        EnumDisplaySettingsExW, DEVMODEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_FLAGS,
    };

    let name = HSTRING::from(device);
    let mut mode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };

    // SAFETY: as above.
    let found = unsafe {
        EnumDisplaySettingsExW(
            &name,
            ENUM_CURRENT_SETTINGS,
            &mut mode,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    };

    found
        .as_bool()
        .then_some((mode.dmPelsWidth, mode.dmPelsHeight, mode.dmDisplayFrequency))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_is_the_gate() {
        assert_eq!(
            asked("resolution"),
            Some(Asked {
                display: None,
                filter: String::new()
            })
        );
        assert_eq!(
            asked("display 2"),
            Some(Asked {
                display: Some(2),
                filter: String::new()
            })
        );
        assert_eq!(
            asked("Display 2 resolution 144"),
            Some(Asked {
                display: Some(2),
                filter: "144".into()
            })
        );
        assert_eq!(
            asked("hz 60"),
            Some(Asked {
                display: None,
                filter: "60".into()
            })
        );

        for not in ["", "resolutions of conflict", "displayport", "notepad", "display 0"] {
            let answer = asked(not);
            assert!(
                answer.is_none() || not == "display 0" && answer.unwrap().display.is_none(),
                "{not:?} asked for a display mode"
            );
        }
    }

    #[test]
    fn modes_are_deduplicated_and_ordered() {
        let raw = vec![
            (1920, 1080, 60),
            (2560, 1440, 60),
            (2560, 1440, 144),
            (1920, 1080, 60),
            (1280, 720, 60),
        ];

        let modes = tidy(r"\\.\DISPLAY1", 1, raw, Some((2560, 1440, 144)));
        let seen: Vec<String> = modes.iter().map(said).collect();

        assert_eq!(
            seen,
            vec![
                "2560 x 1440, 144 Hz",
                "2560 x 1440, 60 Hz",
                "1920 x 1080, 60 Hz",
                "1280 x 720, 60 Hz",
            ]
        );
        assert!(modes[0].current);
        assert!(!modes[1].current);
    }

    #[test]
    fn a_number_in_the_query_narrows_the_modes() {
        let modes = tidy(
            r"\\.\DISPLAY1",
            1,
            vec![(2560, 1440, 144), (2560, 1440, 60), (1920, 1080, 60)],
            None,
        );

        let asked = asked("resolution 144").unwrap();
        let found = matched(&asked, modes.clone());
        assert_eq!(found.len(), 1);
        assert_eq!(said(&found[0]), "2560 x 1440, 144 Hz");

        let asked = asked_all();
        assert_eq!(matched(&asked, modes).len(), 3);
    }

    fn asked_all() -> Asked {
        Asked {
            display: None,
            filter: String::new(),
        }
    }

    #[test]
    fn a_rows_target_reads_back_as_the_mode_it_was() {
        let mode = Mode {
            device: r"\\.\DISPLAY2".into(),
            display: 2,
            width: 2560,
            height: 1440,
            hz: 144,
            current: true,
        };

        let back = mode_from(&target_of(&mode)).unwrap();
        assert_eq!((back.device.as_str(), back.display), (r"\\.\DISPLAY2", 2));
        assert_eq!((back.width, back.height, back.hz), (2560, 1440, 144));

        assert!(mode_from("not a mode").is_err());
        assert!(mode_from(r"\\.\DISPLAY1|1|wide|1440|60").is_err());
    }
}
