//! Against the browsers actually running on this machine.
//!
//! The fixtures in `uia.rs` decide which tab a row means once the strip has
//! been read. They cannot say whether a real Chromium or a real Firefox
//! exposes a strip at all, how deep it sits, what a tab is called there, or
//! what any of it costs, and all four of those are decided by somebody else's
//! release notes rather than by this codebase.
//!
//! Ignored, because a build agent has no browser open:
//!
//! ```text
//! cargo test --lib real_tabs -- --ignored --nocapture
//! ```

#[cfg(windows)]
use crate::browsers::Family;

/// Which families this run is allowed to touch.
///
/// An environment variable rather than a constant, and the only one of these
/// probes that takes an argument, because reading a Firefox window is not
/// only a read: it switches that browser's accessibility engine on. A probe
/// that cannot be pointed at one family cannot measure what the other one
/// costs, because the first run is the only cold one there will be until
/// somebody restarts their browser.
///
/// ```text
/// SILL_TABS=chromium cargo test --lib real_tabs -- --ignored --nocapture
/// ```
#[cfg(windows)]
fn wanted() -> Vec<Family> {
    match std::env::var("SILL_TABS").unwrap_or_default().as_str() {
        "chromium" => vec![Family::Chromium],
        "firefox" => vec![Family::Firefox],
        _ => vec![Family::Chromium, Family::Firefox],
    }
}

/// One window rather than all of them, when `SILL_TABS_WINDOW` names one.
///
/// A handle in hexadecimal. It exists so a probe that switches tabs can be
/// pointed at a window the person running it opened, rather than at the one
/// they are reading.
#[cfg(windows)]
fn only_this_window(open: &mut Vec<crate::uia::Open>) {
    let Ok(only) = std::env::var("SILL_TABS_WINDOW") else {
        return;
    };

    let only = isize::from_str_radix(only.trim_start_matches("0x"), 16).expect("a window handle");
    open.retain(|one| one.window == only);
}

/// Prints the tree under every browser window, shallowly.
///
/// The one thing about this feature that cannot be reasoned about from inside
/// the codebase: where in somebody else's window the tab strip is, and what
/// the elements in it are called.
#[test]
#[ignore]
#[cfg(windows)]
fn the_shape_of_a_real_browser_window() {
    let windows = crate::windowing::list();
    let mut open = crate::uia::browser_windows(&windows, &wanted());
    only_this_window(&mut open);

    if open.is_empty() {
        println!("no browser is running here. every window seen:");
        for window in &windows {
            println!("  {:<40} {}", window.app_path, window.title);
        }
        return;
    }

    for one in &open {
        println!("\n=== {} window {:#x} ===", one.browser, one.window);
        print!("{}", crate::uia::dump(one.window, 12));
    }
}

/// Reads the open tabs, and says what it cost.
#[test]
#[ignore]
#[cfg(windows)]
fn the_tabs_open_here_read() {
    let listing = std::time::Instant::now();
    let windows = crate::windowing::list();
    let listed = listing.elapsed();

    let mut open = crate::uia::browser_windows(&windows, &wanted());
    only_this_window(&mut open);

    if open.is_empty() {
        println!("no browser is running here");
        return;
    }

    println!(
        "browser windows: {:?}",
        open.iter()
            .map(|one| (&one.browser, format!("{:#x}", one.window)))
            .collect::<Vec<_>>()
    );

    let reading = std::time::Instant::now();
    let tabs = crate::uia::read(&open);
    let read = reading.elapsed();

    for tab in &tabs {
        println!(
            "  {:>3} {:<7} {}{}",
            tab.index,
            tab.browser,
            if tab.active { "* " } else { "  " },
            tab.title
        );
    }

    println!(
        "\n{} windows, {} tabs. window list {:?}, tab read {:?}",
        open.len(),
        tabs.len(),
        listed,
        read
    );

    // The claim: a running browser yields tabs. Not asserted for a machine
    // with none open, which is the line above.
    assert!(
        !tabs.is_empty(),
        "{} browser windows are open and not one tab came back, which is what \
         happens when the strip is deeper than the walk goes or a browser \
         renamed its control types",
        open.len()
    );
}

/// A second read after the first, which is what a second keystroke costs.
///
/// Separated from the first because the two numbers are different and the
/// difference is the point: the first read pays for whatever the browser has
/// to switch on, and every read after it does not.
#[test]
#[ignore]
#[cfg(windows)]
fn a_second_read_is_cheaper_than_the_first() {
    let windows = crate::windowing::list();
    let mut open = crate::uia::browser_windows(&windows, &wanted());
    only_this_window(&mut open);

    if open.is_empty() {
        println!("no browser is running here");
        return;
    }

    for round in 0..5 {
        let at = std::time::Instant::now();
        let tabs = crate::uia::read(&open);
        println!("read {round}: {} tabs in {:?}", tabs.len(), at.elapsed());
    }

    println!("\nwhere a read spends itself:\n{}", crate::uia::cost(&open));
}

/// Switches to every tab of a window in turn, and checks it worked.
///
/// The one claim in this feature that a fixture cannot make: that the row does
/// what it says. Reading a strip and choosing a row out of it are testable
/// over values; **whether the tab somebody pressed Enter on is the tab that
/// comes to the front** is a question about two other programs.
///
/// Every tab, not one, because activating the tab that is already in front
/// proves nothing and is the case a broken implementation passes.
///
/// It puts the tab that was in front back before it finishes, so it can be run
/// against a browser somebody is using. `SILL_TABS_WINDOW`, a window handle in
/// hexadecimal, narrows it to one window if that is not good enough.
///
/// ```text
/// SILL_TABS_WINDOW=0xf70ae8 cargo test --lib real_tabs::switching -- --ignored --nocapture
/// ```
#[test]
#[ignore]
#[cfg(windows)]
fn switching_to_a_tab_brings_that_tab_to_the_front() {
    let windows = crate::windowing::list();
    let mut open = crate::uia::browser_windows(&windows, &wanted());
    only_this_window(&mut open);

    if open.is_empty() {
        println!("no browser is running here");
        return;
    }

    for one in &open {
        let tabs = crate::uia::read(std::slice::from_ref(one));

        let Some(was) = tabs.iter().find(|tab| tab.active).or(tabs.first()).cloned() else {
            println!("{} window {:#x} has no tabs", one.browser, one.window);
            continue;
        };

        for tab in &tabs {
            let at = std::time::Instant::now();
            let switched = crate::uia::activate(&tab.located());
            let took = at.elapsed();

            assert!(
                switched.is_ok(),
                "{} would not switch to {:?}: {switched:?}",
                one.browser,
                tab.title
            );

            let now = crate::uia::read(std::slice::from_ref(one));
            let front = now.iter().find(|other| other.active);

            assert_eq!(
                front.map(|other| other.key.as_str()),
                Some(tab.key.as_str()),
                "{} switched to {:?} and {:?} is the tab in front",
                one.browser,
                tab.title,
                front.map(|other| &other.title)
            );

            println!(
                "{} {:?} came to the front in {took:?}",
                one.browser, tab.title
            );
        }

        /*
         * The same again, with the recorded position deliberately wrong.
         *
         * Without this the probe proves less than it looks like it does. Every
         * row above was activated against a strip that had not moved since it
         * was read, so an implementation that ignored the browser's own
         * identifier and simply went to the position it remembered would pass
         * every one of them. It was sabotaged exactly that way and it did.
         *
         * A person's tabs move between the keystroke and the Enter. This is
         * that, made to happen: each tab is asked for at somebody else's
         * position, and the identifier is all that is left to go on.
         */
        for tab in &tabs {
            let mut misremembered = tab.located();
            misremembered.index = (tab.index + 1) % tabs.len().max(1);

            crate::uia::activate(&misremembered).unwrap_or_else(|err| {
                panic!("{} would not switch to {:?}: {err}", one.browser, tab.title)
            });

            let now = crate::uia::read(std::slice::from_ref(one));
            let front = now.iter().find(|other| other.active);

            assert_eq!(
                front.map(|other| other.key.as_str()),
                Some(tab.key.as_str()),
                "{} was asked for {:?} at the wrong position and brought {:?} \
                 to the front, so the position is deciding and not the \
                 browser's own name for the tab",
                one.browser,
                tab.title,
                front.map(|other| &other.title)
            );
        }

        println!(
            "{} switched to all {} of them again from the wrong position",
            one.browser,
            tabs.len()
        );

        // Back where it was, so this can be run against a browser in use.
        let _ = crate::uia::activate(&was.located());
    }
}

/// The whole path a keystroke takes, against a real browser.
///
/// The probes above prove each half. This is the join: read the strip, rank it
/// against what somebody typed, build the row the window would draw, and then
/// take that row's entrypoint back apart and switch to it, which is what
/// pressing Enter on it does.
///
/// It exists because the entrypoint is the one place where a value leaves Rust
/// and comes back. Everything either side of that is checked by fixtures; the
/// crossing itself is only ever checked here and in the fixture that pairs the
/// writer with the reader.
///
/// > [!WARNING]
/// > **"The tab in front" is a question with one answer per window.** This
/// > read every open browser window into one list and then asked which tab in
/// > that list was active, which is the first window's answer whatever window
/// > was just switched in. With one browser open it passes; with two it fails
/// > against a working implementation, which is what happened when this was
/// > run against two Edge windows. Every read-back below names the window the
/// > tab belongs to.
#[test]
#[ignore]
#[cfg(windows)]
fn a_row_built_the_way_the_window_builds_one_still_switches() {
    let windows = crate::windowing::list();
    let mut open = crate::uia::browser_windows(&windows, &wanted());
    only_this_window(&mut open);

    if open.is_empty() {
        println!("no browser is running here");
        return;
    }

    let tabs = crate::uia::read(&open);

    // One per window, because each window has a tab in front of its own and
    // putting "the" one back would leave every other window switched.
    let was: Vec<crate::uia::Tab> = open
        .iter()
        .filter_map(|one| {
            tabs.iter()
                .find(|tab| tab.window == one.window && tab.active)
                .cloned()
        })
        .collect();

    if was.is_empty() {
        println!("nothing is in front, so there is nothing to put back");
        return;
    }

    // The query every tab matches, so the ranking is exercised without this
    // probe needing to know what anybody has open.
    let ranked = crate::uia::rank(tabs, "", 20);
    assert!(
        !ranked.is_empty(),
        "the read found tabs and ranking lost them"
    );

    for tab in ranked {
        let title = tab.title.clone();
        let key = tab.key.clone();
        let window = tab.window;
        let row = crate::commands::search::TabRow::from(tab);

        let want = crate::uia::Where::parse(row.entrypoint())
            .unwrap_or_else(|| panic!("the row for {title:?} does not parse back"));

        crate::uia::activate(&want)
            .unwrap_or_else(|err| panic!("the row for {title:?} would not switch: {err}"));

        // This tab's own window and no other. See the warning above.
        let mine: Vec<crate::uia::Open> = open
            .iter()
            .filter(|one| one.window == window)
            .cloned()
            .collect();

        let now = crate::uia::read(&mine);
        let front = now.iter().find(|other| other.active);

        assert_eq!(
            front.map(|other| other.key.as_str()),
            Some(key.as_str()),
            "the row for {title:?} brought {:?} to the front of window {window:#x}",
            front.map(|other| &other.title)
        );

        println!("the row for {title:?} switched to it");
    }

    for tab in &was {
        let _ = crate::uia::activate(&tab.located());
    }
}

/// What reading a Firefox window costs inside Firefox.
///
/// Firefox keeps its accessibility engine off until a client asks for it, and
/// `ElementFromHandle` is the asking. This prints the working set of every
/// process of that browser before the read, straight after it, and a few
/// seconds later, which is the only honest way to say what the feature costs
/// somebody whose daily browser is a Firefox.
///
/// Run it against a browser that has not been read yet in its current run, or
/// the before and after are the same number for the boring reason.
///
/// **`SILL_TABS=chromium` stops it**, and it has to. This is the one probe here
/// that names a family rather than reading [`wanted`], which is right for a
/// test about Firefox and wrong the moment somebody runs the whole file:
/// `cargo test --lib real_tabs` with the variable set to `chromium` reached a
/// Firefox anyway, which is exactly the thing the variable exists to prevent.
#[test]
#[ignore]
#[cfg(windows)]
fn what_reading_a_firefox_costs_that_firefox() {
    if wanted() == [Family::Chromium] {
        println!("SILL_TABS says Chromium only, and reading a Firefox is not free");
        return;
    }

    let windows = crate::windowing::list();
    let mut open = crate::uia::browser_windows(&windows, &[Family::Firefox]);
    only_this_window(&mut open);

    if open.is_empty() {
        println!("no Firefox-family browser is running here");
        return;
    }

    // The process that draws the window, which is where a Firefox keeps its
    // accessibility engine. Reported beside the whole browser, because the
    // whole browser's number wanders by tens of megabytes on its own and would
    // hide anything this feature does.
    let pid = windows
        .iter()
        .find(|window| window.id == open[0].window)
        .map(|window| window.pid)
        .unwrap_or_default();

    let exe = open[0]
        .program
        .clone()
        .unwrap_or_default()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_string();

    println!("before: {}", working_set(&exe, pid));

    let at = std::time::Instant::now();
    let tabs = crate::uia::read(&open);
    println!("read {} tabs in {:?}", tabs.len(), at.elapsed());

    println!("after:  {}", working_set(&exe, pid));
    std::thread::sleep(std::time::Duration::from_secs(10));
    println!("+10s:   {}", working_set(&exe, pid));

    let at = std::time::Instant::now();
    let tabs = crate::uia::read(&open);
    println!("second read {} tabs in {:?}", tabs.len(), at.elapsed());
    println!("+read:  {}", working_set(&exe, pid));
}

/// What one process and one whole browser weigh right now.
///
/// Through PowerShell rather than through a process walk of our own, for the
/// same reason the Apps folder is enumerated that way: this is a diagnostic
/// print, and a diagnostic is not worth a second implementation of process
/// enumeration in this codebase.
#[cfg(windows)]
fn working_set(exe: &str, pid: u32) -> String {
    let name = exe.trim_end_matches(".exe");

    let Ok(out) = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "$all = Get-Process -Name '{name}' -ErrorAction SilentlyContinue; \
                 $one = Get-Process -Id {pid} -ErrorAction SilentlyContinue; \
                 'window process {{0:N1}} MB, whole browser {{1}} processes {{2:N0}} MB' -f \
                 ($one.WorkingSet64 / 1MB), @($all).Count, \
                 (($all | Measure-Object WorkingSet64 -Sum).Sum / 1MB)"
            ),
        ])
        .output()
    else {
        return "could not ask".to_string();
    };

    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
