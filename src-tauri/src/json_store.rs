//! One way to keep a file of JSON, so every store is as careful as the most
//! careful one.
//!
//! Sill had eight hand-rolled load and save pairs and no two of them agreed on
//! what care meant. Preferences staged its write, moved an unreadable file
//! aside and skipped a byte order mark. Quicklinks, workspaces and extension
//! grants wrote in place, so losing power mid-write lost every quicklink,
//! every saved arrangement and every permission somebody had granted. Nothing
//! but preferences skipped a byte order mark, so opening `quicklinks.json` in
//! Notepad and saving it emptied the file over three bytes of encoding. Only
//! three of the eight said anything at all when a file could not be read.
//!
//! None of those differences were decisions. They were the order the files got
//! written in. This module is the union of them, so a store gets all of it by
//! calling `load` and `save_atomic` rather than by remembering.
//!
//! # The version is a contract
//!
//! Every file carries the schema version the build that wrote it understood.
//! What happens on the way back in is fixed, and is not a judgement call:
//!
//! - **The same version, or older.** Read it. This is the ordinary upgrade
//!   path, and `#[serde(default)]` on every stored type covers a field added
//!   since. The number exists so that a change serde cannot absorb, a field
//!   whose *meaning* changed rather than whose name did, has somewhere to be
//!   noticed.
//! - **Newer than this build knows.** Refuse it, keep it aside, start from
//!   defaults. Reading a file from a later build as though its fields still
//!   mean what they used to is how data is quietly corrupted rather than
//!   loudly lost, and writing over it would destroy the newer install's data
//!   for good. Keeping it aside leaves it on disk to recover by hand, which is
//!   the worst outcome anybody can still fix.
//! - **No version at all.** Version zero, and it reads. Every file already on
//!   disk was written before this module existed, and refusing those would
//!   make this change itself the data loss it exists to prevent.
//!
//! A derived cache is the one exception to keeping a file aside, because there
//! is nothing in it the next scan does not produce again. It says so with
//! `Unreadable::Overwrite`.

use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Where the version number sits in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// A field of the payload, which is an object either way.
    ///
    /// For a file a person opens and edits, which is what `preferences.json`
    /// is. Wrapping that one would push every setting a level deeper for no
    /// gain the reader can see.
    Beside,
    /// A wrapper around the payload: `{"version": n, "items": <payload>}`.
    ///
    /// The only option for a file whose payload is a list or a map, which is
    /// six of the eight. A document with no numeric `version` beside an
    /// `items` is read as the payload itself at version zero, which is what
    /// every one of those files looks like today.
    Around,
}

/// Whether the file is printed for a person to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Indented. For anything somebody may open and edit.
    Readable,
    /// One line. For anything written on a hot path or measured in hundreds of
    /// kilobytes, where the indentation is most of the file.
    Compact,
}

/// What becomes of a file this build cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unreadable {
    /// Renamed beside itself, so the next save writes a clean file and what
    /// could not be read is still there to look at.
    ///
    /// The right answer for anything a person created. Falling back to
    /// defaults and then writing them over the top turns one torn write into
    /// permanent loss, and nothing about that failure looks like a failure.
    KeepAside,
    /// Left where it is, for the next save to replace.
    ///
    /// Only for a derived cache. Keeping half a megabyte of stale index aside
    /// on every start would accumulate copies of something the scan behind it
    /// rebuilds in a second.
    Overwrite,
}

/// How one store's file is kept.
///
/// Declared as a `const` beside the store it belongs to, so the shape, the
/// layout and the version live next to the type they describe rather than at
/// each call.
#[derive(Debug, Clone, Copy)]
pub struct Schema {
    /// What this build writes, and the highest it will read.
    pub version: u32,
    pub shape: Shape,
    pub layout: Layout,
    pub unreadable: Unreadable,
    /// Names the file in the log, in the words somebody would use for it.
    pub what: &'static str,
}

/// Reads a store, falling back to its default.
///
/// A missing file is the ordinary first run rather than a failure, so it is
/// the default and nothing is said about it.
pub fn load<T: DeserializeOwned + Default>(path: &Path, schema: &Schema) -> T {
    load_with(path, schema, |_| {})
}

/// Reads a store, with a chance to rewrite the document first.
///
/// The hook is for `preferences`, which decrypts the sealed credentials in the
/// document before it becomes a `Preferences`. Everything else reaches this
/// through `load` with a closure that does nothing.
pub fn load_with<T: DeserializeOwned + Default>(
    path: &Path,
    schema: &Schema,
    unseal: impl FnOnce(&mut Value),
) -> T {
    let Ok(text) = std::fs::read_to_string(path) else {
        return T::default();
    };

    /*
     * A byte order mark is not part of the JSON.
     *
     * Windows puts one on the front of any file written as "UTF-8" by Notepad,
     * by PowerShell's `Set-Content -Encoding UTF8`, and by a good deal else.
     * `serde_json` refuses the whole document over it, which means a file
     * somebody hand-edited to change one line costs them all of the others.
     *
     * Skipped rather than rejected. It carries no information: the encoding is
     * already known, and nothing in Sill writes one.
     *
     * Preferences learned this the hard way and was the only store that knew
     * it. It is the clearest reason this module exists. The fix belonged to
     * every file anybody might open, not to the one where it was noticed.
     */
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);

    let document = match serde_json::from_str::<Value>(text) {
        Ok(document) => document,
        Err(why) => return give_up(path, schema, &why.to_string()),
    };

    let (version, mut payload) = unwrap(document, schema.shape);

    if version > schema.version {
        return give_up(
            path,
            schema,
            &format!(
                "it is version {version} and this build understands {}",
                schema.version
            ),
        );
    }

    unseal(&mut payload);

    match serde_json::from_value(payload) {
        Ok(value) => value,
        Err(why) => give_up(path, schema, &why.to_string()),
    }
}

/// Reads a store whose file is a list, keeping every entry that can be read.
///
/// The same reasoning as `entries_that_can_be_read`, applied to a whole file.
/// Five of the eight stores are a bare list, and `Conversation`, `Profile` and
/// `CommandRecord` all have required fields, so one entry serde could not read
/// used to fail the whole file: every conversation, every saved arrangement or
/// the entire cached index, over one of them.
///
/// A file that is not a list at all is still a file that could not be read,
/// and goes through `Unreadable` like any other.
pub fn load_list<T: DeserializeOwned>(path: &Path, schema: &Schema) -> Vec<T> {
    readable(load::<Vec<Value>>(path, schema))
}

/// Writes a store, staged and renamed.
pub fn save_atomic<T: Serialize>(path: &Path, value: &T, schema: &Schema) -> std::io::Result<()> {
    save_atomic_with(path, value, schema, |_| {})
}

/// Writes a store, with a chance to rewrite the document first.
///
/// The hook is `preferences` sealing its credentials, and is the save half of
/// the one on `load_with`.
pub fn save_atomic_with<T: Serialize>(
    path: &Path,
    value: &T,
    schema: &Schema,
    seal: impl FnOnce(&mut Value),
) -> std::io::Result<()> {
    let text = to_text_with(value, schema, seal)?;
    write_text(path, &text)
}

/// The document as text, ready for somebody who is not holding a lock.
///
/// Split from `save_atomic` because two callers serialise under a lock and
/// write outside it. The frecency write used to happen on the registry lock
/// that the next keystroke waits behind.
pub fn to_text<T: Serialize>(value: &T, schema: &Schema) -> std::io::Result<String> {
    to_text_with(value, schema, |_| {})
}

fn to_text_with<T: Serialize>(
    value: &T,
    schema: &Schema,
    seal: impl FnOnce(&mut Value),
) -> std::io::Result<String> {
    /*
     * An error rather than a placeholder.
     *
     * Every one of the eight fell back to writing `{}` or `[]` when
     * serialisation failed, which puts an empty file where the real one was:
     * exactly the loss the staged write below exists to prevent, done on
     * purpose. It cannot happen for any type stored here, and if it ever does
     * the file should keep whatever it already holds.
     */
    let mut payload = serde_json::to_value(value)
        .map_err(|why| std::io::Error::new(std::io::ErrorKind::InvalidData, why))?;

    seal(&mut payload);

    let document = wrap(payload, schema);

    match schema.layout {
        Layout::Readable => serde_json::to_string_pretty(&document),
        Layout::Compact => serde_json::to_string(&document),
    }
    .map_err(|why| std::io::Error::new(std::io::ErrorKind::InvalidData, why))
}

/// Puts already-serialised text on disk, staged and renamed.
///
/// A plain write truncates first and fills afterwards, so losing power or
/// being killed in that window leaves a half-written file that reads as
/// corrupt on the next start. A rename is atomic on NTFS, so there is no
/// moment at which the file is neither the old one nor the new one.
pub fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let staging = path.with_extension("json.partial");
    std::fs::write(&staging, text)?;

    if let Err(why) = std::fs::rename(&staging, path) {
        let _ = std::fs::remove_file(&staging);
        return Err(why);
    }

    Ok(())
}

/// Reads a list, keeping every entry that can be read.
///
/// A struct that is a whole file, or a *section* of one, can carry
/// `#[serde(default)]`, and then anything missing is filled in. That does not
/// hold for a struct that is an *element of a list*: `Binding`, `Alias`,
/// `Conversation`, `Profile` and `CommandRecord` all have required fields, so
/// one entry serde cannot read fails the whole list, which fails the whole
/// file, which loses every other entry in it.
///
/// A list is the one place where dropping one thing is obviously better than
/// dropping everything. Defaulting the fields instead would be worse: a
/// binding with no accelerator and no action is not a binding, and keeping it
/// would put an empty row in the Shortcuts panel that does nothing.
///
/// Says what it dropped, because a shortcut, a snippet or a saved arrangement
/// quietly disappearing after an update is exactly the kind of thing nobody
/// can report usefully.
pub fn entries_that_can_be_read<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned,
{
    Ok(readable(Vec::<Value>::deserialize(deserializer)?))
}

/// Every entry that can be read, saying which ones could not.
fn readable<T: DeserializeOwned>(raw: Vec<Value>) -> Vec<T> {
    raw.into_iter()
        .filter_map(|value| match serde_json::from_value::<T>(value) {
            Ok(one) => Some(one),
            Err(why) => {
                crate::say!(
                    "dropped one {} that could not be read: {why}",
                    std::any::type_name::<T>()
                );
                None
            }
        })
        .collect()
}

/// The version the file claims, and the payload without it.
fn unwrap(document: Value, shape: Shape) -> (u32, Value) {
    match shape {
        Shape::Beside => match document {
            Value::Object(mut fields) => {
                // Taken out either way, so a store never sees a field it does
                // not declare. One that cannot be read as a number is version
                // zero rather than a refusal, for the reason a byte order mark
                // is skipped: one odd value in a hand-edited file must not
                // cost somebody the rest of it.
                let version = fields.remove(VERSION).and_then(as_version).unwrap_or(0);
                (version, Value::Object(fields))
            }
            other => (0, other),
        },
        Shape::Around => {
            let Value::Object(mut fields) = document else {
                // A bare list, which is what these files hold today.
                return (0, document);
            };

            /*
             * Both keys, and a number, before this counts as a wrapper.
             *
             * `extension-grants.json` is itself a map, so in principle a file
             * written before this could hold an extension called `version`.
             * Its value would be a list of capabilities rather than a number,
             * so it cannot be mistaken for one of these wrappers.
             */
            let version = fields
                .get(VERSION)
                .and_then(|found| as_version(found.clone()));

            match (version, fields.remove(ITEMS)) {
                (Some(version), Some(items)) => (version, items),
                _ => (0, Value::Object(fields)),
            }
        }
    }
}

/// The payload with the version attached, in the shape this store keeps.
fn wrap(payload: Value, schema: &Schema) -> Value {
    match schema.shape {
        Shape::Beside => match payload {
            Value::Object(mut fields) => {
                fields.insert(VERSION.to_string(), Value::from(schema.version));
                Value::Object(fields)
            }
            // Declaring `Beside` for something that does not serialise as an
            // object is a mistake in the store rather than in the file, and
            // losing the version is better than losing the data.
            other => other,
        },
        Shape::Around => {
            let mut fields = Map::new();
            fields.insert(VERSION.to_string(), Value::from(schema.version));
            fields.insert(ITEMS.to_string(), payload);
            Value::Object(fields)
        }
    }
}

/// Deals with a file that could not be read, and hands back the default.
fn give_up<T: Default>(path: &Path, schema: &Schema, why: &str) -> T {
    match schema.unreadable {
        Unreadable::KeepAside => {
            crate::say!("{} could not be read, keeping it aside: {why}", schema.what);
            let _ = std::fs::rename(path, path.with_extension("json.broken"));
        }
        Unreadable::Overwrite => {
            crate::say!(
                "{} could not be read and will be rebuilt: {why}",
                schema.what
            );
        }
    }

    T::default()
}

fn as_version(found: Value) -> Option<u32> {
    found.as_u64().and_then(|number| u32::try_from(number).ok())
}

const VERSION: &str = "version";
const ITEMS: &str = "items";

/// The same contract, for the two files that are databases rather than JSON.
///
/// Here rather than in a module of its own, because there is one version
/// contract and two implementations of it, and putting them in different files
/// is how the two halves quietly stop agreeing about what a newer file means.
///
/// SQLite keeps a `user_version` in its header for exactly this and Sill left
/// it at zero on both databases, so a schema change had nowhere to be
/// recorded. The clipboard history in particular is migrated with
/// `CREATE TABLE IF NOT EXISTS` and `ALTER TABLE`, which say nothing about a
/// column that already exists under a changed meaning.
///
/// Refused rather than kept aside, which is the one place the two halves
/// differ, and for a reason rather than an oversight. A live database has a
/// write-ahead log and a shared-memory file beside it, so renaming the one
/// file leaves the other two describing a database that is no longer there.
/// Nothing is torn here either: the point of moving a JSON file aside is that
/// the next save would otherwise overwrite it, and refusing to open means
/// there is no next save.
///
/// Read **before** any migration runs. Checking afterwards would mean the DDL
/// had already touched a database this build does not understand.
pub fn refuse_a_newer_database(
    connection: &rusqlite::Connection,
    version: u32,
    what: &str,
) -> rusqlite::Result<()> {
    let found: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if found > version {
        // `SQLITE_NOTADB` is the closest thing SQLite has to "this file is not
        // one I can work with", which is exactly the situation. The message
        // beside it is what actually reaches the log.
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_NOTADB),
            Some(format!(
                "{what} is version {found} and this build understands \
                 {version}, so it was left alone"
            )),
        ));
    }

    Ok(())
}

/// Records which schema version this build left the database at.
///
/// Runs after the migration rather than with it, so a version is only claimed
/// once the shape it names is actually there.
pub fn stamp_database(connection: &rusqlite::Connection, version: u32) -> rusqlite::Result<()> {
    // Not a bindable parameter. `PRAGMA user_version = ?` is a syntax error in
    // SQLite, which is why this is formatted in; `version` is a `u32` from a
    // const in this crate, so there is nothing here to inject.
    connection.pragma_update(None, "user_version", version)
}

/**
One unreadable value, rather than the whole file.

**This has happened twice on a real machine.** A saved `"rightControl"` named
a modifier the enum no longer has, and the entire preferences file was moved
aside over it: every other setting, and every sealed key, gone. What the person
reports is that their settings reverted, and nothing on screen connects that to
one word in one field.

The shape of the danger is that **every enum in a settings file is a promise
about a closed set that a future build may reopen.** Rename a variant, drop
one, or read a file a newer build wrote, and serde refuses the document rather
than the field. A file is worth more than any one value in it.

So an enum field written with this takes its default when the saved value is
not one this build knows, and says so in the log rather than silently. That is
the right trade for a setting with a safe fallback: a theme going back to the
default is visible and recoverable, and losing every sealed key is not.

**Not for everything.** A field where a wrong value would be worse than no file
should refuse instead, which is why this is opt-in per field rather than
applied to the whole document.
*/
pub fn forgiving<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = Value::deserialize(deserializer)?;
    let shown = value.to_string();

    match serde_json::from_value::<T>(value) {
        Ok(known) => Ok(known),
        Err(why) => {
            // Said rather than swallowed. A setting that quietly reverts is a
            // bug report nobody can act on.
            crate::say!(
                "{shown} is not a value this build knows ({why}), so that one \
                 setting went back to its default"
            );
            Ok(T::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(default)]
    struct Settings {
        name: String,
        count: u32,
    }

    const BESIDE: Schema = Schema {
        version: 2,
        shape: Shape::Beside,
        layout: Layout::Readable,
        unreadable: Unreadable::KeepAside,
        what: "the test settings",
    };

    const AROUND: Schema = Schema {
        version: 2,
        shape: Shape::Around,
        layout: Layout::Compact,
        unreadable: Unreadable::KeepAside,
        what: "the test list",
    };

    const CACHE: Schema = Schema {
        version: 2,
        shape: Shape::Around,
        layout: Layout::Compact,
        unreadable: Unreadable::Overwrite,
        what: "the test cache",
    };

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temp dir")
    }

    #[test]
    fn a_missing_file_is_the_default_and_is_left_alone() {
        let dir = dir();
        let path = dir.path().join("absent.json");

        assert_eq!(load::<Settings>(&path, &BESIDE), Settings::default());
        assert!(
            !path.with_extension("json.broken").exists(),
            "a first run is not a failure and must not leave a broken file"
        );
    }

    #[test]
    fn what_was_saved_is_what_comes_back() {
        let dir = dir();
        let path = dir.path().join("settings.json");

        let saved = Settings {
            name: "a name".into(),
            count: 7,
        };
        save_atomic(&path, &saved, &BESIDE).expect("saves");

        assert_eq!(load::<Settings>(&path, &BESIDE), saved);
    }

    /// The version has to reach the bytes, or none of the rest means anything.
    ///
    /// Asserted against the file rather than a round trip, which would pass
    /// just as happily if no version were ever written: an absent one reads
    /// back as zero, and zero is accepted.
    #[test]
    fn the_version_this_build_writes_is_in_the_file() {
        let dir = dir();

        let beside = dir.path().join("settings.json");
        save_atomic(&beside, &Settings::default(), &BESIDE).expect("saves");
        let written = std::fs::read_to_string(&beside).expect("readable");
        assert_eq!(
            serde_json::from_str::<Value>(&written).expect("valid")["version"],
            2,
            "nothing was stamped, so a later build has nothing to refuse:\n{written}"
        );

        let around = dir.path().join("list.json");
        save_atomic(&around, &vec!["one"], &AROUND).expect("saves");
        let written = std::fs::read_to_string(&around).expect("readable");
        let document: Value = serde_json::from_str(&written).expect("valid");
        assert_eq!(document["version"], 2, "not stamped:\n{written}");
        assert_eq!(document["items"][0], "one", "the payload moved or was lost");
    }

    /// The upgrade path: everything already on disk has no version.
    #[test]
    fn a_file_written_before_versioning_still_reads() {
        let dir = dir();

        let beside = dir.path().join("settings.json");
        std::fs::write(&beside, r#"{"name":"old","count":3}"#).expect("writes");
        assert_eq!(
            load::<Settings>(&beside, &BESIDE),
            Settings {
                name: "old".into(),
                count: 3
            },
            "a versionless object is version zero, not a refusal"
        );

        let around = dir.path().join("list.json");
        std::fs::write(&around, r#"["one","two"]"#).expect("writes");
        assert_eq!(
            load::<Vec<String>>(&around, &AROUND),
            vec!["one".to_string(), "two".to_string()],
            "a bare list is the payload itself, which is every list file today"
        );
    }

    /// A version this build does not understand is refused rather than read
    /// with the old meanings. See the contract in the module note.
    #[test]
    fn a_file_from_a_later_build_is_refused_and_kept() {
        let dir = dir();
        let path = dir.path().join("settings.json");
        let aside = path.with_extension("json.broken");

        std::fs::write(&path, r#"{"version":99,"name":"from the future"}"#).expect("writes");

        assert_eq!(
            load::<Settings>(&path, &BESIDE),
            Settings::default(),
            "fields from a later build must not be read as though they still mean what they did"
        );
        assert!(!path.exists(), "the file was left for the next save to eat");
        assert!(
            std::fs::read_to_string(&aside)
                .expect("kept aside")
                .contains("from the future"),
            "the newer file has to survive somewhere recoverable"
        );
    }

    #[test]
    fn a_wrapped_file_from_a_later_build_is_refused_too() {
        let dir = dir();
        let path = dir.path().join("list.json");

        std::fs::write(&path, r#"{"version":99,"items":["one"]}"#).expect("writes");

        assert!(load::<Vec<String>>(&path, &AROUND).is_empty());
        assert!(path.with_extension("json.broken").exists());
    }

    #[test]
    fn a_file_this_build_wrote_is_read_by_a_build_that_writes_a_later_one() {
        // The other half of the contract: older reads, so shipping a version
        // bump does not orphan the files already written.
        let dir = dir();
        let path = dir.path().join("settings.json");

        let older = Schema {
            version: 1,
            ..BESIDE
        };
        save_atomic(
            &path,
            &Settings {
                name: "kept".into(),
                count: 1,
            },
            &older,
        )
        .expect("saves");

        assert_eq!(load::<Settings>(&path, &BESIDE).name, "kept");
    }

    /// Notepad and PowerShell both write one, and it used to cost the file.
    #[test]
    fn a_file_saved_with_a_byte_order_mark_still_reads() {
        let dir = dir();
        let path = dir.path().join("settings.json");

        std::fs::write(&path, "\u{feff}{\"name\":\"hand edited\"}").expect("writes");

        assert_eq!(
            load::<Settings>(&path, &BESIDE).name,
            "hand edited",
            "three bytes of encoding threw away the whole file"
        );
        assert!(
            !path.with_extension("json.broken").exists(),
            "the file was moved aside over a byte order mark"
        );
    }

    #[test]
    fn an_unreadable_file_is_kept_aside_rather_than_overwritten() {
        let dir = dir();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ not json at all").expect("writes");

        assert_eq!(load::<Settings>(&path, &BESIDE), Settings::default());

        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.broken")).expect("kept aside"),
            "{ not json at all",
            "what could not be read has to still be on disk"
        );
    }

    /// A cache is rebuilt, so keeping copies of a broken one is pure waste.
    #[test]
    fn an_unreadable_cache_is_left_for_the_next_save() {
        let dir = dir();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, "{ torn").expect("writes");

        assert!(load::<Vec<String>>(&path, &CACHE).is_empty());
        assert!(
            !path.with_extension("json.broken").exists(),
            "a derived cache does not get kept aside"
        );
    }

    #[test]
    fn saving_leaves_no_staging_file_behind() {
        let dir = dir();
        let path = dir.path().join("settings.json");

        save_atomic(&path, &Settings::default(), &BESIDE).expect("saves");

        assert!(path.is_file());
        assert!(!path.with_extension("json.partial").exists());
    }

    #[test]
    fn the_directory_is_made_if_it_is_not_there() {
        let dir = dir();
        let path = dir.path().join("not").join("yet").join("settings.json");

        save_atomic(&path, &Settings::default(), &BESIDE).expect("saves");

        assert!(path.is_file());
    }

    /// The store's own fields must not collide with the version.
    #[test]
    fn the_version_is_not_handed_to_the_store_as_a_field() {
        #[derive(Debug, Default, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Strict {
            name: String,
        }

        let dir = dir();
        let path = dir.path().join("strict.json");
        std::fs::write(&path, r#"{"version":1,"name":"kept"}"#).expect("writes");

        assert_eq!(
            load::<Strict>(&path, &BESIDE).name,
            "kept",
            "the version has to be removed before the store sees the document"
        );
    }

    #[test]
    fn one_unreadable_entry_costs_that_entry_and_not_the_list() {
        #[derive(Debug, Deserialize)]
        struct Needs {
            id: String,
        }

        #[derive(Debug, Default, Deserialize)]
        #[serde(default)]
        struct Holder {
            #[serde(deserialize_with = "entries_that_can_be_read")]
            all: Vec<Needs>,
        }

        let json = r#"{"all":[{"id":"one"},{},{"id":"three"}]}"#;
        let held: Holder = serde_json::from_str(json).expect("the list still reads");

        assert_eq!(held.all.len(), 2, "the readable entries survive");
        assert_eq!(held.all[0].id, "one");
        assert_eq!(held.all[1].id, "three");
    }

    /// Nothing overwrites the file when the value cannot be serialised.
    ///
    /// The eight all wrote `{}` or `[]` in this case, which destroys the file
    /// on purpose in the name of not failing.
    #[test]
    fn a_value_that_cannot_be_serialised_leaves_the_file_alone() {
        use std::collections::HashMap;

        let dir = dir();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"name":"still here"}"#).expect("writes");

        // A map keyed by something that is not a string has no JSON shape.
        let impossible: HashMap<(u8, u8), u8> = HashMap::from([((1, 2), 3)]);

        assert!(save_atomic(&path, &impossible, &BESIDE).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).expect("still there"),
            r#"{"name":"still here"}"#,
            "a failed serialise emptied the file"
        );
    }
}
