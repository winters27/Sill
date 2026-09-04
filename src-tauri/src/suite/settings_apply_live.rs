//! What a save has to go and redo, and that redoing it changes the index.
//!
//! The Sources panel used to say "turning a source off removes its entries
//! from the next search", and it did not: the switches gate a scan and nothing
//! scanned again until the Rebuild button or a restart. Script folders and the
//! typed file roots were the same. The panel and the machine disagreed, and
//! the panel was the confident one.
//!
//! Two halves are tested here, because either alone proves nothing. That the
//! comparison notices the settings the index has to be told about, and that
//! telling it produces a different index. A test of the first alone would pass
//! with a `reload_index` that did nothing.

use std::fs;

use crate::preferences::{Preferences, Redo};

/// A settings change that costs nothing is not made to cost a scan.
#[test]
fn a_change_the_index_does_not_care_about_redoes_nothing() {
    let before = Preferences::default();
    let mut after = before.clone();

    after.appearance.visible_rows = 12;
    after.hotkey.summon = "Ctrl+Space".to_string();
    // Read on every query rather than at scan time, so a word added here is in
    // effect on the next keystroke and a rescan would be a minute of work to
    // change nothing.
    after.sources.excluded.push("vendor".to_string());
    after.scripts.timeout_seconds = 120;

    assert_eq!(Redo::between(&before, &after), Redo::default());
    assert!(Redo::between(&before, &after).is_empty());
}

#[test]
fn a_source_switch_asks_for_a_scan() {
    let before = Preferences::default();
    let mut after = before.clone();
    after.sources.path_executables = !before.sources.path_executables;

    let redo = Redo::between(&before, &after);
    assert!(
        redo.sources,
        "the switches gate the scan, so the scan runs again"
    );
    assert!(!redo.scripts);
    assert_eq!(redo.file_roots, None);
}

/// A named folder is somewhere that has to be walked before anything in it
/// can be found, so adding one is a scan and not a filter.
///
/// The distinction is the reason it sits with the source switches rather than
/// beside `excluded`: a word added to `excluded` takes effect on the next
/// keystroke, and a folder added here does nothing at all until a scan reads
/// it. Leaving it out of the comparison would leave the panel saying the
/// folder was added and the index saying nothing was there.
#[test]
fn a_folder_of_your_own_asks_for_a_scan() {
    let before = Preferences::default();
    let mut after = before.clone();
    after.sources.folders.push(r"D:\Portable".to_string());

    let redo = Redo::between(&before, &after);
    assert!(
        redo.sources,
        "a folder nobody has walked yet holds nothing anybody can find"
    );

    // And an exclusion still does not, because it is read on every query.
    let mut filtered = before.clone();
    filtered.sources.excluded.push("vendor".to_string());
    assert!(!Redo::between(&before, &filtered).sources);
}

/// Games are a source like any other, so switching them off is a scan.
#[test]
fn switching_games_off_asks_for_a_scan() {
    let before = Preferences::default();
    let mut after = before.clone();
    after.sources.games = false;

    assert!(Redo::between(&before, &after).sources);
}

#[test]
fn a_script_folder_asks_for_a_walk() {
    let before = Preferences::default();
    let mut after = before.clone();
    after.scripts.folders.push("C:/scripts".to_string());

    let redo = Redo::between(&before, &after);
    assert!(redo.scripts);
    assert!(!redo.sources);

    // Switching the whole feature off is the same question: off means the
    // folders are not walked, which the index has to be told about too.
    let mut off = after.clone();
    off.scripts.enabled = false;
    assert!(Redo::between(&after, &off).scripts);
}

#[test]
fn a_typed_root_asks_for_a_rebuild_and_says_which_folders() {
    let mut before = Preferences::default();
    before.files.index = true;
    before.files.roots = vec!["C:/one".to_string()];

    let mut after = before.clone();
    after.files.roots = vec!["C:/one".to_string(), "C:/two".to_string()];

    let redo = Redo::between(&before, &after);
    let roots = redo
        .file_roots
        .expect("the new folders travel with the answer");

    assert_eq!(roots, after.files.indexed_roots());
    assert_eq!(roots.len(), 2);
    assert!(!redo.sources);
}

/// The folders survive being written to disk and read back, and are still seen
/// as a change once they have.
///
/// Round-tripped rather than compared in memory, because that is the journey
/// the setting actually makes: the window sends it, `save` writes it, and the
/// next start reads it. A field that does not survive that would leave the
/// index rebuilt once and wrong forever after.
#[test]
fn typed_roots_survive_the_disk_and_are_still_a_change() {
    let dir = std::env::temp_dir().join(format!("sill-live-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("a temp folder");
    let path = dir.join("preferences.json");

    let mut wanted = Preferences::default();
    wanted.files.index = true;
    wanted.files.roots = vec![dir.join("notes").to_string_lossy().to_string()];
    wanted.scripts.folders = vec![dir.join("scripts").to_string_lossy().to_string()];
    wanted.sources.path_executables = !Preferences::default().sources.path_executables;

    wanted.save(&path).expect("preferences are written");

    // The object in hand is dropped before anything is read back, so what the
    // assertions see came off the disk and not out of this function.
    drop(wanted);

    let read = Preferences::load(&path);

    assert_eq!(read.files.roots.len(), 1);
    assert_eq!(read.scripts.folders.len(), 1);

    let redo = Redo::between(&Preferences::default(), &read);
    assert!(redo.sources);
    assert!(redo.scripts);
    assert!(redo.file_roots.is_some());

    let _ = fs::remove_dir_all(&dir);
}

/// Turning script commands off, and back on, changes what the index holds.
///
/// This is the scan `reload_scripts` runs, called the same way, so a change
/// that the comparison above asks for is shown to produce a different list of
/// commands rather than only a different flag.
#[test]
fn walking_the_script_folders_again_changes_the_commands() {
    let dir = std::env::temp_dir().join(format!("sill-scripts-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("a temp folder");

    let script = dir.join("greet.ps1");
    fs::write(
        &script,
        "# @raycast.schemaVersion 1\n\
         # @raycast.title Greet\n\
         # @raycast.mode silent\n\
         Write-Output 'hello'\n",
    )
    .expect("a script on disk");

    let found: Vec<_> = crate::scripts::scan(&[dir.clone()])
        .iter()
        .map(crate::registry::script_record)
        .collect();

    assert_eq!(found.len(), 1, "the folder holds one script command");
    assert_eq!(found[0].title, "Greet");

    // Off is no folders at all, which is what `reload_scripts` passes, and the
    // index it produces is empty rather than filtered afterwards.
    let none: Vec<_> = crate::scripts::scan(&[])
        .iter()
        .map(crate::registry::script_record)
        .collect();

    assert!(
        none.is_empty(),
        "off scans nothing rather than hiding results"
    );

    let _ = fs::remove_dir_all(&dir);
}
