//! Moving, focusing and closing a real window.
//!
//! The window is created by the test, which is the only honest way to check
//! this: acting on whatever happens to be open would move the user's own
//! windows around, and an earlier run of a different feature already replaced
//! the contents of a Notepad window that had unsaved work in it.
//!
//! What this catches that a unit test cannot: whether the rectangle a slot
//! computes is the rectangle the window ends up occupying. Those differ by the
//! invisible resize border Windows 10 added, and getting it wrong leaves a gap
//! at every screen edge that no amount of arithmetic testing would reveal.

#![cfg(windows)]

use std::time::{Duration, Instant};

use sill_lib::windowing::{self, Rect, Slot};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW, RegisterClassW,
    ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG, PM_REMOVE, SW_SHOW, WINDOW_EX_STYLE,
    WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

/// A window the test owns, destroyed however the test ends.
struct TestWindow(HWND);

impl TestWindow {
    fn open(title: PCWSTR) -> Self {
        // SAFETY: the class is registered before it is used and the window
        // procedure is the system default, which handles every message.
        unsafe {
            let class = w!("SillWindowControlTest");
            let wnd = WNDCLASSW {
                lpfnWndProc: Some(passthrough),
                lpszClassName: class,
                ..Default::default()
            };
            // Registering twice returns 0 and sets an error; harmless, because
            // the class from the first test in the binary is the one wanted.
            RegisterClassW(&wnd);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                title,
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                600,
                400,
                None,
                None,
                None,
                None,
            )
            .expect("the test window is created");

            let _ = ShowWindow(hwnd, SW_SHOW);
            let this = Self(hwnd);
            this.settle();
            this
        }
    }

    fn id(&self) -> isize {
        self.0 .0 as isize
    }

    /// Runs the message loop briefly.
    ///
    /// Required, and not merely polite. A window that never pumps is not
    /// resized by `SetWindowPos`: the call posts messages the window has to
    /// process, and DWM reports the old frame bounds until it does.
    fn settle(&self) {
        let until = Instant::now() + Duration::from_millis(250);
        while Instant::now() < until {
            // SAFETY: MSG is zeroed here and only read after PeekMessage
            // reports it was filled.
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        // SAFETY: the handle was created here and is destroyed exactly once.
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

/// The display the test window landed on, which is not assumed to be primary.
fn work_area(id: isize) -> Rect {
    windowing::monitor_of(id)
        .expect("the window is on a display")
        .work
}

#[test]
fn the_launcher_never_lists_its_own_windows() {
    // Sill is the frontmost window whenever the switcher is open, and
    // enumeration is in Z-order, so without this it would sit at the top of
    // its own list offering to switch to where you already are.
    //
    // The test's window stands in for Sill's: same process, so the same rule
    // applies to it.
    let window = TestWindow::open(w!("Sill test: not listed"));

    let found = windowing::list();
    assert!(
        !found.iter().any(|w| w.id == window.id()),
        "a window from Sill's own process was listed"
    );
    assert!(
        found.iter().all(|w| w.pid != std::process::id()),
        "some window from this process got through"
    );

    // Excluded from the list, not made unreachable. Every action still works
    // on it, which is what lets the launcher act on a window it was handed.
    let direct = windowing::find(window.id()).expect("still findable by handle");
    assert_eq!(direct.title, "Sill test: not listed");
}

#[test]
fn the_list_is_frontmost_first() {
    // The switcher's whole value is that the first entry is the window you
    // were last in. That order comes from EnumWindows walking the Z-order,
    // so it survives only as long as nothing re-sorts it.
    //
    /*
     * Read until one reading holds still, and judge that one.
     *
     * The claim is about the order Windows keeps, not about how long the
     * desktop stands still, and a desktop that is changing gives two calls two
     * different worlds: the list is walked before a window appears and the
     * foreground is read after it. **A second copy of this binary does that
     * continuously**, opening and closing test windows of its own, and the
     * failure it produced named a window this process had never created.
     *
     * Own-process windows cannot cause it. They are left out of the list on
     * purpose and `foreground` answers `None` for them, so churn from the
     * sibling tests running beside this one is silence rather than a wrong
     * answer.
     *
     * On a still desktop the first reading is the only one, so this costs
     * nothing in the ordinary case.
     */
    let mut disagreed = None;

    for _ in 0..5 {
        let before = windowing::foreground().map(|window| window.id);
        let found = windowing::list();
        let front = windowing::foreground();

        if found.len() < 2 {
            // Nothing to order. Not a failure: the desktop is whatever is open.
            return;
        }

        let Some(front) = front else {
            // Sill's own windows are not in the list and one of them is at the
            // front, so there is nothing here to compare.
            return;
        };

        if before == Some(front.id) {
            if found[0].id == front.id {
                return;
            }

            disagreed = Some(format!(
                "the list starts with {:?} but the foreground window is {:?}",
                found[0].title, front.title
            ));
        }

        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Every reading that held still put the wrong window first, which is the
    // order being wrong rather than the desktop being busy.
    if let Some(why) = disagreed {
        panic!("{why}");
    }
}

#[test]
fn snapping_puts_the_visible_edges_exactly_where_the_slot_says() {
    // The one that catches the invisible resize border. Windows 10 draws a
    // window several pixels wider than it looks, so placing by GetWindowRect
    // leaves a visible gap at the screen edge and between tiled windows.
    let window = TestWindow::open(w!("Sill test: snapped"));
    let work = work_area(window.id());

    for slot in [Slot::Left, Slot::Right, Slot::TopLeft, Slot::Fill] {
        let wanted = windowing::slot_rect(slot, work);

        windowing::snap(window.id(), slot).expect("the window snaps");
        window.settle();

        let got = windowing::find(window.id()).expect("still open").rect;

        // A pixel of slack, and no more. Anything larger is the border
        // compensation being absent rather than rounding.
        for (name, wanted, got) in [
            ("x", wanted.x, got.x),
            ("y", wanted.y, got.y),
            ("width", wanted.width, got.width),
            ("height", wanted.height, got.height),
        ] {
            assert!(
                (wanted - got).abs() <= 1,
                "{slot:?} {name}: asked for {wanted}, got {got} (whole rect {got:?} vs {wanted:?})",
                got = got,
                wanted = wanted
            );
        }
    }
}

#[test]
fn tiled_halves_leave_no_gap_between_them_on_screen() {
    // The arithmetic tiles exactly; this is whether the *windows* do, which is
    // a different claim and the one a person can see.
    let left = TestWindow::open(w!("Sill test: left half"));
    let right = TestWindow::open(w!("Sill test: right half"));
    let work = work_area(left.id());

    windowing::snap(left.id(), Slot::Left).expect("left snaps");
    windowing::snap(right.id(), Slot::Right).expect("right snaps");
    left.settle();
    right.settle();

    let left_rect = windowing::find(left.id()).expect("open").rect;
    let right_rect = windowing::find(right.id()).expect("open").rect;

    assert!(
        (left_rect.right() - right_rect.x).abs() <= 1,
        "a gap of {} pixels between the halves: {left_rect:?} then {right_rect:?}",
        right_rect.x - left_rect.right()
    );
    assert!(
        (right_rect.right() - work.right()).abs() <= 1,
        "the right half does not reach the screen edge: {right_rect:?} in {work:?}"
    );
}

#[test]
fn a_maximized_window_can_still_be_snapped() {
    // SetWindowPos is ignored by a maximized window, which snaps straight back
    // and looks like the shortcut did nothing at all.
    let window = TestWindow::open(w!("Sill test: maximized"));
    let work = work_area(window.id());

    windowing::maximize(window.id()).expect("maximizes");
    window.settle();
    assert!(
        windowing::find(window.id()).expect("open").maximized,
        "it really is maximized before the snap"
    );

    windowing::snap(window.id(), Slot::Left).expect("snaps out of maximized");
    window.settle();

    let after = windowing::find(window.id()).expect("open");
    assert!(!after.maximized, "still maximized after being snapped");
    assert!(
        (after.rect.width - windowing::slot_rect(Slot::Left, work).width).abs() <= 1,
        "it did not become a half: {:?}",
        after.rect
    );
}

#[test]
fn minimize_and_restore_are_reported_as_they_happen() {
    let window = TestWindow::open(w!("Sill test: states"));

    windowing::minimize(window.id()).expect("minimizes");
    window.settle();
    assert!(windowing::find(window.id()).expect("open").minimized);

    windowing::restore(window.id()).expect("restores");
    window.settle();
    assert!(!windowing::find(window.id()).expect("open").minimized);
}

#[test]
fn a_move_can_be_taken_back_exactly() {
    // Undo is only worth offering if it returns the window to where it was,
    // not to somewhere similar.
    let window = TestWindow::open(w!("Sill test: undo"));

    let before = windowing::find(window.id()).expect("open").rect;

    windowing::snap(window.id(), Slot::TopRight).expect("snaps");
    window.settle();
    let moved = windowing::find(window.id()).expect("open").rect;
    assert_ne!(moved, before, "the snap actually moved it");

    windowing::place(window.id(), before).expect("puts it back");
    window.settle();
    let back = windowing::find(window.id()).expect("open").rect;

    for (name, a, b) in [
        ("x", before.x, back.x),
        ("y", before.y, back.y),
        ("width", before.width, back.width),
        ("height", before.height, back.height),
    ] {
        assert!(
            (a - b).abs() <= 1,
            "{name} came back as {b} rather than {a}"
        );
    }
}

#[test]
fn a_closed_window_is_gone_and_stays_refused() {
    // The lifecycle every window action depends on: once a handle stops being
    // a window, every operation refuses rather than acting on whatever took
    // the handle's place.
    let id = {
        let window = TestWindow::open(w!("Sill test: closing"));
        let id = window.id();
        assert!(windowing::find(id).is_some(), "open to begin with");
        id
    };

    // Give the destroy a moment to be processed.
    std::thread::sleep(Duration::from_millis(150));

    assert!(windowing::find(id).is_none(), "still reported as open");
    assert!(windowing::focus(id).is_err());
    assert!(windowing::place(id, Rect::new(0, 0, 100, 100)).is_err());
}

/// The test window's procedure: hand everything to the system.
///
/// `DefWindowProcW` cannot be passed directly. The `windows` crate declares it
/// as a Rust-ABI safe function, and a window class needs a `system`-ABI one,
/// so it is wrapped rather than cast.
unsafe extern "system" fn passthrough(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: forwards every message to the system handler.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
