//! Against the File Explorer window actually in front on this machine.
//!
//! The fixtures in `explorer.rs` decide which window a key means and what a
//! selected item turns into once the shell has answered. They cannot say
//! whether a real Explorer answers at all, whether `IShellWindows` reports the
//! handle `GetForegroundWindow` reports, what a zip's attributes really are, or
//! what any of it costs. All four are decided by Windows rather than by this
//! codebase.
//!
//! Ignored, because a build agent has no Explorer window open and no keyboard
//! focus to speak of:
//!
//! ```text
//! cargo test --lib real_explorer -- --ignored --nocapture
//! ```
//!
//! **Both probes read only.** Nothing is launched, nothing is recycled, and no
//! key is sent anywhere: the whole point of `IShellWindows` is that Explorer
//! will say what is highlighted without being poked.
//!
//! Run them like this: open an Explorer window yourself, highlight a few things
//! in it, leave it in front, and start the test from a terminal on another
//! monitor or with a delay. Sill is not running, so nothing else is competing
//! for the foreground.

/// How long to wait before reading, so the terminal can be left behind.
///
/// `SILL_EXPLORER_WAIT=5` gives five seconds to click on the Explorer window
/// after starting the test. Without it the read happens immediately, which is
/// right when the test is started from a second machine or a second monitor
/// and useless when it is started from a terminal on top of the window being
/// measured.
#[cfg(windows)]
fn wait() -> std::time::Duration {
    let seconds: u64 = std::env::var("SILL_EXPLORER_WAIT")
        .ok()
        .and_then(|given| given.trim().parse().ok())
        .unwrap_or(0);

    std::time::Duration::from_secs(seconds)
}

/// What Explorer says is highlighted, and what it becomes.
///
/// The one thing about this feature that cannot be reasoned about from inside
/// the codebase: whether a real shell view answers, and whether the paths and
/// the folder bit come back the way the fixtures assume.
#[test]
#[ignore]
#[cfg(windows)]
fn what_is_selected_in_the_window_in_front() {
    std::thread::sleep(wait());

    let started = std::time::Instant::now();
    let selected = crate::explorer::selection();
    let took = started.elapsed();

    println!("read the selection in {took:?}");
    println!("{} item(s) highlighted", selected.len());

    for item in &selected {
        println!(
            "  {} {}",
            if item.folder { "[folder]" } else { "[file]  " },
            if item.path.is_empty() {
                "(nothing on disk)"
            } else {
                &item.path
            },
        );
    }

    let objects = crate::explorer::objects_from(&selected);
    println!("{} of them can be acted on:", objects.len());

    for object in &objects {
        println!("  {:?} {} -> {}", object.kind, object.title, object.target);
    }

    // Not an assertion about the count: a run with nothing highlighted is a
    // perfectly good run and its output says so. What must hold is that
    // anything offered to the registry has something to act on.
    for object in &objects {
        assert!(!object.target.is_empty(), "{object:?} has no target");
        assert!(!object.title.is_empty(), "{object:?} has no title");
    }
}

/**
Which folder Explorer has open behind whatever is in front.

The read `P8-07` needs, and the one the fixtures cannot check: whether the
Z-order walk finds a real folder window, whether `IShellWindows` reports the
same handle for it, and whether `IPersistFolder2` gives back a path rather than
a shell item with no path at all.

Deliberately **not** a foreground read. The terminal running this test is in
front, exactly as a Save dialog would be, so a run that answers is a run that
proves the difference between this and [`crate::explorer::selection`]. Open an
Explorer window, leave it behind the terminal, and run:

```text
cargo test --lib real_explorer::which_folder -- --ignored --nocapture
```

Measured on this machine with one Explorer window open behind the terminal:

```text
read the folder in 32.6845ms
C:\Sill\.claude\worktrees\agent-aa27b6edf696a3dd3\docs
```

Thirty milliseconds, which is the price of the whole `IShellWindows` walk and
is only ever paid on a press that already found a dialog. The press that finds
nothing costs the Z-order walk and stops.
*/
#[test]
#[ignore]
#[cfg(windows)]
fn which_folder_is_open_behind() {
    std::thread::sleep(wait());

    let started = std::time::Instant::now();
    let folder = crate::explorer::folder_in_front();
    let took = started.elapsed();

    println!("read the folder in {took:?}");

    match &folder {
        Some(path) => println!("{path}"),
        None => println!("(no folder window is open)"),
    }

    // Not an assertion that a window was open: a run with none is a good run
    // and its output says so. What must hold is that an answer is a path.
    if let Some(path) = folder {
        assert!(!path.trim().is_empty(), "an answer with nothing in it");
        assert!(
            std::path::Path::new(&path).is_dir(),
            "{path} is not a folder a dialog could be pointed at"
        );
    }
}

/// What it costs when there is no Explorer in front at all.
///
/// The case that runs on every press of a universal key in a text editor, so
/// it is the one that has to be cheap. It still creates the shell windows
/// object and walks the list; what it does not do is reach a shell view.
#[test]
#[ignore]
#[cfg(windows)]
fn what_the_miss_costs() {
    std::thread::sleep(wait());

    let mut worst = std::time::Duration::ZERO;

    for run in 0..5 {
        let started = std::time::Instant::now();
        let selected = crate::explorer::selection();
        let took = started.elapsed();
        worst = worst.max(took);

        println!("run {run}: {} item(s) in {took:?}", selected.len());
    }

    println!("worst of five: {worst:?}");

    assert!(
        worst < crate::explorer::PATIENCE,
        "a read took longer than the deadline it is given: {worst:?}",
    );
}
