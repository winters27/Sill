//! Naming a clipboard entry, and correcting its text.
//!
//! Against the real store in a temporary file, like the collections tests
//! beside it: the questions are about what SQLite does with a title column
//! and a moved hash, and a fixture cannot answer those.

use crate::clipboard::kind::Kind;
use crate::clipboard::store::{Edit, Recording, Store};

const NOW: i64 = 1_700_000_000;

fn store() -> (Store, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temp directory");
    let store = Store::open(&dir.path().join("clipboard.db")).expect("opens");
    (store, dir)
}

fn add(store: &Store, text: &str) -> i64 {
    store
        .record(Recording {
            hash: &crate::clipboard::monitor::hash(text.as_bytes()),
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
fn a_renamed_entry_keeps_its_text_and_is_found_by_its_name() {
    let (store, _dir) = store();
    let id = add(&store, "ssh-keygen -t ed25519 -C work");
    add(&store, "something else entirely");

    store.set_title(id, Some("key command")).expect("names it");

    let entry = store.get(id).expect("reads").expect("still there");
    assert_eq!(entry.title.as_deref(), Some("key command"));
    assert_eq!(entry.text, "ssh-keygen -t ed25519 -C work");

    // Found by the name, which the full-text index knows nothing about.
    let found = store.search("key command", None, 10).expect("searches");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id);

    // And still found by its text, once, not once per index.
    let by_text = store.search("ed25519", None, 10).expect("searches");
    assert_eq!(by_text.len(), 1);
    assert_eq!(by_text[0].id, id);
}

#[test]
fn a_blank_name_is_no_name() {
    let (store, _dir) = store();
    let id = add(&store, "hello");

    store.set_title(id, Some("   ")).expect("accepts");
    assert_eq!(store.get(id).unwrap().unwrap().title, None);

    store.set_title(id, Some("greeting")).expect("names");
    store.set_title(id, None).expect("unnames");
    assert_eq!(store.get(id).unwrap().unwrap().title, None);
}

#[test]
fn editing_text_moves_the_hash_with_it() {
    let (store, _dir) = store();
    let id = add(&store, "teh quick brown fox");

    store.set_text(id, "the quick brown fox", NOW + 1).expect("edits");

    let entry = store.get(id).unwrap().unwrap();
    assert_eq!(entry.text, "the quick brown fox");
    assert_eq!(entry.last_seen, NOW + 1);

    // The full-text index followed, through the trigger that had never fired.
    let found = store.search("quick brown", None, 10).expect("searches");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, id);

    // Copying the corrected text again lands on the same row, which is what
    // moving the hash was for.
    assert_eq!(add(&store, "the quick brown fox"), id);
}

#[test]
fn editing_into_a_text_already_kept_is_refused_not_merged() {
    let (store, _dir) = store();
    let one = add(&store, "one");
    let two = add(&store, "two");

    assert_eq!(store.set_text(two, "one", NOW + 1), Err(Edit::Collides));

    // Both rows are exactly as they were.
    assert_eq!(store.get(one).unwrap().unwrap().text, "one");
    assert_eq!(store.get(two).unwrap().unwrap().text, "two");
}

#[test]
fn the_name_and_text_can_both_be_put_back() {
    let (store, _dir) = store();
    let id = add(&store, "first draft");

    store.set_title(id, Some("draft")).expect("names");
    store.set_text(id, "second draft", NOW + 1).expect("edits");

    // What an undo does: both back, whatever one action changed.
    store.set_title(id, None).expect("unnames");
    store.set_text(id, "first draft", NOW + 2).expect("restores");

    let entry = store.get(id).unwrap().unwrap();
    assert_eq!(entry.title, None);
    assert_eq!(entry.text, "first draft");
}
