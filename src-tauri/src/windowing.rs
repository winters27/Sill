//! The windows on the desktop, as things Sill can find and act on.
//!
//! Nothing here runs unless somebody asks. There is no watcher, no timer and
//! no cached list: enumerating every top-level window is a few hundred
//! microseconds of synchronous Win32, and a launcher only needs the answer
//! when a query is being typed. Keeping a live model of the desktop up to date
//! would mean a shell hook and a stream of events for a question nobody is
//! asking most of the time, which rule 23 rules out.
//!
//! The layout arithmetic is separate from the Win32 calls on purpose. Where a
//! window should go is the part with judgement in it and the part that can be
//! wrong; it is pure functions over rectangles, tested without a desktop.

use serde::{Deserialize, Serialize};

/// A rectangle in virtual-screen coordinates.
///
/// Virtual-screen, so it is signed: a monitor to the left of the primary one
/// has negative x, and a layout that assumes otherwise puts every window on
/// the wrong display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.height
    }

    /// Whether the two overlap at all.
    pub const fn meets(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// How much of `self` lies inside `other`, in pixels.
    ///
    /// Used to decide which monitor a window is "on" when it straddles two.
    pub fn overlap(&self, other: &Rect) -> i64 {
        let width = (self.right().min(other.right()) - self.x.max(other.x)).max(0) as i64;
        let height = (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0) as i64;
        width * height
    }
}

/// One top-level window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    /// The window handle, as a number.
    ///
    /// Meaningful only while the window lives and only on this machine, so it
    /// is never persisted. Every operation revalidates it, because a handle
    /// can be reused by a different window after the first one closes and
    /// acting on a stale one would act on a stranger.
    pub id: isize,
    pub title: String,
    /// What a person calls the application, from its executable.
    pub app: String,
    pub app_path: String,
    pub pid: u32,
    pub minimized: bool,
    pub maximized: bool,
    /// Where the window appears, as the user sees it. See [`frame`].
    pub rect: Rect,
    /// Index into [`monitors`].
    pub monitor: usize,
}

/// One display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Monitor {
    pub index: usize,
    /// The whole display.
    pub full: Rect,
    /// The display minus the taskbar and any other appbar.
    ///
    /// This is what windows are laid out against. Using `full` puts the bottom
    /// of every window behind the taskbar.
    pub work: Rect,
    pub primary: bool,
}

/// Where a window can be sent.
///
/// Named positions rather than free rectangles because these are what a person
/// asks for. A free rectangle is still available through [`place`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Slot {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    FirstThird,
    CenterThird,
    LastThird,
    FirstTwoThirds,
    LastTwoThirds,
    Center,
    Fill,
}

impl Slot {
    /// Everything a menu offers, in the order it reads.
    pub const ALL: [Slot; 15] = [
        Slot::Left,
        Slot::Right,
        Slot::Top,
        Slot::Bottom,
        Slot::TopLeft,
        Slot::TopRight,
        Slot::BottomLeft,
        Slot::BottomRight,
        Slot::FirstThird,
        Slot::CenterThird,
        Slot::LastThird,
        Slot::FirstTwoThirds,
        Slot::LastTwoThirds,
        Slot::Center,
        Slot::Fill,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Slot::Left => "Left Half",
            Slot::Right => "Right Half",
            Slot::Top => "Top Half",
            Slot::Bottom => "Bottom Half",
            Slot::TopLeft => "Top Left Quarter",
            Slot::TopRight => "Top Right Quarter",
            Slot::BottomLeft => "Bottom Left Quarter",
            Slot::BottomRight => "Bottom Right Quarter",
            Slot::FirstThird => "First Third",
            Slot::CenterThird => "Centre Third",
            Slot::LastThird => "Last Third",
            Slot::FirstTwoThirds => "First Two Thirds",
            Slot::LastTwoThirds => "Last Two Thirds",
            Slot::Center => "Centre",
            Slot::Fill => "Fill Screen",
        }
    }

    /// The stable id an action and a binding refer to it by.
    pub const fn id(self) -> &'static str {
        match self {
            Slot::Left => "left",
            Slot::Right => "right",
            Slot::Top => "top",
            Slot::Bottom => "bottom",
            Slot::TopLeft => "topLeft",
            Slot::TopRight => "topRight",
            Slot::BottomLeft => "bottomLeft",
            Slot::BottomRight => "bottomRight",
            Slot::FirstThird => "firstThird",
            Slot::CenterThird => "centerThird",
            Slot::LastThird => "lastThird",
            Slot::FirstTwoThirds => "firstTwoThirds",
            Slot::LastTwoThirds => "lastTwoThirds",
            Slot::Center => "center",
            Slot::Fill => "fill",
        }
    }

    /// The action id a binding and the action panel refer to it by.
    ///
    /// Spelled out rather than built from [`Self::id`] because the trait wants
    /// a `&'static str` and there is nowhere to keep a built one. The two are
    /// held in step by a test rather than by construction, which is the trade
    /// being made here and the reason that test exists.
    pub const fn action_id(self) -> &'static str {
        match self {
            Slot::Left => "sill.window.snap.left",
            Slot::Right => "sill.window.snap.right",
            Slot::Top => "sill.window.snap.top",
            Slot::Bottom => "sill.window.snap.bottom",
            Slot::TopLeft => "sill.window.snap.topLeft",
            Slot::TopRight => "sill.window.snap.topRight",
            Slot::BottomLeft => "sill.window.snap.bottomLeft",
            Slot::BottomRight => "sill.window.snap.bottomRight",
            Slot::FirstThird => "sill.window.snap.firstThird",
            Slot::CenterThird => "sill.window.snap.centerThird",
            Slot::LastThird => "sill.window.snap.lastThird",
            Slot::FirstTwoThirds => "sill.window.snap.firstTwoThirds",
            Slot::LastTwoThirds => "sill.window.snap.lastTwoThirds",
            Slot::Center => "sill.window.snap.center",
            Slot::Fill => "sill.window.snap.fill",
        }
    }

    pub fn from_id(id: &str) -> Option<Slot> {
        Slot::ALL.into_iter().find(|slot| slot.id() == id)
    }
}

/// Where a slot puts a window on a given work area.
///
/// **Halves and thirds are computed so they tile exactly.** Taking the floor
/// of a third three times leaves up to two pixels of desktop showing down the
/// right-hand edge, which is the kind of thing that looks like a rendering bug
/// rather than arithmetic. Each division ends at the next boundary rather than
/// carrying its own width, so the last one absorbs the remainder.
pub fn slot_rect(slot: Slot, work: Rect) -> Rect {
    let half_w = work.width / 2;
    let half_h = work.height / 2;
    let third = work.width / 3;
    let two_thirds = (work.width * 2) / 3;

    match slot {
        Slot::Left => Rect::new(work.x, work.y, half_w, work.height),
        Slot::Right => Rect::new(
            work.x + half_w,
            work.y,
            work.width - half_w,
            work.height,
        ),
        Slot::Top => Rect::new(work.x, work.y, work.width, half_h),
        Slot::Bottom => Rect::new(
            work.x,
            work.y + half_h,
            work.width,
            work.height - half_h,
        ),
        Slot::TopLeft => Rect::new(work.x, work.y, half_w, half_h),
        Slot::TopRight => Rect::new(work.x + half_w, work.y, work.width - half_w, half_h),
        Slot::BottomLeft => Rect::new(work.x, work.y + half_h, half_w, work.height - half_h),
        Slot::BottomRight => Rect::new(
            work.x + half_w,
            work.y + half_h,
            work.width - half_w,
            work.height - half_h,
        ),
        Slot::FirstThird => Rect::new(work.x, work.y, third, work.height),
        Slot::CenterThird => Rect::new(work.x + third, work.y, two_thirds - third, work.height),
        Slot::LastThird => Rect::new(
            work.x + two_thirds,
            work.y,
            work.width - two_thirds,
            work.height,
        ),
        Slot::FirstTwoThirds => Rect::new(work.x, work.y, two_thirds, work.height),
        Slot::LastTwoThirds => Rect::new(
            work.x + third,
            work.y,
            work.width - third,
            work.height,
        ),
        // Two thirds of the work area, centred. A "centre" that kept the
        // window's own size would do nothing at all to a maximized window,
        // which is the state people most often want out of.
        Slot::Center => {
            let width = (work.width * 2) / 3;
            let height = (work.height * 2) / 3;
            Rect::new(
                work.x + (work.width - width) / 2,
                work.y + (work.height - height) / 2,
                width,
                height,
            )
        }
        Slot::Fill => work,
    }
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod platform {
    use super::{Monitor, Rect, Window};

    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
        PostMessageW, SetWindowPos, ShowWindow, GA_ROOTOWNER, GWL_EXSTYLE, HWND_TOP,
        MONITORINFOF_PRIMARY, SWP_NOACTIVATE, SWP_NOZORDER, SW_MINIMIZE, SW_RESTORE,
        SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, WINDOWPLACEMENT, WM_CLOSE, WS_EX_TOOLWINDOW,
    };

    fn hwnd_of(id: isize) -> HWND {
        HWND(id as *mut core::ffi::c_void)
    }

    fn rect_of(r: RECT) -> Rect {
        Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
    }

    /// Every window the user would consider a window.
    ///
    /// This is the Alt-Tab rule, and every clause earns its place:
    ///
    /// - Visible, and with a title. A window with no caption is not something
    ///   anyone is looking for by name.
    /// - Not a tool window. `WS_EX_TOOLWINDOW` is how an application says "this
    ///   is a palette, not a document".
    /// - Its own root owner. Otherwise every modal dialog and every owned popup
    ///   appears as a peer of the window it belongs to.
    /// - **Not cloaked.** This is the clause everyone forgets. A suspended
    ///   store application leaves behind a window that is visible, titled,
    ///   non-tool and its own owner, and is not on screen at all. Without this
    ///   the list fills with ghosts, and worse, focusing one does nothing and
    ///   looks like Sill is broken.
    fn is_listable(hwnd: HWND) -> bool {
        // SAFETY: every call takes a window handle and returns a value; the
        // handle came from EnumWindows and is checked by Windows itself.
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return false;
            }

            if GetWindowTextLengthW(hwnd) == 0 {
                return false;
            }

            let extended = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            if extended & WS_EX_TOOLWINDOW.0 != 0 {
                return false;
            }

            if GetAncestor(hwnd, GA_ROOTOWNER) != hwnd {
                return false;
            }

            let mut cloaked = 0u32;
            let read = DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );

            // An error means DWM has nothing to say about this window, which
            // is not the same as it being cloaked.
            if read.is_ok() && cloaked != 0 {
                return false;
            }

            true
        }
    }

    fn title_of(hwnd: HWND) -> String {
        // SAFETY: the buffer is sized from the length Windows just reported,
        // plus room for the terminator it writes.
        unsafe {
            let length = GetWindowTextLengthW(hwnd);
            if length <= 0 {
                return String::new();
            }

            let mut buffer = vec![0u16; length as usize + 1];
            let written = GetWindowTextW(hwnd, &mut buffer);
            String::from_utf16_lossy(&buffer[..written as usize])
        }
    }

    /// Where the window appears, rather than where Win32 says it is.
    ///
    /// Since Windows 10, `GetWindowRect` includes an invisible resize border
    /// several pixels wide on the left, right and bottom. Laying two windows
    /// out side by side using those numbers leaves a visible gap between them
    /// and a gap at the screen edge, which is the single most common way
    /// window management on this platform looks subtly wrong. DWM's extended
    /// frame bounds are what the user actually sees.
    fn frame(hwnd: HWND) -> Option<Rect> {
        // SAFETY: the out parameter is a correctly sized RECT and the size is
        // taken from the type.
        unsafe {
            let mut bounds = RECT::default();
            let read = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut bounds as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<RECT>() as u32,
            );

            if read.is_ok() {
                return Some(rect_of(bounds));
            }

            let mut raw = RECT::default();
            GetWindowRect(hwnd, &mut raw).ok()?;
            Some(rect_of(raw))
        }
    }

    /// How far the real window extends past what the user sees, per edge.
    ///
    /// Added back when placing, so asking for the left half puts the *visible*
    /// edge on the screen edge rather than a few pixels in.
    fn shadow(hwnd: HWND) -> (i32, i32, i32, i32) {
        // SAFETY: both calls fill a RECT that is sized correctly here.
        unsafe {
            let mut raw = RECT::default();
            if GetWindowRect(hwnd, &mut raw).is_err() {
                return (0, 0, 0, 0);
            }

            let mut visible = RECT::default();
            let read = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut visible as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<RECT>() as u32,
            );

            if read.is_err() {
                return (0, 0, 0, 0);
            }

            (
                visible.left - raw.left,
                visible.top - raw.top,
                raw.right - visible.right,
                raw.bottom - visible.bottom,
            )
        }
    }

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: the pointer is the Vec passed in by `list` below, which
        // outlives the enumeration because EnumWindows is synchronous.
        let found = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
        if is_listable(hwnd) {
            found.push(hwnd.0 as isize);
        }
        BOOL(1)
    }

    pub fn list() -> Vec<Window> {
        let mut handles: Vec<isize> = Vec::new();

        // SAFETY: the callback matches the required signature and the pointer
        // points at a live Vec for the duration of this synchronous call.
        unsafe {
            let _ = EnumWindows(
                Some(collect),
                LPARAM(&mut handles as *mut Vec<isize> as isize),
            );
        }

        let displays = monitors();
        handles
            .into_iter()
            .filter_map(|id| describe(id, &displays))
            .collect()
    }

    pub fn find(id: isize) -> Option<Window> {
        describe(id, &monitors())
    }

    fn describe(id: isize, displays: &[Monitor]) -> Option<Window> {
        let hwnd = hwnd_of(id);

        // SAFETY: handle-taking calls only. IsWindow is what makes the rest
        // safe against a handle whose window has closed since enumeration.
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() {
                return None;
            }

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));

            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            let placed = GetWindowPlacement(hwnd, &mut placement).is_ok();

            let rect = frame(hwnd)?;
            let app = crate::dictation::context::of_pid(pid);

            Some(Window {
                id,
                title: title_of(hwnd),
                app: app
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                app_path: app.map(|a| a.path).unwrap_or_default(),
                pid,
                minimized: placed && placement.showCmd == SW_SHOWMINIMIZED.0 as u32,
                maximized: placed && placement.showCmd == SW_SHOWMAXIMIZED.0 as u32,
                rect,
                monitor: nearest(rect, displays),
            })
        }
    }

    /// Which display a rectangle mostly sits on.
    ///
    /// By overlap area rather than by its top-left corner: a window dragged so
    /// that only its title bar is on the left monitor belongs to the right one
    /// as far as anybody looking at it is concerned.
    fn nearest(rect: Rect, displays: &[Monitor]) -> usize {
        displays
            .iter()
            .max_by_key(|monitor| rect.overlap(&monitor.full))
            .map(|monitor| monitor.index)
            .unwrap_or(0)
    }

    unsafe extern "system" fn gather(
        handle: HMONITOR,
        _dc: HDC,
        _clip: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        // SAFETY: the pointer is the Vec passed in by `monitors`, live for the
        // duration of this synchronous call.
        let found = unsafe { &mut *(lparam.0 as *mut Vec<Monitor>) };

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };

        // SAFETY: cbSize is set as the API requires.
        if unsafe { GetMonitorInfoW(handle, &mut info) }.as_bool() {
            found.push(Monitor {
                index: found.len(),
                full: rect_of(info.rcMonitor),
                work: rect_of(info.rcWork),
                primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
            });
        }

        BOOL(1)
    }

    pub fn monitors() -> Vec<Monitor> {
        let mut found: Vec<Monitor> = Vec::new();

        // SAFETY: the callback matches the required signature and the pointer
        // points at a live Vec for this synchronous call.
        unsafe {
            let _ = EnumDisplayMonitors(
                None,
                None,
                Some(gather),
                LPARAM(&mut found as *mut Vec<Monitor> as isize),
            );
        }

        found
    }

    /// The display a window is on, for laying it out.
    pub fn monitor_of(id: isize) -> Option<Monitor> {
        let displays = monitors();
        let window = describe(id, &displays)?;
        displays.into_iter().nth(window.monitor)
    }

    fn alive(id: isize) -> Result<HWND, String> {
        let hwnd = hwnd_of(id);
        // SAFETY: takes a handle, returns a bool, dereferences nothing.
        if unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            Ok(hwnd)
        } else {
            Err("that window has closed".to_string())
        }
    }

    pub fn focus(id: isize) -> Result<(), String> {
        let hwnd = alive(id)?;

        // A minimized window cannot take focus. Restoring first is what makes
        // "switch to this" work from the switcher, which is the whole point of
        // listing minimized windows at all.
        // SAFETY: the handle is checked live directly above.
        unsafe {
            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            if GetWindowPlacement(hwnd, &mut placement).is_ok()
                && placement.showCmd == SW_SHOWMINIMIZED.0 as u32
            {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }

        crate::summon::force_foreground(hwnd);
        Ok(())
    }

    /// Asks the window to close, the same way its own close button does.
    ///
    /// `WM_CLOSE`, never `TerminateProcess`: the application gets to run its
    /// shutdown, and gets to put up "save changes?" if there is unsaved work.
    /// A launcher that can silently discard somebody's document is not one
    /// anybody should install.
    pub fn close(id: isize) -> Result<(), String> {
        let hwnd = alive(id)?;

        // SAFETY: posts a message to a live window and returns immediately.
        unsafe {
            PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))
                .map_err(|err| format!("that window would not close: {err}"))
        }
    }

    pub fn minimize(id: isize) -> Result<(), String> {
        let hwnd = alive(id)?;
        // SAFETY: the handle is checked live directly above.
        unsafe { let _ = ShowWindow(hwnd, SW_MINIMIZE); };
        Ok(())
    }

    pub fn maximize(id: isize) -> Result<(), String> {
        let hwnd = alive(id)?;
        // SAFETY: the handle is checked live directly above.
        unsafe { let _ = ShowWindow(hwnd, SW_SHOWMAXIMIZED); };
        Ok(())
    }

    pub fn restore(id: isize) -> Result<(), String> {
        let hwnd = alive(id)?;
        // SAFETY: the handle is checked live directly above.
        unsafe { let _ = ShowWindow(hwnd, SW_RESTORE); };
        Ok(())
    }

    /// Moves and resizes, in visible coordinates.
    ///
    /// `rect` is where the window should *appear*. The invisible resize border
    /// is added back here, so callers work in the coordinates they can see.
    pub fn place(id: isize, rect: Rect) -> Result<(), String> {
        let hwnd = alive(id)?;

        // A maximized window ignores SetWindowPos and snaps back the moment it
        // is touched. Restoring first is what makes "left half" work on a
        // maximized window rather than silently doing nothing.
        // SAFETY: the handle is checked live directly above.
        unsafe {
            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            if GetWindowPlacement(hwnd, &mut placement).is_ok()
                && (placement.showCmd == SW_SHOWMAXIMIZED.0 as u32
                    || placement.showCmd == SW_SHOWMINIMIZED.0 as u32)
            {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }

        let (left, top, right, bottom) = shadow(hwnd);

        // SAFETY: the handle is checked live above and the flags are valid.
        unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOP),
                rect.x - left,
                rect.y - top,
                rect.width + left + right,
                rect.height + top + bottom,
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .map_err(|err| format!("that window would not move: {err}"))
        }
    }

    /// Where the pointer is, for choosing a monitor with no window in hand.
    pub fn cursor_monitor() -> Option<Monitor> {
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        // SAFETY: fills a POINT sized here; the monitor handle is used only by
        // GetMonitorInfoW.
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;

            let displays = monitors();
            displays
                .iter()
                .find(|monitor| {
                    point.x >= monitor.full.x
                        && point.x < monitor.full.right()
                        && point.y >= monitor.full.y
                        && point.y < monitor.full.bottom()
                })
                .cloned()
                .or_else(|| displays.into_iter().next())
        }
    }

    /// The window the user was last in, from Sill's point of view.
    ///
    /// Sill's own window is skipped: a shortcut pressed while the launcher is
    /// open means "the thing behind me", never the launcher.
    pub fn foreground() -> Option<Window> {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

        // SAFETY: returns a handle or null; nothing is dereferenced.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0.is_null() {
            return None;
        }

        let mut pid = 0u32;
        // SAFETY: fills a u32 declared here.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == std::process::id() {
            return None;
        }

        find(hwnd.0 as isize)
    }

    /// Kept so the layout code can be read against what DWM reports.
    pub fn visible_frame(id: isize) -> Option<Rect> {
        frame(hwnd_of(id))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Monitor, Rect, Window};

    pub fn list() -> Vec<Window> {
        Vec::new()
    }
    pub fn find(_id: isize) -> Option<Window> {
        None
    }
    pub fn monitors() -> Vec<Monitor> {
        Vec::new()
    }
    pub fn monitor_of(_id: isize) -> Option<Monitor> {
        None
    }
    pub fn focus(_id: isize) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn close(_id: isize) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn minimize(_id: isize) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn maximize(_id: isize) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn restore(_id: isize) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn place(_id: isize, _rect: Rect) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn cursor_monitor() -> Option<Monitor> {
        None
    }
    pub fn foreground() -> Option<Window> {
        None
    }
    pub fn visible_frame(_id: isize) -> Option<Rect> {
        None
    }
}

pub use platform::{
    close, cursor_monitor, find, focus, foreground, list, maximize, minimize, monitor_of, monitors,
    place, restore, visible_frame,
};

/// The open windows, as things the ranker already knows how to sort.
///
/// Built rather than indexed, on every query. A window list has no business
/// being cached: it is wrong the moment anything is opened, closed or renamed,
/// and enumerating it costs less than checking whether a cache is stale.
///
/// The application name goes in `extension_title`, which ranking weights twice
/// as heavily as a keyword. That is what makes typing "chrome" find every
/// Chrome window when not one of them has "chrome" in its title.
pub fn records() -> Vec<crate::registry::CommandRecord> {
    list()
        .into_iter()
        .map(|window| {
            let title = if window.title.is_empty() {
                window.app.clone()
            } else {
                window.title.clone()
            };

            crate::registry::CommandRecord {
                id: format!("window:{}", window.id),
                extension: "window".to_string(),
                extension_title: window.app.clone(),
                command: title.clone(),
                title,
                subtitle: if window.minimized {
                    format!("{}, minimized", window.app)
                } else {
                    window.app.clone()
                },
                description: String::new(),
                mode: "window".to_string(),
                // The handle, which is what every window action parses back.
                entrypoint: window.id.to_string(),
                keywords: Vec::new(),
                // The executable, so the row draws the application's real icon
                // rather than a lettered tile.
                icon: if window.app_path.is_empty() {
                    None
                } else {
                    Some(window.app_path.clone())
                },
                panel: None,
                preferences: serde_json::Value::Null,
            }
        })
        .collect()
}

/// Sends a window to a named slot on the display it is already on.
///
/// Returns where it was, so the move can be undone.
pub fn snap(id: isize, slot: Slot) -> Result<Rect, String> {
    let window = find(id).ok_or_else(|| "that window has closed".to_string())?;
    let monitor = monitor_of(id).ok_or_else(|| "that window is on no display".to_string())?;

    let was = window.rect;
    place(id, slot_rect(slot, monitor.work))?;
    Ok(was)
}

/// Moves a window to the next display along, keeping its relative position.
///
/// Relative rather than absolute: a window in the top-left of a 4K display
/// placed at the same coordinates on a 1080p one lands mostly off-screen.
pub fn send_to_monitor(id: isize, target: usize) -> Result<Rect, String> {
    let displays = monitors();
    if displays.len() < 2 {
        return Err("there is only one display".to_string());
    }

    let window = find(id).ok_or_else(|| "that window has closed".to_string())?;
    let from = displays
        .get(window.monitor)
        .ok_or_else(|| "that window is on no display".to_string())?;
    let to = displays
        .get(target % displays.len())
        .ok_or_else(|| "there is no such display".to_string())?;

    let was = window.rect;
    place(id, rescale(was, from.work, to.work))?;
    Ok(was)
}

/// The same window, in the same place proportionally, on a different display.
pub fn rescale(rect: Rect, from: Rect, to: Rect) -> Rect {
    if from.width <= 0 || from.height <= 0 {
        return rect;
    }

    let scale_x = to.width as f64 / from.width as f64;
    let scale_y = to.height as f64 / from.height as f64;

    let width = ((rect.width as f64 * scale_x).round() as i32).min(to.width);
    let height = ((rect.height as f64 * scale_y).round() as i32).min(to.height);

    let x = to.x + (((rect.x - from.x) as f64 * scale_x).round() as i32);
    let y = to.y + (((rect.y - from.y) as f64 * scale_y).round() as i32);

    // Nudged back on-screen rather than clamped to the origin, so a window
    // near the right edge stays near the right edge.
    Rect::new(
        x.min(to.right() - width).max(to.x),
        y.min(to.bottom() - height).max(to.y),
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1080p display with a 40px taskbar, which is the ordinary case.
    const WORK: Rect = Rect::new(0, 0, 1920, 1040);

    #[test]
    fn halves_tile_exactly() {
        // Two halves that do not add up leave a strip of desktop showing
        // between them, which reads as a rendering bug rather than as
        // arithmetic.
        let left = slot_rect(Slot::Left, WORK);
        let right = slot_rect(Slot::Right, WORK);

        assert_eq!(left.right(), right.x, "no gap and no overlap");
        assert_eq!(left.width + right.width, WORK.width);
        assert_eq!(right.right(), WORK.right());
    }

    #[test]
    fn thirds_tile_exactly_on_a_width_that_does_not_divide() {
        // 1921 / 3 is 640.33. Three windows of 640 leave a pixel showing.
        let work = Rect::new(0, 0, 1921, 1040);

        let first = slot_rect(Slot::FirstThird, work);
        let middle = slot_rect(Slot::CenterThird, work);
        let last = slot_rect(Slot::LastThird, work);

        assert_eq!(first.right(), middle.x);
        assert_eq!(middle.right(), last.x);
        assert_eq!(
            first.width + middle.width + last.width,
            work.width,
            "the remainder has to land somewhere"
        );
        assert_eq!(last.right(), work.right());
    }

    #[test]
    fn quarters_tile_exactly() {
        let corners = [
            slot_rect(Slot::TopLeft, WORK),
            slot_rect(Slot::TopRight, WORK),
            slot_rect(Slot::BottomLeft, WORK),
            slot_rect(Slot::BottomRight, WORK),
        ];

        let covered: i64 = corners.iter().map(|r| r.width as i64 * r.height as i64).sum();
        assert_eq!(covered, WORK.width as i64 * WORK.height as i64);

        for (a, b) in [(0, 1), (0, 2), (1, 3), (2, 3), (0, 3), (1, 2)] {
            assert!(
                !corners[a].meets(&corners[b]),
                "{a} and {b} overlap: {:?} {:?}",
                corners[a],
                corners[b]
            );
        }
    }

    #[test]
    fn two_thirds_meet_the_matching_third() {
        // "First two thirds" beside "last third" is the common asymmetric
        // layout, and it only works if the two agree on where the boundary is.
        let first_two = slot_rect(Slot::FirstTwoThirds, WORK);
        let last = slot_rect(Slot::LastThird, WORK);
        assert_eq!(first_two.right(), last.x);

        let first = slot_rect(Slot::FirstThird, WORK);
        let last_two = slot_rect(Slot::LastTwoThirds, WORK);
        assert_eq!(first.right(), last_two.x);
        assert_eq!(last_two.right(), WORK.right());
    }

    #[test]
    fn every_slot_stays_inside_the_work_area() {
        // The work area excludes the taskbar. A slot that runs past it puts
        // the bottom of the window behind the taskbar.
        for slot in Slot::ALL {
            let rect = slot_rect(slot, WORK);
            assert!(rect.x >= WORK.x, "{slot:?} starts left of the display");
            assert!(rect.y >= WORK.y, "{slot:?} starts above the display");
            assert!(rect.right() <= WORK.right(), "{slot:?} runs off the right");
            assert!(
                rect.bottom() <= WORK.bottom(),
                "{slot:?} runs under the taskbar"
            );
            assert!(rect.width > 0 && rect.height > 0, "{slot:?} is empty");
        }
    }

    #[test]
    fn slots_work_on_a_display_that_is_not_at_the_origin() {
        // A second monitor to the left of the primary has negative x. Code
        // that assumes a display starts at 0,0 puts every window on the wrong
        // screen, and it is invisible on a single-monitor machine.
        let left_of_primary = Rect::new(-1920, -200, 1920, 1040);

        for slot in Slot::ALL {
            let rect = slot_rect(slot, left_of_primary);
            assert!(
                rect.x >= left_of_primary.x && rect.right() <= left_of_primary.right(),
                "{slot:?} left the display: {rect:?}"
            );
            assert!(
                rect.y >= left_of_primary.y && rect.bottom() <= left_of_primary.bottom(),
                "{slot:?} left the display: {rect:?}"
            );
        }
    }

    #[test]
    fn centre_shrinks_a_maximized_window_rather_than_doing_nothing() {
        let centred = slot_rect(Slot::Center, WORK);
        assert!(centred.width < WORK.width);
        assert!(centred.height < WORK.height);

        // Equal margins on both sides, within the rounding.
        let left = centred.x - WORK.x;
        let right = WORK.right() - centred.right();
        assert!((left - right).abs() <= 1, "left {left} right {right}");
    }

    #[test]
    fn a_slot_id_survives_the_round_trip() {
        // Ids go into preferences and into action ids. A slot that cannot be
        // read back is a binding that silently stops working.
        for slot in Slot::ALL {
            assert_eq!(Slot::from_id(slot.id()), Some(slot), "{slot:?}");
        }
        assert_eq!(Slot::from_id("nonsense"), None);
    }

    #[test]
    fn an_action_id_is_its_slot_id_and_stays_that_way() {
        // Written out by hand because the trait needs a static string, so the
        // only thing stopping the two drifting is this.
        for slot in Slot::ALL {
            assert_eq!(
                slot.action_id(),
                format!("sill.window.snap.{}", slot.id()),
                "{slot:?}"
            );
        }
    }

    #[test]
    fn no_two_slots_share_an_id_or_a_title() {
        let mut ids = std::collections::HashSet::new();
        let mut titles = std::collections::HashSet::new();

        for slot in Slot::ALL {
            assert!(ids.insert(slot.id()), "{:?} shares an id", slot);
            assert!(titles.insert(slot.title()), "{:?} shares a title", slot);
            assert!(
                ids.insert(slot.action_id()),
                "{:?} shares an action id",
                slot
            );
        }
    }

    #[test]
    fn moving_to_a_bigger_display_keeps_the_window_proportional() {
        let small = Rect::new(0, 0, 1920, 1040);
        let big = Rect::new(1920, 0, 3840, 2120);

        // The left half of the small display.
        let moved = rescale(Rect::new(0, 0, 960, 1040), small, big);

        assert_eq!(moved.x, big.x, "still against the left edge");
        assert_eq!(moved.width, 1920, "still half the width");
        assert_eq!(moved.height, 2120, "still full height");
    }

    #[test]
    fn a_window_near_an_edge_stays_near_that_edge() {
        // Absolute coordinates would put this off the smaller screen entirely.
        let big = Rect::new(0, 0, 3840, 2120);
        let small = Rect::new(3840, 0, 1920, 1040);

        let moved = rescale(Rect::new(3340, 1900, 500, 220), big, small);

        assert!(moved.right() <= small.right(), "{moved:?} ran off the right");
        assert!(moved.bottom() <= small.bottom(), "{moved:?} ran off the bottom");
        assert_eq!(moved.right(), small.right(), "it was against the right edge");
    }

    #[test]
    fn a_window_bigger_than_the_target_display_is_cut_down_to_fit() {
        let big = Rect::new(0, 0, 3840, 2120);
        let small = Rect::new(3840, 0, 1920, 1040);

        let moved = rescale(big, big, small);
        assert!(moved.width <= small.width && moved.height <= small.height, "{moved:?}");
        assert!(moved.x >= small.x && moved.y >= small.y, "{moved:?}");
    }

    #[test]
    fn a_degenerate_display_does_not_divide_by_zero() {
        // A display can report a zero-sized work area while it is being
        // reconfigured, which is exactly when a window move might be running.
        let broken = Rect::new(0, 0, 0, 0);
        let rect = Rect::new(10, 10, 100, 100);
        assert_eq!(rescale(rect, broken, WORK), rect);
    }

    #[test]
    fn overlap_picks_the_display_a_window_is_mostly_on() {
        let left = Rect::new(0, 0, 1920, 1080);
        let right = Rect::new(1920, 0, 1920, 1080);

        // Only its title bar is on the left display.
        let straddling = Rect::new(1820, 100, 800, 600);

        assert!(
            straddling.overlap(&right) > straddling.overlap(&left),
            "a window mostly on the right belongs to the right"
        );
    }

    #[test]
    fn rectangles_that_touch_do_not_count_as_overlapping() {
        // Tiled halves share an edge. Counting that as an overlap would make
        // every tiling test fail for no reason.
        let left = Rect::new(0, 0, 100, 100);
        let right = Rect::new(100, 0, 100, 100);

        assert!(!left.meets(&right));
        assert_eq!(left.overlap(&right), 0);
    }
}
