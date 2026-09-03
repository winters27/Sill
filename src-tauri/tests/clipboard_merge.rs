//! Merging several history entries into one.
//!
//! An integration test rather than a unit test for the same reason the action
//! registry's are: a lib unit-test binary that retains these code paths also
//! retains the dialog plugin, which needs a manifest only test targets can be
//! given. See `build.rs`.

use sill_lib::clipboard::kind::Kind;
use sill_lib::clipboard::store::{Recording, Store};

fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temp directory");
    let store = Store::open(&dir.path().join("clipboard.db")).expect("opens");
    (store, dir)
}

fn add(store: &Store, text: &str, at: i64) -> i64 {
    store
        .record(Recording {
            hash: &format!("hash-{text}"),
            kind: Kind::Text,
            text,
            html: None,
            app: Some("Test"),
            app_path: None,
            bytes: text.len() as i64,
            now: at,
        })
        .expect("records");

    store
        .search("", None, 50)
        .expect("searches")
        .into_iter()
        .find(|entry| entry.text == text)
        .expect("is there")
        .id
}

#[test]
fn entries_join_in_the_order_they_were_picked() {
    // Not in the order they are listed. The list is newest first, so merging
    // by list order would assemble everything backwards from how it was
    // chosen, which is the one thing a person would notice immediately.
    let (store, _dir) = store();

    let first = add(&store, "alpha", 100);
    let second = add(&store, "beta", 200);
    let third = add(&store, "gamma", 300);

    // Picked oldest first, which is the reverse of how they are listed.
    assert_eq!(
        store.merge(&[first, second, third], "|").expect("merges"),
        Some("alpha|beta|gamma".to_string())
    );

    // And a different pick order really does produce a different result.
    assert_eq!(
        store.merge(&[third, first, second], "|").expect("merges"),
        Some("gamma|alpha|beta".to_string())
    );
}

#[test]
fn an_entry_deleted_between_picking_and_merging_is_skipped() {
    // Losing the rest of somebody's selection because one row went away would
    // be worse than merging what is left.
    let (store, _dir) = store();

    let first = add(&store, "kept one", 100);
    let gone = add(&store, "deleted", 200);
    let last = add(&store, "kept two", 300);

    store.delete(gone).expect("deletes");

    assert_eq!(
        store.merge(&[first, gone, last], "|").expect("merges"),
        Some("kept one|kept two".to_string())
    );
}

#[test]
fn merging_nothing_that_still_exists_is_not_an_empty_string() {
    // An empty result and "every entry you picked is gone" are different
    // answers, and the caller has to be able to tell them apart rather than
    // quietly putting an empty clipboard in front of somebody.
    let (store, _dir) = store();

    let only = add(&store, "gone too", 100);
    store.delete(only).expect("deletes");

    assert_eq!(store.merge(&[only], "|").expect("merges"), None);
    assert_eq!(store.merge(&[], "|").expect("merges"), None);
}

// ------------------------------------------------------ the log beside it

#[test]
fn the_log_does_not_outgrow_the_history_it_describes() {
    // Measured on a real machine before this was bounded: a 557 KB history
    // with a 3.46 MB log beside it. SQLite waits for a thousand pages, about
    // four megabytes, and a clipboard writes a few kilobytes at a time, so
    // nothing ever wrote enough at once to reach the threshold.
    //
    /*
     * A directory of its own, like `store` above, rather than one named after
     * the test.
     *
     * These two wanted the path as well as the store, and reached for
     * `temp_dir().join("sill-wal-growth")` to get it. One name means one
     * directory for every run on the machine, so a second `cargo test`, which
     * this repository invites by keeping worktrees under `.claude/worktrees`,
     * emptied this one part way through and the assertion failed on a number
     * neither run produced. It passed alone every time, which is the worst
     * shape a flake can have.
     *
     * `TempDir` names itself and removes itself when it goes out of scope, so
     * the wipe at the top and the tidy-up at the bottom are gone with it.
     */
    let dir = tempfile::tempdir().expect("a temp directory");
    let path = dir.path().join("clipboard.db");

    {
        let store = sill_lib::clipboard::store::Store::open(&path).unwrap();

        // Enough copies to have blown past the default threshold.
        for n in 0..1_500 {
            store
                .record(Recording {
                    hash: &format!("hash-{n}"),
                    kind: Kind::Text,
                    text: &format!("entry number {n}, with enough text to matter"),
                    html: None,
                    app: Some("Test"),
                    app_path: None,
                    bytes: 48,
                    now: 1_756_000_000 + n,
                })
                .unwrap();
        }

        let log = std::fs::metadata(dir.path().join("clipboard.db-wal"))
            .map(|m| m.len())
            .unwrap_or(0);
        let history = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        // Twice the setting, not once. A checkpoint runs when the log has
        // already passed the threshold rather than as it reaches it, and one
        // more write lands while that is happening. The number that matters is
        // that this is a ceiling at all: without the setting the log grows
        // with the number of entries and never comes back.
        assert!(
            log <= 2 * 1_048_576,
            "log grew to {log} bytes against a {history} byte history"
        );
    }
}

#[test]
fn opening_hands_back_a_log_that_already_grew() {
    // The setting bounds what happens next. Nothing else would ever shrink a
    // log that grew before it existed, and on a real machine that was three
    // and a half megabytes sitting there.
    let dir = tempfile::tempdir().expect("a temp directory");
    let path = dir.path().join("clipboard.db");

    {
        let store = sill_lib::clipboard::store::Store::open(&path).unwrap();
        for n in 0..300 {
            store
                .record(Recording {
                    hash: &format!("hash-{n}"),
                    kind: Kind::Text,
                    text: &format!("entry {n}"),
                    html: None,
                    app: Some("Test"),
                    app_path: None,
                    bytes: 8,
                    now: 1_756_000_000 + n,
                })
                .unwrap();
        }
    }

    // Reopening runs the checkpoint.
    {
        let _store = sill_lib::clipboard::store::Store::open(&path).unwrap();
    }

    let log = std::fs::metadata(dir.path().join("clipboard.db-wal"))
        .map(|m| m.len())
        .unwrap_or(0);

    assert!(log < 65_536, "log kept {log} bytes after a checkpoint");
}
