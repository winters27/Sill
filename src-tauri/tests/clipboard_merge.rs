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
