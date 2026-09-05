//! Named groups of history entries.
//!
//! Was an integration test "for the same reason the others are", which turned
//! out not to apply here: nothing in this file reaches the dialog plugin, so
//! the manifest `build.rs` gives `tests/` targets is not needed and the
//! library's own test binary runs these perfectly well. See `suite/mod.rs`.

use crate::clipboard::kind::Kind;
use crate::clipboard::store::{Recording, Store};

const NOW: i64 = 1_700_000_000;

fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temp directory");
    let store = Store::open(&dir.path().join("clipboard.db")).expect("opens");
    (store, dir)
}

fn add(store: &Store, text: &str) -> i64 {
    store
        .record(Recording {
            hash: text,
            kind: Kind::Text,
            text,
            html: None,
            app: None,
            app_path: None,
            bytes: text.len() as i64,
            now: NOW,
        })
        .expect("records")
}

#[test]
fn a_collection_keeps_the_order_things_were_put_in() {
    // Not newest first. A collection is something somebody arranged, and
    // re-sorting it by when things were copied throws that away.
    let (store, _dir) = store();
    let one = add(&store, "first");
    let two = add(&store, "second");
    let three = add(&store, "third");

    let id = store
        .create_collection("Release notes", NOW)
        .expect("creates");
    store
        .add_to_collection(id, &[three, one, two])
        .expect("adds");

    let texts: Vec<String> = store
        .collection_entries(id)
        .expect("reads")
        .into_iter()
        .map(|e| e.text)
        .collect();

    assert_eq!(texts, vec!["third", "first", "second"]);
}

#[test]
fn asking_for_the_same_name_twice_is_the_same_collection() {
    // The name is how a person refers to it. A second one with the same name
    // would shadow the first and there would be no way to tell them apart.
    let (store, _dir) = store();

    let first = store.create_collection("Snippets", NOW).expect("creates");
    let again = store
        .create_collection("snippets", NOW + 5)
        .expect("creates");

    assert_eq!(first, again, "case does not make it a different collection");
    assert_eq!(store.collections().expect("lists").len(), 1);
}

#[test]
fn adding_something_already_there_does_not_move_it() {
    // Adding a batch that overlaps what is already in the collection must not
    // reshuffle the part that was arranged.
    let (store, _dir) = store();
    let one = add(&store, "alpha");
    let two = add(&store, "beta");
    let three = add(&store, "gamma");

    let id = store.create_collection("Work", NOW).expect("creates");
    store
        .add_to_collection(id, &[one, two, three])
        .expect("adds");

    // Re-adding the FIRST one. Anything that reassigns its position sends it
    // to the end, which is visible; re-adding a middle or last entry is not,
    // because it lands back roughly where it was and the test would pass
    // either way. That was the first version of this test and it was useless.
    store.add_to_collection(id, &[one]).expect("adds again");

    let texts: Vec<String> = store
        .collection_entries(id)
        .expect("reads")
        .into_iter()
        .map(|e| e.text)
        .collect();

    assert_eq!(
        texts,
        vec!["alpha", "beta", "gamma"],
        "alpha was already first and must not have been sent to the end"
    );
}

#[test]
fn deleting_a_collection_leaves_the_entries_alone() {
    // A collection groups the history; it does not own it. Deleting the group
    // must not delete what somebody copied.
    let (store, _dir) = store();
    let one = add(&store, "kept");

    let id = store.create_collection("Temporary", NOW).expect("creates");
    store.add_to_collection(id, &[one]).expect("adds");
    store.delete_collection(id).expect("deletes");

    assert!(store.collections().expect("lists").is_empty());
    assert!(
        store.get(one).expect("reads").is_some(),
        "the entry itself survived"
    );
}

#[test]
fn an_entry_that_ages_out_leaves_its_collection_cleanly() {
    // Retention prunes entries on its own schedule, so this is ordinary
    // rather than rare. A membership row pointing at nothing would make the
    // collection unreadable.
    let (store, _dir) = store();
    let one = add(&store, "stays");
    let two = add(&store, "pruned");

    let id = store.create_collection("Mixed", NOW).expect("creates");
    store.add_to_collection(id, &[one, two]).expect("adds");

    store.delete(two).expect("deletes the entry");

    let texts: Vec<String> = store
        .collection_entries(id)
        .expect("reads")
        .into_iter()
        .map(|e| e.text)
        .collect();

    assert_eq!(texts, vec!["stays"]);
    assert_eq!(store.collections().expect("lists")[0].count, 1);
}

#[test]
fn a_collection_with_nothing_left_in_it_still_exists() {
    // It was named deliberately and is still somewhere to put things.
    // Disappearing when the last entry ages out would look like data loss.
    let (store, _dir) = store();
    let only = add(&store, "gone soon");

    let id = store.create_collection("Empties", NOW).expect("creates");
    store.add_to_collection(id, &[only]).expect("adds");
    store.delete(only).expect("deletes");

    let listed = store.collections().expect("lists");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].count, 0);
    assert!(store.collection_entries(id).expect("reads").is_empty());
}

#[test]
fn renaming_keeps_the_membership() {
    let (store, _dir) = store();
    let one = add(&store, "thing");

    let id = store.create_collection("Old name", NOW).expect("creates");
    store.add_to_collection(id, &[one]).expect("adds");
    store.rename_collection(id, "New name").expect("renames");

    let listed = store.collections().expect("lists");
    assert_eq!(listed[0].name, "New name");
    assert_eq!(store.collection_entries(id).expect("reads").len(), 1);
}

/// `tag:name` on the clipboard means the collection of that name.
#[test]
fn a_collection_name_answers_a_tag_filter() {
    let (store, _dir) = store();
    let kept = add(&store, "kept");
    add(&store, "loose");

    let work = store.create_collection("Work", NOW).expect("creates");
    store.add_to_collection(work, &[kept]).expect("adds");

    assert_eq!(store.collection_named("work").expect("looks up"), Some(work));
    assert_eq!(store.collection_named("WORK ").expect("looks up"), Some(work));
    assert_eq!(store.collection_named("home").expect("looks up"), None);

    let inside = store.collection_entries(work).expect("lists");
    assert_eq!(inside.len(), 1);
    assert_eq!(inside[0].id, kept);
}
