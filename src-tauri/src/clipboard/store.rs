//! Where clipboard history lives.
//!
//! SQLite with FTS5 rather than the JSON lines the rest of Sill uses. The
//! difference is search: this grows to tens of thousands of entries and the
//! whole point is finding one of them by a word you remember. Reading a
//! JSON file into memory to substring-match it stops being viable long before
//! that, and FTS5 is a rank-ordered index that ships inside SQLite.
//!
//! Images are kept as their bytes in a side table rather than inline, so a
//! query that lists a hundred rows does not carry a hundred screenshots with
//! it.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::clipboard::kind::Kind;

/// One thing that was copied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub kind: Kind,
    /// The text, or a description for an image.
    pub text: String,
    /// Unix seconds when it was first copied.
    pub first_seen: i64,
    /// Unix seconds when it was last copied or used.
    pub last_seen: i64,
    /// How many times it has been copied or pasted from here.
    pub uses: i64,
    /// Kept until deleted by hand, and never trimmed by retention.
    pub pinned: bool,
    /// The application it was copied from, when that could be read.
    pub app: Option<String>,
    /// That application's executable, which is the only thing an icon can be
    /// extracted from. The name alone cannot be turned back into a path.
    pub app_path: Option<String>,
    /// Size in bytes, which is the only useful thing to say about an image.
    pub bytes: i64,
}

pub struct Store {
    connection: Connection,
}

impl Store {
    /// Opens the history, creating it if this is the first run.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let connection = Connection::open(path)?;

        // WAL so a read while the monitor is writing does not block, and
        // NORMAL because losing the last clipboard entry to a power cut is
        // not worth an fsync on every copy.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;

        let store = Self { connection };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS entries (
                id         INTEGER PRIMARY KEY,
                -- The deduplication key. A UNIQUE index on the text itself
                -- would refuse an entry longer than SQLite's index limit,
                -- and copying a whole file is a thing people do.
                hash       TEXT NOT NULL UNIQUE,
                kind       TEXT NOT NULL,
                text       TEXT NOT NULL,
                first_seen INTEGER NOT NULL,
                last_seen  INTEGER NOT NULL,
                uses       INTEGER NOT NULL DEFAULT 1,
                pinned     INTEGER NOT NULL DEFAULT 0,
                app        TEXT,
                bytes      INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS entries_recent ON entries(pinned DESC, last_seen DESC);

            -- Contentless-delete FTS, so the text is not stored twice.
            CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
                text,
                content = 'entries',
                content_rowid = 'id',
                tokenize = 'unicode61'
            );

            -- The index is only correct if every write to `entries` reaches
            -- it, which is what these triggers guarantee. Maintaining it by
            -- hand from Rust is the classic way to end up with a search that
            -- silently misses recent entries.
            CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
                INSERT INTO entries_fts(rowid, text) VALUES (new.id, new.text);
            END;
            CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
                INSERT INTO entries_fts(entries_fts, rowid, text) VALUES('delete', old.id, old.text);
            END;
            CREATE TRIGGER IF NOT EXISTS entries_au AFTER UPDATE OF text ON entries BEGIN
                INSERT INTO entries_fts(entries_fts, rowid, text) VALUES('delete', old.id, old.text);
                INSERT INTO entries_fts(rowid, text) VALUES (new.id, new.text);
            END;

            CREATE TABLE IF NOT EXISTS blobs (
                entry_id INTEGER PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
                data     BLOB NOT NULL
            );
            "#,
        )?;

        // Added after the table already existed in the wild, so it cannot go
        // in the CREATE above. `ALTER TABLE` is cheap, and the error when the
        // column is already there is the expected outcome on every run but
        // the first.
        let _ = self
            .connection
            .execute("ALTER TABLE entries ADD COLUMN app_path TEXT", []);

        // Off by default in SQLite, and the blobs table depends on it.
        self.connection.pragma_update(None, "foreign_keys", true)?;
        Ok(())
    }

    /// Records something copied, or bumps it if it is already the newest.
    ///
    /// Returns the entry's id. Copying the same thing twice does not produce
    /// two rows: it moves the existing one to the top and counts the use,
    /// which is what makes a history of the last hundred copies useful rather
    /// than a hundred copies of the same URL.
    pub fn record(
        &self,
        hash: &str,
        kind: Kind,
        text: &str,
        app: Option<&str>,
        app_path: Option<&str>,
        bytes: i64,
        now: i64,
    ) -> rusqlite::Result<i64> {
        self.connection.execute(
            r#"
            INSERT INTO entries (hash, kind, text, first_seen, last_seen, app, bytes, app_path)
            VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)
            ON CONFLICT(hash) DO UPDATE SET
                last_seen = ?4,
                uses = uses + 1,
                -- A later copy knows where it came from; an earlier one may
                -- not have. Never overwrite a known source with nothing.
                app = COALESCE(?5, app),
                app_path = COALESCE(?7, app_path)
            "#,
            params![hash, kind.as_str(), text, now, app, bytes, app_path],
        )?;

        self.connection
            .query_row("SELECT id FROM entries WHERE hash = ?1", [hash], |row| {
                row.get(0)
            })
    }

    /// Stores an image's bytes against an entry.
    pub fn put_blob(&self, id: i64, data: &[u8]) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO blobs (entry_id, data) VALUES (?1, ?2)",
            params![id, data],
        )?;
        Ok(())
    }

    pub fn blob(&self, id: i64) -> rusqlite::Result<Option<Vec<u8>>> {
        self.connection
            .query_row("SELECT data FROM blobs WHERE entry_id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()
    }

    /// The history, newest first, pinned entries above everything.
    ///
    /// An empty query lists; a non-empty one searches the full-text index and
    /// orders by relevance instead. Two different orderings on purpose: with
    /// no query the useful answer is "what did I just copy", and with one it
    /// is "which entry matches best".
    pub fn search(
        &self,
        query: &str,
        kind: Option<Kind>,
        limit: usize,
    ) -> rusqlite::Result<Vec<Entry>> {
        let trimmed = query.trim();
        let filter = kind.map(Kind::as_str);

        let mut rows = if trimmed.is_empty() {
            let mut statement = self.connection.prepare(
                r#"
                SELECT id, kind, text, first_seen, last_seen, uses, pinned, app, bytes, app_path
                FROM entries
                WHERE (?1 IS NULL OR kind = ?1)
                ORDER BY pinned DESC, last_seen DESC
                LIMIT ?2
                "#,
            )?;
            let mapped = statement
                .query_map(params![filter, limit as i64], read_entry)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            mapped
        } else {
            let mut statement = self.connection.prepare(
                r#"
                SELECT e.id, e.kind, e.text, e.first_seen, e.last_seen, e.uses, e.pinned, e.app, e.bytes, e.app_path
                FROM entries_fts f
                JOIN entries e ON e.id = f.rowid
                WHERE entries_fts MATCH ?1 AND (?2 IS NULL OR e.kind = ?2)
                ORDER BY e.pinned DESC, bm25(entries_fts)
                LIMIT ?3
                "#,
            )?;
            let mapped = statement
                .query_map(
                    params![fts_query(trimmed), filter, limit as i64],
                    read_entry,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            mapped
        };

        rows.shrink_to_fit();
        Ok(rows)
    }

    pub fn get(&self, id: i64) -> rusqlite::Result<Option<Entry>> {
        self.connection
            .query_row(
                r#"
                SELECT id, kind, text, first_seen, last_seen, uses, pinned, app, bytes, app_path
                FROM entries WHERE id = ?1
                "#,
                [id],
                read_entry,
            )
            .optional()
    }

    /// Counts a use without moving the entry, for a paste out of the history.
    pub fn touch(&self, id: i64, now: i64) -> rusqlite::Result<()> {
        self.connection.execute(
            "UPDATE entries SET uses = uses + 1, last_seen = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    pub fn set_pinned(&self, id: i64, pinned: bool) -> rusqlite::Result<()> {
        self.connection.execute(
            "UPDATE entries SET pinned = ?2 WHERE id = ?1",
            params![id, pinned],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> rusqlite::Result<()> {
        self.connection
            .execute("DELETE FROM entries WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Empties the history. Pinned entries survive unless `everything`.
    pub fn clear(&self, everything: bool) -> rusqlite::Result<usize> {
        let removed = if everything {
            self.connection.execute("DELETE FROM entries", [])?
        } else {
            self.connection
                .execute("DELETE FROM entries WHERE pinned = 0", [])?
        };
        Ok(removed)
    }

    /// Drops entries older than `days`, keeping pinned ones.
    ///
    /// A clipboard accumulates everything typed near a password field and
    /// every one-time code, so a history with no end date is a liability
    /// rather than a feature.
    pub fn prune(&self, days: u32, now: i64) -> rusqlite::Result<usize> {
        if days == 0 {
            return Ok(0);
        }
        let cutoff = now - i64::from(days) * 86_400;
        self.connection.execute(
            "DELETE FROM entries WHERE pinned = 0 AND last_seen < ?1",
            [cutoff],
        )
    }

    pub fn count(&self) -> rusqlite::Result<i64> {
        self.connection
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
    }
}

fn read_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: row.get(0)?,
        kind: Kind::from_str(&row.get::<_, String>(1)?),
        text: row.get(2)?,
        first_seen: row.get(3)?,
        last_seen: row.get(4)?,
        uses: row.get(5)?,
        pinned: row.get::<_, i64>(6)? != 0,
        app: row.get(7)?,
        bytes: row.get(8)?,
        app_path: row.get(9)?,
    })
}

/// Turns what someone typed into something FTS5 will accept.
///
/// FTS5's query language treats `"`, `*`, `(`, `:` and others as syntax, so a
/// raw query containing one is a syntax error rather than a search. Every
/// term is quoted and a prefix wildcard is added to the last, which is what
/// makes results appear while still typing.
fn fts_query(input: &str) -> String {
    let terms: Vec<String> = input
        .split_whitespace()
        .map(|term| term.replace('"', ""))
        .filter(|term| !term.is_empty())
        .collect();

    if terms.is_empty() {
        return String::from("\"\"");
    }

    let last = terms.len() - 1;
    terms
        .iter()
        .enumerate()
        .map(|(i, term)| {
            if i == last {
                format!("\"{term}\"*")
            } else {
                format!("\"{term}\"")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = Store::open(&dir.path().join("clipboard.db")).expect("opens");
        (dir, store)
    }

    fn add(store: &Store, text: &str, at: i64) -> i64 {
        store
            .record(
                text,
                Kind::Text,
                text,
                Some("Test"),
                None,
                text.len() as i64,
                at,
            )
            .expect("records")
    }

    #[test]
    fn copying_the_same_thing_twice_bumps_it_rather_than_duplicating() {
        // Otherwise a history of the last hundred copies is a hundred copies
        // of the same URL.
        let (_dir, store) = store();
        let first = add(&store, "hello", NOW);
        let again = add(&store, "hello", NOW + 60);

        assert_eq!(first, again, "the same text is the same entry");
        assert_eq!(store.count().expect("counts"), 1);

        let entry = store.get(first).expect("reads").expect("exists");
        assert_eq!(entry.uses, 2);
        assert_eq!(entry.first_seen, NOW, "the first sighting is remembered");
        assert_eq!(entry.last_seen, NOW + 60);
    }

    #[test]
    fn a_later_copy_never_forgets_where_an_earlier_one_came_from() {
        let (_dir, store) = store();
        store
            .record(
                "x",
                Kind::Text,
                "x",
                Some("Slack"),
                Some("C:/slack.exe"),
                1,
                NOW,
            )
            .expect("records");
        store
            .record("x", Kind::Text, "x", None, None, 1, NOW + 1)
            .expect("records");

        let entry = store.get(1).expect("reads").expect("exists");
        assert_eq!(entry.app.as_deref(), Some("Slack"));
        assert_eq!(
            entry.app_path.as_deref(),
            Some("C:/slack.exe"),
            "the path is what the icon comes from, so it must survive too"
        );
    }

    #[test]
    fn an_empty_query_lists_newest_first_with_pins_on_top() {
        let (_dir, store) = store();
        add(&store, "oldest", NOW);
        add(&store, "middle", NOW + 10);
        let newest = add(&store, "newest", NOW + 20);
        let pinned = add(&store, "pinned", NOW - 1000);
        store.set_pinned(pinned, true).expect("pins");

        let listed = store.search("", None, 10).expect("searches");
        assert_eq!(listed[0].text, "pinned", "a pin outranks recency");
        assert_eq!(listed[1].id, newest);
        assert_eq!(listed.len(), 4);
    }

    #[test]
    fn search_finds_by_a_word_from_the_middle() {
        let (_dir, store) = store();
        add(&store, "the quick brown fox", NOW);
        add(&store, "something else entirely", NOW + 1);

        let found = store.search("brown", None, 10).expect("searches");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "the quick brown fox");
    }

    #[test]
    fn search_matches_a_prefix_so_results_appear_while_typing() {
        let (_dir, store) = store();
        add(&store, "deployment checklist", NOW);

        for typed in ["dep", "deploy", "deployment"] {
            assert_eq!(
                store.search(typed, None, 10).expect("searches").len(),
                1,
                "typing {typed:?} should already find it"
            );
        }
    }

    #[test]
    fn a_query_full_of_fts_syntax_searches_rather_than_erroring() {
        // FTS5 treats these as its own grammar; unescaped they are a syntax
        // error, which would look like search being broken.
        let (_dir, store) = store();
        add(&store, "select * from users", NOW);

        for query in ["*", "\"", "(", "a:b", "NEAR", "users OR", "-"] {
            assert!(
                store.search(query, None, 10).is_ok(),
                "{query:?} should search, not fail"
            );
        }
    }

    #[test]
    fn deleting_removes_it_from_search_as_well_as_the_list() {
        // The FTS index is maintained by triggers; without them a deleted
        // entry keeps turning up in results forever.
        let (_dir, store) = store();
        let id = add(&store, "findable", NOW);
        assert_eq!(
            store.search("findable", None, 10).expect("searches").len(),
            1
        );

        store.delete(id).expect("deletes");
        assert_eq!(
            store.search("findable", None, 10).expect("searches").len(),
            0
        );
        assert_eq!(store.count().expect("counts"), 0);
    }

    #[test]
    fn filtering_by_kind_narrows_both_listing_and_search() {
        let (_dir, store) = store();
        store
            .record("a", Kind::Link, "https://example.com", None, None, 0, NOW)
            .expect("records");
        store
            .record("b", Kind::Text, "example text", None, None, 0, NOW)
            .expect("records");

        assert_eq!(
            store.search("", Some(Kind::Link), 10).expect("lists").len(),
            1
        );
        assert_eq!(
            store
                .search("example", Some(Kind::Link), 10)
                .expect("searches")
                .len(),
            1
        );
    }

    #[test]
    fn clearing_spares_pinned_entries_unless_told_otherwise() {
        let (_dir, store) = store();
        add(&store, "ordinary", NOW);
        let pinned = add(&store, "kept", NOW);
        store.set_pinned(pinned, true).expect("pins");

        store.clear(false).expect("clears");
        assert_eq!(store.count().expect("counts"), 1, "the pin survives");

        store.clear(true).expect("clears everything");
        assert_eq!(store.count().expect("counts"), 0);
    }

    #[test]
    fn pruning_drops_the_old_and_keeps_the_pinned() {
        // A clipboard accumulates one-time codes and whatever was near a
        // password field, so a history with no end date is a liability.
        let (_dir, store) = store();
        add(&store, "recent", NOW);
        add(&store, "ancient", NOW - 40 * 86_400);
        let pinned = add(&store, "old but kept", NOW - 90 * 86_400);
        store.set_pinned(pinned, true).expect("pins");

        let removed = store.prune(30, NOW).expect("prunes");
        assert_eq!(removed, 1);

        let left: Vec<String> = store
            .search("", None, 10)
            .expect("lists")
            .into_iter()
            .map(|e| e.text)
            .collect();
        assert!(left.contains(&"recent".to_string()));
        assert!(left.contains(&"old but kept".to_string()));
        assert!(!left.contains(&"ancient".to_string()));
    }

    #[test]
    fn a_retention_of_zero_days_means_keep_everything() {
        let (_dir, store) = store();
        add(&store, "old", NOW - 1000 * 86_400);
        assert_eq!(store.prune(0, NOW).expect("prunes"), 0);
        assert_eq!(store.count().expect("counts"), 1);
    }

    #[test]
    fn an_image_blob_survives_a_round_trip_and_goes_with_its_entry() {
        let (_dir, store) = store();
        let id = add(&store, "screenshot", NOW);
        store.put_blob(id, &[1, 2, 3, 4]).expect("stores");

        assert_eq!(store.blob(id).expect("reads"), Some(vec![1, 2, 3, 4]));

        store.delete(id).expect("deletes");
        assert_eq!(store.blob(id).expect("reads"), None, "the blob is cascaded");
    }

    #[test]
    fn a_very_long_entry_is_stored_rather_than_refused() {
        // Copying a whole file happens, and a UNIQUE index on the text
        // itself would refuse it. The hash is the key for this reason.
        let (_dir, store) = store();
        let long = "x".repeat(5_000_000);
        let id = store
            .record(
                "hash-of-long",
                Kind::Text,
                &long,
                None,
                None,
                long.len() as i64,
                NOW,
            )
            .expect("records");
        assert_eq!(
            store.get(id).expect("reads").expect("exists").text.len(),
            5_000_000
        );
    }
}
