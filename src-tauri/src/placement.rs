//! Which screen the launcher comes up on.
//!
//! ## Why this exists
//!
//! The window was centred once, at startup, by `window.center()`, and never
//! moved again. On one screen that is invisible and correct. On two it means
//! the launcher always appears on the primary monitor, however far from what
//! somebody is actually looking at, and after a display change it can sit
//! entirely off every screen with no way to bring it back.
//!
//! So it is placed on every summon rather than once, and placed by the work
//! area rather than the full bounds, because a window centred in the bounds of
//! a screen with a taskbar sits slightly low.
//!
//! ## Why an atomic
//!
//! `summon::show` is synchronous and is the hottest path in the application:
//! it runs between a key being pressed and a window being on screen.
//! Preferences live behind an async lock, so reading them here would mean
//! either blocking that path or making it async. The setting is one of three
//! values, so it is kept as one of three values.

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::WebviewWindow;

use crate::preferences::SummonOn;

/// Where the launcher should appear, readable from anywhere without waiting.
#[derive(Default)]
pub struct Placement(AtomicU8);

impl Placement {
    pub(crate) fn set(&self, on: SummonOn) {
        let code = match on {
            SummonOn::Cursor => 0,
            SummonOn::ActiveWindow => 1,
            SummonOn::Primary => 2,
        };

        self.0.store(code, Ordering::Relaxed);
    }

    pub(crate) fn get(&self) -> SummonOn {
        match self.0.load(Ordering::Relaxed) {
            1 => SummonOn::ActiveWindow,
            2 => SummonOn::Primary,
            // Zero is both the default of an `AtomicU8` and the default
            // preference, so a `Placement` nobody set behaves as configured
            // rather than as something nobody chose.
            _ => SummonOn::Cursor,
        }
    }
}

/// A screen's usable rectangle, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Area {
    fn width(self) -> i32 {
        self.right - self.left
    }

    fn height(self) -> i32 {
        self.bottom - self.top
    }
}

/// Where a window of this size goes to sit centred on that screen.
///
/// Its own function so the arithmetic can be tested without a screen. The
/// clamp is the part worth testing: a window taller than the work area, which
/// happens with a tall row count on a short screen, must hang off the bottom
/// rather than off the top, because the top is where the search field is and a
/// launcher whose field is above the screen cannot be used at all.
pub fn centred_in(area: Area, width: i32, height: i32) -> (i32, i32) {
    let x = area.left + (area.width() - width) / 2;
    let y = area.top + (area.height() - height) / 2;

    (x.max(area.left), y.max(area.top))
}

#[cfg(windows)]
mod win {
    use super::Area;
    use crate::preferences::SummonOn;

    use windows::Win32::Foundation::{HWND, POINT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow, HMONITOR, MONITORINFO,
        MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    /// The screen the setting points at, or nothing if Windows will not say.
    pub(super) fn area(on: SummonOn) -> Option<Area> {
        let monitor = screen(on)?;

        let mut info = MONITORINFO {
            cbSize: u32::try_from(size_of::<MONITORINFO>()).ok()?,
            ..Default::default()
        };

        // SAFETY: the handle came from Windows, and `info` is a local with its
        // size field set, which is the whole of what this call requires.
        if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
            return None;
        }

        Some(Area {
            left: info.rcWork.left,
            top: info.rcWork.top,
            right: info.rcWork.right,
            bottom: info.rcWork.bottom,
        })
    }

    fn screen(on: SummonOn) -> Option<HMONITOR> {
        match on {
            SummonOn::Cursor => {
                let mut point = POINT::default();
                // SAFETY: `point` is a local the call only writes into.
                unsafe { GetCursorPos(&mut point) }.ok()?;

                // NEAREST rather than NULL: the pointer can be in the gap
                // between two screens of different heights, and a launcher
                // that declines to appear because of that is worse than one
                // that appears on the closer screen.
                // SAFETY: takes a point by value and returns a handle.
                Some(unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) })
            }
            SummonOn::ActiveWindow => {
                // Remembered by `show` before it takes focus, which is the
                // only moment the answer still exists.
                let hwnd = crate::summon::previous_foreground()?;

                // SAFETY: the handle was the foreground window a moment ago.
                // If it has since closed this returns the nearest monitor
                // rather than failing.
                Some(unsafe {
                    MonitorFromWindow(
                        HWND(hwnd as *mut core::ffi::c_void),
                        MONITOR_DEFAULTTONEAREST,
                    )
                })
            }
            // The origin is on the primary screen by definition, and
            // TOPRIMARY makes that explicit rather than incidental.
            // SAFETY: takes a point by value and returns a handle.
            SummonOn::Primary => {
                Some(unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) })
            }
        }
    }
}

/// Puts the launcher on the screen the preference names, centred in its work
/// area.
///
/// Called on every summon rather than once, which is also what fixes a window
/// stranded off-screen by a display change: the next summon brings it back
/// whatever happened to the desktop in between.
pub fn centre_for_summon(window: &WebviewWindow, on: SummonOn) {
    #[cfg(windows)]
    {
        let Some(area) = win::area(on) else {
            let _ = window.center();
            return;
        };

        /*
         * Twice, because moving between screens can change the size.
         *
         * Two monitors can have different scale factors, and Windows resizes a
         * window that crosses between them so it keeps its logical size. The
         * first move is what triggers that, so the size read before it is the
         * size on the old screen. Placing again with the size it ended up with
         * is what keeps it centred rather than nearly centred.
         */
        for _ in 0..2 {
            let Ok(size) = window.outer_size() else {
                return;
            };

            let (x, y) = centred_in(
                area,
                i32::try_from(size.width).unwrap_or(i32::MAX),
                i32::try_from(size.height).unwrap_or(i32::MAX),
            );

            if window
                .set_position(tauri::PhysicalPosition::new(x, y))
                .is_err()
            {
                return;
            }
        }
    }

    #[cfg(not(windows))]
    {
        let _ = on;
        let _ = window.center();
    }
}

#[cfg(test)]
mod tests {
    use super::{centred_in, Area, Placement};
    use crate::preferences::SummonOn;

    /// The second monitor here is portrait, to the left, at negative
    /// coordinates. Arithmetic that assumed a screen starts at zero would
    /// place the window on the primary one and look like it worked.
    #[test]
    fn a_screen_at_negative_coordinates_gets_the_window() {
        let portrait = Area {
            left: -1080,
            top: -801,
            right: 0,
            bottom: 1071,
        };

        let (x, y) = centred_in(portrait, 750, 500);

        assert_eq!(x, -915, "the window is not centred on the left-hand screen");
        assert!(
            x >= portrait.left && x + 750 <= portrait.right,
            "the window hangs off the screen it was placed on"
        );
        // A 1872-tall work area minus a 500-tall window, halved, from the top.
        assert_eq!(y, -115);
    }

    #[test]
    fn the_work_area_is_used_rather_than_the_full_screen() {
        // 2560x1440 with a 48px taskbar, which is this machine.
        let with_taskbar = Area {
            left: 0,
            top: 0,
            right: 2560,
            bottom: 1392,
        };

        let (_, y) = centred_in(with_taskbar, 750, 500);

        assert_eq!(y, 446, "centred in the bounds rather than the usable area");
    }

    /// A window taller than the screen hangs off the bottom, never the top.
    ///
    /// The top is where the search field is. A launcher whose field is above
    /// the screen cannot be typed into, which is worse than one whose last
    /// rows are cut off.
    #[test]
    fn a_window_too_tall_for_the_screen_keeps_its_field_on_it() {
        let short = Area {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 700,
        };

        let (_, y) = centred_in(short, 750, 900);

        assert_eq!(y, 0, "the top of the launcher is off the top of the screen");
    }

    /// The default of the atomic and the default of the preference agree.
    #[test]
    fn a_placement_nobody_set_is_the_one_the_preferences_default_to() {
        assert_eq!(Placement::default().get(), SummonOn::Cursor);
    }

    #[test]
    fn every_choice_survives_being_stored() {
        for on in [SummonOn::Cursor, SummonOn::ActiveWindow, SummonOn::Primary] {
            let placement = Placement::default();
            placement.set(on);
            assert_eq!(placement.get(), on, "{on:?} did not come back");
        }
    }
}
