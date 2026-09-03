//! Where `LocalStorage` actually lives.
//!
//! It was a `HashMap` held by the API layer, which meant every extension lost
//! everything it had saved the moment Sill closed, and `LocalStorage` is how
//! extensions remember a token, a last selection, a generated history. A store
//! that silently forgets is worse than one that is missing, because the
//! extension has no way to tell.
//!
//! SQLite rather than a JSON file per extension, for the reason the clipboard
//! uses it: writes are small, frequent and concurrent with reads, and a file
//! rewritten whole on every `set` loses everything on a bad shutdown.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};

/// What shape this build leaves the database in.
///
/// Every value here is an extension's own JSON, so the interesting change is
/// not to a column but to how the rows are scoped: `(extension, key)` is what
/// keeps one extension out of another's storage, and anything that alters that
/// meaning has to be a version an older build refuses rather than reads.
const SCHEMA: u32 = 1;

pub struct Storage {
    /// One connection behind a lock rather than a pool. Extensions write a
    /// key at a time and rarely; a pool would be machinery for no traffic.
    connection: Mutex<Connection>,
}

impl Storage {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self::from_connection(Connection::open(path)?)
    }

    /// A store that exists only for the length of a test.
    pub fn memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> rusqlite::Result<Self> {
        // Before the table is created, so a file a later build wrote is
        // refused rather than having this build's DDL run over it. See the
        // note on the helper for why a database is refused where a JSON file
        // is kept aside.
        crate::json_store::refuse_a_newer_database(&connection, SCHEMA, "extension storage")?;

        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS storage (
                -- Scoped per extension, and the scope is half the point: one
                -- extension must not be able to read or clobber another's
                -- keys, and a shared namespace makes that a matter of luck.
                extension TEXT NOT NULL,
                key       TEXT NOT NULL,
                -- Held as JSON text. LocalStorage values are whatever an
                -- extension passed, and flattening them to strings would
                -- change what comes back out.
                value     TEXT NOT NULL,
                PRIMARY KEY (extension, key)
            ) WITHOUT ROWID;
            "#,
        )?;

        crate::json_store::stamp_database(&connection, SCHEMA)?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn get(&self, extension: &str, key: &str) -> Value {
        let guard = self.connection.lock().expect("storage poisoned");
        let stored: Option<String> = guard
            .query_row(
                "SELECT value FROM storage WHERE extension = ?1 AND key = ?2",
                (extension, key),
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);

        stored
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null)
    }

    pub fn set(&self, extension: &str, key: &str, value: &Value) -> rusqlite::Result<()> {
        let text = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
        let guard = self.connection.lock().expect("storage poisoned");
        guard.execute(
            "INSERT INTO storage (extension, key, value) VALUES (?1, ?2, ?3)
             ON CONFLICT (extension, key) DO UPDATE SET value = excluded.value",
            (extension, key, text),
        )?;
        Ok(())
    }

    pub fn remove(&self, extension: &str, key: &str) -> rusqlite::Result<()> {
        let guard = self.connection.lock().expect("storage poisoned");
        guard.execute(
            "DELETE FROM storage WHERE extension = ?1 AND key = ?2",
            (extension, key),
        )?;
        Ok(())
    }

    pub fn clear(&self, extension: &str) -> rusqlite::Result<()> {
        let guard = self.connection.lock().expect("storage poisoned");
        guard.execute("DELETE FROM storage WHERE extension = ?1", (extension,))?;
        Ok(())
    }

    pub fn list(&self, extension: &str) -> Map<String, Value> {
        let guard = self.connection.lock().expect("storage poisoned");
        let Ok(mut statement) =
            guard.prepare("SELECT key, value FROM storage WHERE extension = ?1")
        else {
            return Map::new();
        };

        let rows = statement.query_map((extension,), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });

        let Ok(rows) = rows else { return Map::new() };

        rows.flatten()
            .map(|(key, text)| {
                let value = serde_json::from_str(&text).unwrap_or(Value::Null);
                (key, value)
            })
            .collect()
    }
}

/// Where the store lives, given the app's data directory.
pub fn path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("extension-storage.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The header records which shape this build left the file in.
    #[test]
    fn the_schema_version_is_written_into_the_file() {
        let store = Storage::memory().expect("in-memory store opens");

        let stamped: u32 = store
            .connection
            .lock()
            .expect("storage poisoned")
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("readable");

        assert_eq!(stamped, SCHEMA);
    }

    /// A store from a later build is refused rather than opened.
    ///
    /// What is at stake is the scoping. `(extension, key)` is what keeps one
    /// extension out of another's storage, and a build that changed how rows
    /// are scoped would have this one reading and writing across the boundary
    /// while everything looked normal.
    #[test]
    fn a_store_from_a_later_build_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("extension-storage.db");

        let ahead = Connection::open(&path).expect("opens");
        ahead
            .pragma_update(None, "user_version", SCHEMA + 1)
            .expect("stamped");
        drop(ahead);

        assert!(Storage::open(&path).is_err());
    }

    /// Every extension store on disk has a zero in the header.
    #[test]
    fn a_store_written_before_versioning_still_opens() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("extension-storage.db");

        let before = Connection::open(&path).expect("opens");
        before
            .pragma_update(None, "user_version", 0)
            .expect("stamped");
        drop(before);

        let store = Storage::open(&path).expect("an unstamped store has to open");
        store.set("alpha", "token", &json!("abc123")).unwrap();
        assert_eq!(store.get("alpha", "token"), json!("abc123"));
    }

    #[test]
    fn a_value_survives_being_written_and_read_back() {
        let store = Storage::memory().expect("in-memory store opens");
        store.set("alpha", "token", &json!("abc123")).unwrap();
        assert_eq!(store.get("alpha", "token"), json!("abc123"));
    }

    #[test]
    fn a_missing_key_is_null_rather_than_an_error() {
        // What `LocalStorage.getItem` promises. An error here would surface
        // in an extension as a rejected promise on a perfectly normal miss.
        let store = Storage::memory().unwrap();
        assert_eq!(store.get("alpha", "never-set"), Value::Null);
    }

    #[test]
    fn structure_survives_the_round_trip() {
        // The reason values are stored as JSON rather than as strings: an
        // extension that saved an array must not get back its debug form.
        let store = Storage::memory().unwrap();
        let value = json!({"items": [1, 2, 3], "nested": {"ok": true}});
        store.set("alpha", "state", &value).unwrap();
        assert_eq!(store.get("alpha", "state"), value);
    }

    #[test]
    fn one_extension_cannot_see_or_clobber_another() {
        // The isolation is the point. Two extensions using the obvious key
        // name must not fight over one row.
        let store = Storage::memory().unwrap();
        store.set("alpha", "key", &json!("from alpha")).unwrap();
        store.set("beta", "key", &json!("from beta")).unwrap();

        assert_eq!(store.get("alpha", "key"), json!("from alpha"));
        assert_eq!(store.get("beta", "key"), json!("from beta"));

        store.clear("alpha").unwrap();
        assert_eq!(store.get("alpha", "key"), Value::Null);
        assert_eq!(
            store.get("beta", "key"),
            json!("from beta"),
            "clearing one extension emptied another"
        );
    }

    #[test]
    fn setting_the_same_key_twice_replaces_rather_than_failing() {
        // A primary key without the upsert would make the second write an
        // error, which is what `setItem` is for.
        let store = Storage::memory().unwrap();
        store.set("alpha", "key", &json!(1)).unwrap();
        store.set("alpha", "key", &json!(2)).unwrap();
        assert_eq!(store.get("alpha", "key"), json!(2));
    }

    #[test]
    fn listing_returns_only_that_extensions_keys() {
        let store = Storage::memory().unwrap();
        store.set("alpha", "a", &json!(1)).unwrap();
        store.set("alpha", "b", &json!(2)).unwrap();
        store.set("beta", "c", &json!(3)).unwrap();

        let listed = store.list("alpha");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed.get("a"), Some(&json!(1)));
        assert!(listed.get("c").is_none(), "another extension's key leaked");
    }

    #[test]
    fn removing_one_key_leaves_the_rest() {
        let store = Storage::memory().unwrap();
        store.set("alpha", "keep", &json!(1)).unwrap();
        store.set("alpha", "drop", &json!(2)).unwrap();
        store.remove("alpha", "drop").unwrap();

        assert_eq!(store.get("alpha", "keep"), json!(1));
        assert_eq!(store.get("alpha", "drop"), Value::Null);
    }
}
