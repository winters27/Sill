//! Which filesystem changes are worth rebuilding the file index for.
//!
//! Watching is recursive and the walk is not, so most of what a watcher sees
//! comes from directories the index deliberately skips. Measured before any of
//! this existed: an idle machine rebuilt the index eight times in seven
//! minutes, because `AppData` never stops changing.
//!
//! Every test here calls the code the watcher calls. An earlier version of
//! this file re-implemented both judgements, which meant it could pass while
//! the watcher was wrong, and a deliberate break confirmed exactly that.
use crate::catalog::{changes_the_index, worth_indexing, NOISE, SYSTEM};
use std::path::Path;
use std::path::PathBuf;

// ------------------------------------------------------- where it happened

#[test]
fn a_file_somebody_wrote_is_worth_rebuilding_for() {
    for path in [
        r"C:\Users\me\Documents\notes.md",
        r"C:\Users\me\code\thing\src\main.rs",
        r"C:\Users\me\Desktop\photo.png",
    ] {
        assert!(worth_indexing(Path::new(path), &[]), "{path}");
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
        assert!(!worth_indexing(Path::new(path), &[]), "{path}");
    }
}

#[test]
fn a_skipped_directory_anywhere_in_the_path_is_enough() {
    // Judged by every component, not just the last one. A file deep inside a
    // package cache is not interesting however interesting its parent is.
    assert!(!worth_indexing(
        Path::new(r"C:\Users\me\work\important\node_modules\pkg\deep\deeper\file.js"),
        &[]
    ));
    assert!(worth_indexing(
        Path::new(r"C:\Users\me\work\important\deep\file.js"),
        &[]
    ));
}

// --------------------------------------- a folder somebody deliberately added

#[test]
fn a_folder_somebody_added_is_watched_even_inside_one_normally_skipped() {
    // The walk's filter only ever sees entries *below* a root, so adding
    // `%TEMP%\work` indexes everything in it however the path to it is
    // spelled. Checking the whole path made the watcher disagree: every file
    // under that root contains `AppData`, so nothing in a folder somebody had
    // deliberately chosen was ever noticed changing.
    //
    // Found by a device test whose own scratch folder lived in `%TEMP%`, where
    // writes correctly caused no rebuild and creates caused none either.
    let root = PathBuf::from(r"C:\Users\me\AppData\Local\Temp\work");
    let inside = Path::new(r"C:\Users\me\AppData\Local\Temp\work\notes.md");

    assert!(
        !worth_indexing(inside, &[]),
        "the whole path really does look skippable"
    );
    assert!(
        worth_indexing(inside, std::slice::from_ref(&root)),
        "a folder that was deliberately added is not being watched"
    );
}

#[test]
fn what_is_skipped_below_an_added_folder_is_still_skipped() {
    // Judging from the root down must not turn the skip list off. A build
    // directory inside a chosen folder is still a build directory.
    let root = PathBuf::from(r"C:\Users\me\AppData\Local\Temp\work");
    let roots = std::slice::from_ref(&root);

    assert!(!worth_indexing(
        Path::new(r"C:\Users\me\AppData\Local\Temp\work\node_modules\x\a.js"),
        roots
    ));
    assert!(!worth_indexing(
        Path::new(r"C:\Users\me\AppData\Local\Temp\work\target\debug\a.log"),
        roots
    ));
}

#[test]
fn a_path_under_no_root_at_all_is_judged_whole() {
    // Events can arrive for paths outside every root, and the old rule is the
    // right one for those.
    let root = PathBuf::from(r"C:\work");

    assert!(!worth_indexing(
        Path::new(r"C:\Users\me\AppData\Local\thing.tmp"),
        std::slice::from_ref(&root)
    ));
}

#[test]
fn the_watcher_and_the_walk_agree_on_what_to_skip() {
    // Two lists would drift, and the drift would either rebuild for changes
    // that cannot matter or miss changes that do.
    assert!(NOISE.contains(&"node_modules"));
    assert!(NOISE.contains(&"AppData"));
    assert!(NOISE.contains(&".git"));
    assert!(SYSTEM.contains(&"Windows"));
}

// ------------------------------------------------------------- what it was

#[test]
fn saving_a_file_does_not_change_a_list_of_file_names() {
    // This is nearly everything a watcher reports: every save in an editor,
    // every log line, every application touching its own state. The index
    // holds names and where they are, and a write changes neither.
    use notify::event::{AccessKind, DataChange, EventKind, MetadataKind, ModifyKind};

    for kind in [
        EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        EventKind::Modify(ModifyKind::Data(DataChange::Size)),
        EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime)),
        EventKind::Access(AccessKind::Read),
    ] {
        assert!(!changes_the_index(&kind), "{kind:?} would rebuild");
    }
}

#[test]
fn appearing_going_away_and_being_renamed_all_change_it() {
    use notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode};

    for kind in [
        EventKind::Create(CreateKind::File),
        EventKind::Create(CreateKind::Folder),
        EventKind::Remove(RemoveKind::File),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
        EventKind::Modify(ModifyKind::Name(RenameMode::From)),
    ] {
        assert!(changes_the_index(&kind), "{kind:?} would be missed");
    }
}

#[test]
fn a_platform_that_will_not_say_is_taken_at_its_word() {
    // `Any` and `Other` are what a backend reports when it cannot tell. Read
    // as a change, because missing one leaves the index wrong until something
    // else happens, and the rate floor bounds what guessing wrong costs.
    assert!(changes_the_index(&notify::EventKind::Any));
    assert!(changes_the_index(&notify::EventKind::Other));
}

// ------------------------------------------------------------------ drives

#[test]
fn a_drive_root_is_recognised_however_it_is_written() {
    use crate::catalog::same_folder;

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
    use crate::catalog::same_folder;

    assert!(same_folder(r"C:\work\thing", "C:/work/thing"));
    assert!(same_folder(r"C:\work\thing\", r"C:\work\thing"));
    assert!(!same_folder(r"C:\work\thing", r"C:\work\other"));
}

// ------------------------------------------------- how often it may rebuild

#[test]
fn a_costlier_walk_earns_a_longer_rest() {
    use crate::state::quiet_after;
    use std::time::Duration;

    // The wait is the last walk's cost multiplied out, so indexing takes about
    // a twentieth of one processor while files are changing constantly,
    // whether the folder is small or a whole drive. Measured costs: a home
    // folder walks in 1.3 seconds and a whole C: drive in about six.
    assert_eq!(
        quiet_after(Duration::from_millis(1300)),
        Duration::from_secs(26)
    );
    assert_eq!(
        quiet_after(Duration::from_millis(6000)),
        Duration::from_secs(120)
    );

    // Longer walk, longer rest, always.
    let mut previous = Duration::ZERO;
    for ms in [0, 500, 1300, 3000, 6000, 12_000] {
        let rest = quiet_after(Duration::from_millis(ms));
        assert!(rest >= previous, "{ms} ms went backwards");
        previous = rest;
    }
}

#[test]
fn a_very_fast_walk_still_gets_a_rest() {
    use crate::state::quiet_after;
    use std::time::Duration;

    // A tiny folder walks in milliseconds, and multiplying that out would let
    // it rebuild on every keystroke of somebody's editor. The first rebuild
    // has no previous cost to go on at all.
    assert_eq!(quiet_after(Duration::ZERO), Duration::from_secs(20));
    assert_eq!(
        quiet_after(Duration::from_millis(10)),
        Duration::from_secs(20)
    );
}
