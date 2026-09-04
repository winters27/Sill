//! Against the virtual desktops this machine actually has.
//!
//! The fixtures in `desktops.rs` decide what a desktop number means once the
//! desktops are known. They cannot say whether the undocumented interface
//! answers on this build of Windows, whether the layout read out of it is the
//! layout Microsoft shipped, or whether the documented interface will move a
//! window belonging to somebody else's process. Those three are decided by
//! somebody else's release notes rather than by this codebase, and they are
//! the whole reason `VERIFIED` is a list rather than a comparison.
//!
//! **This is what a build has to pass before it goes in `VERIFIED`.**
//!
//! Ignored, because a build agent has no desktop:
//!
//! ```text
//! cargo test --lib real_desktops -- --ignored --nocapture
//! ```

#[cfg(windows)]
use crate::desktops::{self, Reach};

/// What this machine says about itself, and whether the gate opens.
#[test]
#[ignore]
#[cfg(windows)]
fn reports_what_this_build_is_allowed_to_be_asked() {
    let build = desktops::build();
    let reach = desktops::reach(build);

    println!("Windows build {build}, reach {reach:?}");
    println!("verified builds: {:?}", desktops::VERIFIED);

    assert!(build > 0, "the build number could not be read at all");

    if reach != Reach::Ordered {
        println!(
            "this build is not in VERIFIED, so windows on other desktops will be listed \
             without a number. Run the rest of this file, and if it passes, add {build}."
        );
    }
}

/// The undocumented list, held against the documented interface.
///
/// The tripwire, run for real. `desktops::desktops` refuses unless the
/// interface the pinned identity hands back names the same current desktop
/// that the documented `GetWindowDesktopId` names for the window in front, so
/// this passing is the evidence that the vtable was read at the right offsets.
#[test]
#[ignore]
#[cfg(windows)]
fn the_two_interfaces_describe_the_same_desktop() {
    let here = desktops::here().expect("the documented interface named no desktop");
    println!("documented: this desktop is {here}");

    let all = match desktops::desktops() {
        Ok(all) => all,
        Err(why) => panic!(
            "the undocumented half would not answer on build {}: {why}",
            desktops::build()
        ),
    };

    for (at, desktop) in all.iter().enumerate() {
        let mark = if *desktop == here {
            "  <- on screen"
        } else {
            ""
        };
        println!("desktop {} is {desktop}{mark}", at + 1);
    }

    assert!(!all.is_empty(), "a machine always has at least one desktop");
    assert_eq!(
        desktops::number_of(here, &all),
        Some(all.iter().position(|d| *d == here).unwrap() + 1),
        "the desktop on screen is not in the ordered list"
    );

    if all.len() == 1 {
        println!(
            "NOTE: one desktop. The numbering below is exercised, but nothing here has \
             seen a window on a desktop other than this one. Run this again on a machine \
             with two."
        );
    }
}

/// What the window list makes of windows that are not on this desktop.
///
/// Prints them rather than asserting a count, because how many there are is a
/// fact about whoever is running this. What is asserted is the part that can
/// be wrong: every window the list marks as elsewhere really is on another
/// desktop, no window on this one is marked, and a marked window never carries
/// a number when there is no ordered list to have got one from.
#[test]
#[ignore]
#[cfg(windows)]
fn the_window_list_marks_only_the_windows_that_are_really_elsewhere() {
    let windows = crate::windowing::list();
    let numbered = desktops::desktops().is_ok();
    println!(
        "{} windows listed, ordered list available: {numbered}",
        windows.len()
    );

    for window in &windows {
        if window.elsewhere {
            println!(
                "elsewhere: [{}] {} -> {}",
                window.app,
                window.title,
                desktops::label(window.desktop)
            );
        }
    }

    for window in &windows {
        let on = desktops::on_current(window.id);

        if window.elsewhere {
            assert_eq!(
                on,
                Some(false),
                "[{}] {} is marked elsewhere and Windows says it is here",
                window.app,
                window.title
            );
        } else {
            assert_ne!(
                on,
                Some(false),
                "[{}] {} is on another desktop and was not marked",
                window.app,
                window.title
            );
            assert_eq!(
                window.desktop, None,
                "[{}] {} is on this desktop and carries a desktop number",
                window.app, window.title
            );
        }

        if window.desktop.is_some() {
            assert!(
                numbered,
                "a window was numbered with no ordered list to number it from"
            );
        }
    }
}

/**
The finding that decided the shape of this whole feature.

**`IVirtualDesktopManager::MoveWindowToDesktop` refuses a window belonging to
another process**, which is every window a launcher deals with. That is why
Sill can find a window on another desktop and cannot send one there: doing it
needs the undocumented `MoveViewToDesktop`, a mutating call into a vtable
nothing here has watched work.

This test asserts the refusal. It is not testing Sill; it is holding a fact
about Windows that a design decision rests on. **The day it fails, moving a
window between desktops becomes something Sill can offer without an
undocumented mutating call**, and whoever sees it fail should go and do that.

Character Map, never one of somebody's own windows: a plain Win32 program with
one top-level window that starts and closes in under a second.
*/
#[test]
#[ignore]
#[cfg(windows)]
fn the_documented_interface_still_will_not_move_another_process_window() {
    let mut child = std::process::Command::new(r"C:\Windows\System32\charmap.exe")
        .spawn()
        .expect("Character Map would not start");

    let pid = child.id();
    let found = (0..40).find_map(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        crate::windowing::list()
            .into_iter()
            .find(|window| window.pid == pid)
            .map(|window| window.id)
    });

    let refusal = found.map(|id| {
        let started = desktops::of_window(id).expect("Windows would not place the new window");
        // To the desktop it is already on, so a Windows that allowed this
        // would move nothing. The access check is the same either way, and it
        // is the only thing being asked about.
        (started, desktops::send(id, started))
    });

    let _ = child.kill();
    let _ = child.wait();

    let id = found.expect("Character Map opened no window this test could find");
    let (started, outcome) = refusal.expect("no outcome");

    println!("window {id:#x} started on {started}, opened and closed by this test");
    println!("MoveWindowToDesktop said: {outcome:?}");

    let why = outcome.expect_err(
        "MoveWindowToDesktop moved another process's window. It could not before, and if it \
         can now then Sill can offer to move windows between desktops using nothing \
         undocumented. Go and read the module header in desktops.rs.",
    );

    assert!(
        why.contains("Access is denied"),
        "refused for a reason nothing here predicted: {why}"
    );
}

/// What asking costs, which is what decides whether it may run per keystroke.
///
/// The apartment has to be warm before any of this means anything: the first
/// call on a thread pays for standing one up, and `desktops::with_com` exists
/// because that cost used to be paid on every call.
#[test]
#[ignore]
#[cfg(windows)]
fn reports_what_a_window_list_costs_now() {
    for _ in 0..5 {
        let _ = crate::windowing::list();
        let _ = desktops::here();
    }

    let mut listing = Vec::new();
    let mut counted = 0;
    for _ in 0..10 {
        let at = std::time::Instant::now();
        let windows = crate::windowing::list();
        counted = windows.len();
        listing.push(at.elapsed().as_micros());
    }

    let mut asking = Vec::new();
    for _ in 0..10 {
        let at = std::time::Instant::now();
        let _ = desktops::here();
        asking.push(at.elapsed().as_micros());
    }

    let mut nothing = Vec::new();
    for _ in 0..10 {
        let at = std::time::Instant::now();
        let _ = desktops::elsewhere(&[]);
        nothing.push(at.elapsed().as_micros());
    }

    println!("list of {counted} windows, us: {listing:?}");
    println!("one documented question, us: {asking:?}");
    println!("no cloaked windows at all, us: {nothing:?}");

    assert!(
        nothing.iter().all(|each| *each < 100),
        "a machine with nothing cloaked must pay nothing: {nothing:?}"
    );
}
