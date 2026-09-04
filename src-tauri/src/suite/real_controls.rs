//! Against a real window, in a real other process, opened by this test.
//!
//! The fixtures in `controls.rs` decide which control a row means once a window
//! has been read, and which control types are offered. They cannot say whether
//! a real program exposes its buttons at all, how deep they sit, whether
//! `Invoke` reaches across a process boundary, or what any of it costs. All
//! four belong to somebody else's program.
//!
//! Ignored, because a build agent has no desktop:
//!
//! ```text
//! cargo test --lib real_controls -- --ignored --nocapture
//! ```
//!
//! **The window belongs to this test.** Character Map is started here and
//! killed here, and nothing goes near a window somebody else had open. The
//! failure mode of getting this wrong is a button pressed in a stranger's
//! program, and a probe that reaches for whatever is in front is that mistake
//! in a different costume.
//!
//! ## What makes a press provable
//!
//! A checkbox. Everything else a press does happens inside the other program
//! and has to be read back out of it somehow, usually out of a document, which
//! is exactly the thing this feature does not do. A toggle state is a property
//! of the control itself: press it, ask it what it is set to, and the answer
//! is not a guess about what happened elsewhere on the screen.
//!
//! Character Map has one, called "Advanced view", and it is put back before the
//! probe finishes.

#![cfg(windows)]

use std::time::{Duration, Instant};

/// How long the window gets to appear before the probe gives up on it.
const APPEARS_WITHIN: Duration = Duration::from_secs(15);

/// The program this probe drives.
///
/// Character Map, which ships with Windows, is a plain Win32 dialog with a
/// handful of named buttons and one checkbox, and does nothing at all until it
/// is told to. It is the same program `P3-16` used to measure whether a window
/// of another process could be moved between desktops, and for the same
/// reason: a probe needs a window it can be careless with.
const PROGRAM: &str = "charmap.exe";

/// A window of our own, once it is on screen.
///
/// By process id rather than by name. There may already be a Character Map
/// open, and this test may not touch it.
fn wait_for_window(child: &std::process::Child) -> isize {
    let waiting = Instant::now();
    let pid = child.id();

    loop {
        if let Some(window) = crate::windowing::list()
            .into_iter()
            .find(|window| window.pid == pid && !window.title.is_empty())
        {
            return window.id;
        }

        assert!(
            waiting.elapsed() <= APPEARS_WITHIN,
            "no window of {PROGRAM} appeared within {APPEARS_WITHIN:?}"
        );

        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Starts one, so it can be killed again.
fn open_it() -> std::process::Child {
    std::process::Command::new(PROGRAM)
        .spawn()
        .expect("Character Map starts")
}

/// The named control of a window, as a spot to press.
fn spot(window: isize, name: &str) -> crate::controls::Spot {
    let controls = crate::controls::read(window).expect("the window answers");

    controls
        .iter()
        .find(|control| control.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no control called {name:?}; the window has {:?}",
                controls.iter().map(|one| &one.name).collect::<Vec<_>>()
            )
        })
        .spotted()
}

/// The whole of the pressing half of `P8-04`, against a window this test opened.
///
/// The output of a passing run:
///
/// ```text
/// Character Map window 0x1f0afc appeared after 1.4290475s
/// 10 controls read in 137.4648ms
///   Button        "Select"                     key 42.6556716
///   Checkbox      "Advanced view"              key 42.10486372
///   Link          "Help"                       key 42.3148548
///   ...
/// read 0: 10 controls in 70.3798ms
/// "Advanced view" was off
/// pressed it in 75.8616ms, and it is on
/// pressed it again, and it is off
/// a spot whose identifier has gone: "Advanced view is not in that window any more"
/// a spot whose name has changed:    "Advanced view (renamed) is not in that window any more"
/// ```
///
/// "Copy" is absent from that list on purpose and it is the enabled filter
/// working: Character Map greys it out until a character has been chosen.
#[test]
#[ignore]
fn a_real_window_gives_up_its_buttons_and_one_of_them_is_pressed() {
    let mut child = open_it();
    let opening = Instant::now();
    let window = wait_for_window(&child);
    println!(
        "Character Map window {window:#x} appeared after {:?}",
        opening.elapsed()
    );

    let reading = Instant::now();
    let controls = crate::controls::read(window).expect("the window answers");
    let read = reading.elapsed();

    println!("{} controls read in {read:?}", controls.len());
    for control in &controls {
        println!(
            "  {:<13} {:<28} key {}",
            control.kind.said(),
            format!("{:?}", control.name),
            control.key
        );
    }

    // Five more, because the first read of any window pays for whatever that
    // program has to switch on and every read after it does not.
    for round in 0..5 {
        let at = Instant::now();
        let again = crate::controls::read(window).expect("the window answers");
        println!(
            "read {round}: {} controls in {:?}",
            again.len(),
            at.elapsed()
        );
    }

    assert!(
        !controls.is_empty(),
        "a window with buttons on it gave up none, which is what happens when \
         the walk stops above them or the control types were renamed"
    );

    // The claim a fixture cannot make: a real program's buttons are named the
    // way somebody reading the screen would expect, and reachable.
    for wanted in ["Help", "Advanced view"] {
        assert!(
            controls.iter().any(|one| one.name == wanted),
            "{wanted:?} is on the screen and not in the list"
        );
    }

    /*
     * And the claim the enabled filter earns.
     *
     * Character Map greys "Copy" out until a character has been chosen, which
     * it has not, because nothing has touched this window. A row for it would
     * be a row that does nothing when somebody presses Enter on it, and there
     * is no way to check that without a program that greys something out.
     */
    assert!(
        !controls.iter().any(|one| one.name == "Copy"),
        "\"Copy\" is greyed out in a Character Map nobody has touched, and it \
         is being offered as something to press"
    );

    // The whole point of the identity rule, against a real provider: every
    // control has a runtime identifier and no two share one.
    let mut keys: Vec<&str> = controls.iter().map(|one| one.key.as_str()).collect();
    keys.sort_unstable();
    let all = keys.len();
    keys.dedup();
    assert_eq!(all, keys.len(), "two controls carry the same identifier");

    let switch = spot(window, "Advanced view");
    let was = crate::controls::switched_on(&switch).expect("a checkbox says what it is set to");
    println!("{:?} was {}", switch.name, if was { "on" } else { "off" });

    let at = Instant::now();
    crate::controls::press(&switch).expect("the checkbox is pressed");
    let pressed = at.elapsed();

    let now = crate::controls::switched_on(&switch).expect("it still says what it is set to");
    assert_eq!(
        now, !was,
        "the checkbox was pressed and did not change, so nothing crossed into \
         that program"
    );
    println!(
        "pressed it in {pressed:?}, and it is {}",
        if now { "on" } else { "off" }
    );

    // Back where it was, and the second press proves the first was not a
    // one-way coincidence.
    crate::controls::press(&switch).expect("the checkbox is pressed again");
    assert_eq!(
        crate::controls::switched_on(&switch),
        Some(was),
        "pressing it twice did not put it back"
    );
    println!(
        "pressed it again, and it is {}",
        if was { "on" } else { "off" }
    );

    /*
     * The refusals, which are the half a fixture proves over values and this
     * proves against a real provider.
     *
     * An identifier that has gone is a control that has gone. A name that has
     * changed is a different control at the same element, which is the reuse
     * this module is stricter than the tab reader about.
     */
    let mut gone = switch.clone();
    gone.key = format!("{}.9999", switch.key);
    let refused = crate::controls::press(&gone).expect_err("a control that has gone was pressed");
    println!("a spot whose identifier has gone: {refused:?}");

    let mut renamed = switch.clone();
    renamed.name = format!("{} (renamed)", switch.name);
    let refused =
        crate::controls::press(&renamed).expect_err("a control that was renamed was pressed");
    println!("a spot whose name has changed:    {refused:?}");

    // A handle that is not a window is refused rather than read, which is what
    // stops a stale row reaching whatever Windows has since put there.
    assert!(crate::controls::read(0).is_err());

    let _ = child.kill();
    let _ = child.wait();
}

/// The same, against a browser window somebody names.
///
/// Two things this cannot learn from Character Map. A browser's furniture is
/// built by the browser rather than by Windows, so how deep it sits and what
/// its parts are called are that vendor's decision; and a **tab** is a control
/// that is chosen rather than pressed, which is the `SelectionItem` half of
/// the three ways to press that nothing else here exercises.
///
/// It takes a window handle in hexadecimal, and takes one deliberately: a
/// browser window is not a thing to reach for by name. Point it at one this
/// session opened.
///
/// ```text
/// SILL_CONTROLS_WINDOW=0x1040d78 cargo test --lib real_controls::a_browser -- --ignored --nocapture
/// ```
///
/// **Run it with that window behind something**, which is the state
/// production reads in: the launcher has the foreground by then. Run with the
/// window in front it passes on a Firefox where production would have found
/// nothing, which is how the `IsOffscreen` filter survived being written. See
/// `controls::ready`.
///
/// Measured here, each window behind: Edge gave 18 controls in 151 ms and Zen
/// 17 in 116, both including their tabs, and choosing a tab as a control put
/// that tab in front on both.
#[test]
#[ignore]
fn a_browsers_own_furniture_is_reachable_and_a_tab_is_chosen() {
    let Ok(named) = std::env::var("SILL_CONTROLS_WINDOW") else {
        println!("SILL_CONTROLS_WINDOW names no window, so there is nothing to read");
        return;
    };

    let window = isize::from_str_radix(named.trim().trim_start_matches("0x"), 16)
        .expect("a window handle in hexadecimal");

    let reading = Instant::now();
    let controls = crate::controls::read(window).expect("the window answers");
    let read = reading.elapsed();

    println!("{} controls read in {read:?}", controls.len());
    for control in &controls {
        println!("  {:<13} {:?}", control.kind.said(), control.name);
    }

    let tabs: Vec<&crate::controls::Control> = controls
        .iter()
        .filter(|one| one.kind == crate::controls::Kind::Tab)
        .collect();

    assert!(
        tabs.len() > 1,
        "one tab or none, so choosing one proves nothing. Open a second."
    );

    // Which tab is in front, read the way the tab feature reads it, so this
    // checks the two halves against each other rather than against itself.
    let open = crate::uia::browser_windows(
        &crate::windowing::list(),
        &[
            crate::browsers::Family::Chromium,
            crate::browsers::Family::Firefox,
        ],
    )
    .into_iter()
    .filter(|one| one.window == window)
    .collect::<Vec<_>>();

    assert!(!open.is_empty(), "{window:#x} is not a browser Sill knows");

    let front = |name: &str| -> bool {
        crate::uia::read(&open)
            .iter()
            .any(|tab| tab.active && tab.title == name)
    };

    let was = crate::uia::read(&open)
        .into_iter()
        .find(|tab| tab.active)
        .expect("something is in front");

    for tab in &tabs {
        if tab.name == was.title {
            continue;
        }

        let at = Instant::now();
        crate::controls::press(&tab.spotted())
            .unwrap_or_else(|err| panic!("{:?} would not be chosen: {err}", tab.name));

        assert!(
            front(&tab.name),
            "{:?} was chosen as a control and is not the tab in front",
            tab.name
        );

        println!("{:?} came to the front in {:?}", tab.name, at.elapsed());
    }

    // Back where it was, so this can be run twice.
    let _ = crate::uia::activate(&was.located());
}
