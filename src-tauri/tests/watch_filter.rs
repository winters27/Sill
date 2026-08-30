//! Which filesystem changes are worth rebuilding the file index for.
//!
//! Watching is recursive and the walk is not, so most of what a watcher sees
//! comes from directories the index deliberately skips. Measured before this
//! existed: an idle machine rebuilt the index eight times in seven minutes.
use std::path::Path;

use sill_lib::catalog::NOISE;

/// The same judgement the watcher makes, over the same list.
fn worth_rebuilding(path: &str) -> bool {
    !Path::new(path).components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| NOISE.contains(&name))
    })
}

#[test]
fn a_file_somebody_wrote_is_worth_rebuilding_for() {
    for path in [
        r"C:\Users\me\Documents\notes.md",
        r"C:\Users\me\code\thing\src\main.rs",
        r"C:\Users\me\Desktop\photo.png",
    ] {
        assert!(worth_rebuilding(path), "{path}");
    }
}

#[test]
fn churn_in_a_directory_the_walk_skips_is_not() {
    // Every one of these changes constantly on a machine nobody is touching,
    // and none of them can change what a search finds, because the walk never
    // went in there.
    for path in [
        r"C:\Users\me\AppData\Local\Google\Chrome\User Data\Default\Cache\f_00a1",
        r"C:\Users\me\code\thing\node_modules\left-pad\index.js",
        r"C:\Users\me\code\thing\target\debug\build.log",
        r"C:\Users\me\code\thing\.git\index.lock",
    ] {
        assert!(!worth_rebuilding(path), "{path}");
    }
}

#[test]
fn a_skipped_directory_anywhere_in_the_path_is_enough() {
    // Judged by every component, not just the last one. A file deep inside a
    // package cache is not interesting however interesting its parent is.
    assert!(!worth_rebuilding(
        r"C:\Users\me\work\important\node_modules\pkg\deep\deeper\file.js"
    ));
    assert!(worth_rebuilding(r"C:\Users\me\work\important\deep\file.js"));
}

#[test]
fn the_watcher_and_the_walk_agree_on_what_to_skip() {
    // Two lists would drift, and the drift would either rebuild for changes
    // that cannot matter or miss changes that do.
    assert!(NOISE.contains(&"node_modules"));
    assert!(NOISE.contains(&"AppData"));
    assert!(NOISE.contains(&".git"));
}

// ------------------------------------------------------------------ drives

#[test]
fn a_drive_root_is_recognised_however_it_is_written() {
    use sill_lib::catalog::same_folder;

    // Somebody may type any of these, and all mean the same disk. Reading them
    // as different folders is how a root ends up in the list twice and the
    // same files get indexed twice.
    for written in [r"C:\", "C:/", "C:", r"c:\", "  C:\\  "] {
        assert!(same_folder(written, r"C:\"), "{written:?}");
    }

    assert!(!same_folder(r"D:\", r"C:\"));
    assert!(!same_folder(r"C:\Users", r"C:\"));
}

#[test]
fn a_folder_is_recognised_however_its_separators_lean() {
    use sill_lib::catalog::same_folder;

    assert!(same_folder(r"C:\work\thing", "C:/work/thing"));
    assert!(same_folder(r"C:\work\thing\", r"C:\work\thing"));
    assert!(!same_folder(r"C:\work\thing", r"C:\work\other"));
}
