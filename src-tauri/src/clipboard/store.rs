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
    /// Whether a formatted version was kept alongside the text.
    ///
    /// A flag rather than the markup, because the list sends every row to the
    /// window on every search and the markup is routinely many times the size
    /// of the text. Whoever needs the markup asks for the entry by id.
    #[serde(default)]
    pub rich: bool,
}

pub struct Store {
    connection: Connection,
}

/// A named group of history entries.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub created: i64,
    /// How many entries are in it right now.
    pub count: i64,
}

/// One thing copied, as it is written down.
///
/// A struct rather than eight positional arguments. Two of them are
/// `Option<&str>` and adjacent, which is exactly the shape where a caller
/// silently swaps them and nothing complains.
pub struct Recording<'a> {
    /// The deduplication key.
    pub hash: &'a str,
    pub kind: Kind,
    pub text: &'a str,
    /// The formatted version, when the source offered one.
    pub html: Option<&'a str>,
    pub app: Option<&'a str>,
    pub app_path: Option<&'a str>,
    pub bytes: i64,
    pub now: i64,
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

        // How much of the log may pile up before it is folded back in.
        //
        // SQLite waits for a thousand pages, which is about four megabytes,
        // and a clipboard writes a few kilobytes at a time. Measured on a real
        // machine before this: **a 557 KB history with a 3.46 MB log beside
        // it**, six times the size of the thing it describes, because nothing
        // ever wrote enough at once to reach the threshold.
        //
        // A quarter of that, so the log stays roughly the size of the history
        // rather than several times it. Folding in more often costs a little
        // more work per copy, which is an operation that happens when a person
        // presses two keys and has milliseconds to spare.
        connection.pragma_update(None, "wal_autocheckpoint", 256)?;

        let store = Self { connection };
        store.migrate()?;

        // Whatever piled up before now, handed back to the filesystem. The
        // setting above bounds what happens next; this is what shrinks a log
        // that already grew, which nothing else would ever do.
        store.compact();

        Ok(store)
    }

    /// Folds the log back into the history and hands the space back.
    ///
    /// `TRUNCATE` rather than `PASSIVE`, because passive leaves the file at
    /// whatever size it reached and the point here is to give it back.
    ///
    /// Failure is ignored on purpose. A checkpoint cannot run while another
    /// connection is reading, and the answer to that is to try again next
    /// time, not to refuse to open the clipboard history.
    pub fn compact(&self) {
        let _ = self
            .connection
            .pragma_update(None, "wal_checkpoint", "TRUNCATE");
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

            -- Named groups of entries.
            --
            -- Names are unique because a collection is chosen by name, and two
            -- with the same one would be indistinguishable in every list that
            -- offers them.
            CREATE TABLE IF NOT EXISTS collections (
                id      INTEGER PRIMARY KEY,
                name    TEXT NOT NULL UNIQUE COLLATE NOCASE,
                created INTEGER NOT NULL
            );

            -- Membership, with the order the entries were put in.
            --
            -- ON DELETE CASCADE both ways: deleting an entry has to remove it
            -- from every collection, and deleting a collection must not leave
            -- rows pointing at nothing. Retention prunes entries on its own
            -- schedule, so this is not a rare case.
            CREATE TABLE IF NOT EXISTS collection_entries (
                collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                entry_id      INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                position      INTEGER NOT NULL,
                PRIMARY KEY (collection_id, entry_id)
            );

            CREATE INDEX IF NOT EXISTS collection_order
                ON collection_entries(collection_id, position);
            "#,
        )?;

        // Added after the table already existed in the wild, so it cannot go
        // in the CREATE above. `ALTER TABLE` is cheap, and the error when the
        // column is already there is the expected outcome on every run but
        // the first.
        let _ = self
            .connection
            .execute("ALTER TABLE entries ADD COLUMN app_path TEXT", []);

        // The formatted version of the same copy, when there was one.
        //
        // Beside the text rather than instead of it, and both are always
        // written. The plain text is what search reads, what a preview shows
        // and what "paste as plain text" pastes; the markup is only ever an
        // upgrade offered to an application that can take it.
        let _ = self
            .connection
            .execute("ALTER TABLE entries ADD COLUMN html TEXT", []);

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
    pub fn record(&self, recording: Recording<'_>) -> rusqlite::Result<i64> {
        let Recording {
            hash,
            kind,
            text,
            html,
            app,
            app_path,
            bytes,
            now,
        } = recording;

        self.connection.execute(
            r#"
            INSERT INTO entries (hash, kind, text, first_seen, last_seen, app, bytes, app_path, html)
            VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(hash) DO UPDATE SET
                last_seen = ?4,
                uses = uses + 1,
                -- A later copy knows where it came from; an earlier one may
                -- not have. Never overwrite a known source with nothing.
                app = COALESCE(?5, app),
                app_path = COALESCE(?7, app_path),
                -- Same reasoning: the same text copied once from an editor
                -- and once from a terminal should keep the formatted version
                -- it had, rather than losing it to the plainer copy.
                html = COALESCE(?8, html)
            "#,
            params![hash, kind.as_str(), text, now, app, bytes, app_path, html],
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
                SELECT id, kind, text, first_seen, last_seen, uses, pinned, app, bytes, app_path, html
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
                SELECT e.id, e.kind, e.text, e.first_seen, e.last_seen, e.uses, e.pinned, e.app, e.bytes, e.app_path, e.html
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
                SELECT id, kind, text, first_seen, last_seen, uses, pinned, app, bytes, app_path, html
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

    // ----------------------------------------------------------- collections

    /// Every collection, with how many entries each holds.
    ///
    /// The count comes from the same query rather than a second one per row.
    /// A collection whose entries have all aged out shows zero rather than
    /// disappearing: it was named deliberately and is still somewhere to put
    /// things.
    pub fn collections(&self) -> rusqlite::Result<Vec<Collection>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT c.id, c.name, c.created, COUNT(m.entry_id)
            FROM collections c
            LEFT JOIN collection_entries m ON m.collection_id = c.id
            GROUP BY c.id
            ORDER BY c.name COLLATE NOCASE
            "#,
        )?;

        let rows = statement.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                created: row.get(2)?,
                count: row.get(3)?,
            })
        })?;

        rows.collect()
    }

    /// Makes a collection, or returns the one already called that.
    ///
    /// Idempotent because the name is how a person refers to it. Asking for
    /// "Release notes" twice means the same collection both times, not a
    /// second one that shadows the first.
    pub fn create_collection(&self, name: &str, now: i64) -> rusqlite::Result<i64> {
        let name = name.trim();

        self.connection.execute(
            "INSERT OR IGNORE INTO collections (name, created) VALUES (?1, ?2)",
            params![name, now],
        )?;

        self.connection.query_row(
            "SELECT id FROM collections WHERE name = ?1 COLLATE NOCASE",
            [name],
            |row| row.get(0),
        )
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            "UPDATE collections SET name = ?2 WHERE id = ?1",
            params![id, name.trim()],
        )?;
        Ok(())
    }

    /// Removes a collection. The entries in it are untouched.
    ///
    /// A collection is a way of grouping the history, not a container that
    /// owns it. Deleting the group must not delete what somebody copied.
    pub fn delete_collection(&self, id: i64) -> rusqlite::Result<()> {
        self.connection
            .execute("DELETE FROM collections WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Adds entries to a collection, keeping the order they were given in.
    ///
    /// Already-present entries keep their original position rather than
    /// jumping to the end, so adding a batch that overlaps one already there
    /// does not reshuffle what was arranged.
    pub fn add_to_collection(&self, collection: i64, ids: &[i64]) -> rusqlite::Result<usize> {
        let next: i64 = self.connection.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM collection_entries WHERE collection_id = ?1",
            [collection],
            |row| row.get(0),
        )?;

        let mut added = 0;
        for (offset, id) in ids.iter().enumerate() {
            added += self.connection.execute(
                "INSERT OR IGNORE INTO collection_entries (collection_id, entry_id, position)
                 VALUES (?1, ?2, ?3)",
                params![collection, id, next + offset as i64],
            )?;
        }

        Ok(added)
    }

    pub fn remove_from_collection(&self, collection: i64, id: i64) -> rusqlite::Result<()> {
        self.connection.execute(
            "DELETE FROM collection_entries WHERE collection_id = ?1 AND entry_id = ?2",
            params![collection, id],
        )?;
        Ok(())
    }

    /// The entries in a collection, in the order they were added.
    ///
    /// Not newest first. A collection is something somebody arranged, and
    /// re-sorting it by when things were copied would throw that away.
    pub fn collection_entries(&self, collection: i64) -> rusqlite::Result<Vec<Entry>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT e.id, e.kind, e.text, e.first_seen, e.last_seen, e.uses, e.pinned, e.app, e.bytes, e.app_path, e.html
            FROM entries e
            JOIN collection_entries m ON m.entry_id = e.id
            WHERE m.collection_id = ?1
            ORDER BY m.position
            "#,
        )?;

        let rows = statement.query_map([collection], read_entry)?;
        rows.collect()
    }

    /// The formatted version of an entry, when it kept one.
    ///
    /// Asked for by id rather than carried on every row: markup is routinely
    /// several times the size of the text it formats, and the list sends every
    /// row to the window on every keystroke.
    pub fn html(&self, id: i64) -> rusqlite::Result<Option<String>> {
        self.connection
            .query_row("SELECT html FROM entries WHERE id = ?1", [id], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()
            .map(Option::flatten)
    }

    /// Several entries joined into one piece of text.
    ///
    /// The order is the order of `ids`, which is the order they were picked
    /// rather than the order they are listed in. Merging is composition, and
    /// a list sorted newest-first would silently assemble it backwards.
    ///
    /// A missing entry is skipped rather than fatal: it was deleted between
    /// being picked and being merged, and losing the rest of a selection over
    /// one row would be worse than merging what remains.
    pub fn merge(&self, ids: &[i64], separator: &str) -> rusqlite::Result<Option<String>> {
        let mut parts = Vec::with_capacity(ids.len());

        for id in ids {
            if let Some(entry) = self.get(*id)? {
                if !entry.text.is_empty() {
                    parts.push(entry.text);
                }
            }
        }

        Ok(if parts.is_empty() {
            None
        } else {
            Some(parts.join(separator))
        })
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
        rich: row.get::<_, Option<String>>(10)?.is_some(),
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
            .record(Recording {
                hash: text,
                kind: Kind::Text,
                text,
                html: None,
                app: Some("Test"),
                app_path: None,
                bytes: text.len() as i64,
                now: at,
            })
            .expect("records")
    }

    fn add_rich(store: &Store, text: &str, html: Option<&str>, at: i64) -> i64 {
        store
            .record(Recording {
                hash: text,
                kind: Kind::Text,
                text,
                html,
                app: Some("Word"),
                app_path: None,
                bytes: text.len() as i64,
                now: at,
            })
            .expect("records")
    }

    #[test]
    fn formatting_is_kept_beside_the_text_and_not_instead_of_it() {
        // Both, always. The plain text is what search reads and what a
        // preview shows; the markup is only ever an upgrade offered to an
        // application that can take it.
        let (_dir, store) = store();
        let id = add_rich(&store, "hello world", Some("<b>hello world</b>"), NOW);

        let entry = store.get(id).expect("reads").expect("exists");
        assert_eq!(entry.text, "hello world", "the plain text is still there");
        assert!(entry.rich, "and it says a formatted version exists");

        assert_eq!(
            store.html(id).expect("reads"),
            Some("<b>hello world</b>".to_string())
        );
    }

    #[test]
    fn a_plainer_copy_of_the_same_text_does_not_lose_the_formatting() {
        // The same sentence copied once from a document and once from a
        // terminal is one entry. Letting the plainer copy win would quietly
        // strip formatting that was there a moment ago.
        let (_dir, store) = store();
        let id = add_rich(&store, "shared", Some("<i>shared</i>"), NOW);
        let again = add_rich(&store, "shared", None, NOW + 60);

        assert_eq!(id, again, "the same text is the same entry");
        assert_eq!(
            store.html(id).expect("reads"),
            Some("<i>shared</i>".to_string()),
            "the formatting survived the plainer copy"
        );
        assert!(store.get(id).expect("reads").expect("exists").rich);
    }

    #[test]
    fn an_entry_with_no_formatting_says_so_rather_than_pretending() {
        let (_dir, store) = store();
        let id = add_rich(&store, "just text", None, NOW);

        assert!(!store.get(id).expect("reads").expect("exists").rich);
        assert_eq!(store.html(id).expect("reads"), None);
    }

    #[test]
    fn the_list_carries_a_flag_and_never_the_markup() {
        // Markup is routinely several times the size of the text it formats,
        // and every row of the list crosses to the window on every keystroke.
        // Sending it there would undo the payload work the search already did.
        let (_dir, store) = store();
        let long = format!("<span style=\"{}\">x</span>", "a:b;".repeat(500));
        add_rich(&store, "x", Some(&long), NOW);

        let listed = store.search("", None, 10).expect("searches");
        let json = serde_json::to_string(&listed).expect("serialises");

        assert!(json.contains("\"rich\":true"));
        assert!(
            !json.contains("<span"),
            "the markup reached the window: {} bytes",
            json.len()
        );
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
            .record(Recording {
                hash: "x",
                kind: Kind::Text,
                text: "x",
                html: None,
                app: Some("Slack"),
                app_path: Some("C:/slack.exe"),
                bytes: 1,
                now: NOW,
            })
            .expect("records");
        store
            .record(Recording {
                hash: "x",
                kind: Kind::Text,
                text: "x",
                html: None,
                app: None,
                app_path: None,
                bytes: 1,
                now: NOW + 1,
            })
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
            .record(Recording {
                hash: "a",
                kind: Kind::Link,
                text: "https://example.com",
                html: None,
                app: None,
                app_path: None,
                bytes: 0,
                now: NOW,
            })
            .expect("records");
        store
            .record(Recording {
                hash: "b",
                kind: Kind::Text,
                text: "example text",
                html: None,
                app: None,
                app_path: None,
                bytes: 0,
                now: NOW,
            })
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
            .record(Recording {
                hash: "hash-of-long",
                kind: Kind::Text,
                text: &long,
                html: None,
                app: None,
                app_path: None,
                bytes: long.len() as i64,
                now: NOW,
            })
            .expect("records");
        assert_eq!(
            store.get(id).expect("reads").expect("exists").text.len(),
            5_000_000
        );
    }
}
