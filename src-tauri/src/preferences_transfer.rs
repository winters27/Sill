//! Getting Sill's whole settings in and out of a file, and back to defaults.
//!
//! Three jobs that look separate and share one rule, which is why they are in
//! one place: **nothing here writes anything.** Every function takes what is
//! held now and hands back what would replace it, or the reason it cannot.
//! The caller decides whether to save. That is not tidiness. It is the entire
//! safety property, and the only way to state it once rather than hope three
//! call sites each remembered it.
//!
//! # An export is a file somebody sends to somebody
//!
//! `preferences.json` holds credentials. They are sealed with DPAPI, which
//! binds them to one Windows account on one machine, so a sealed value in an
//! export is two failures at once: a credential leaving the machine, and a
//! credential that would not work when it arrived. **An export carries no
//! secret in either form.** The paths are taken from `preferences::SEALED`, so
//! a credential added there is left out of an export without anybody having to
//! remember this file exists, and `no_credential_reaches_an_export` fails if
//! one ever does.
//!
//! The file says what it withheld. Somebody restoring a backup and finding
//! their AI provider silent deserves to be told why in the file itself rather
//! than to work it out.
//!
//! # An import must not be able to destroy a working setup
//!
//! An import is the one operation a person runs once, on a file they did not
//! write, expecting it to help. So the failure mode has to be "nothing
//! happened", never "everything is gone":
//!
//! - Reading, merging and deserialising all happen before anything is saved.
//!   A file that is truncated, from a newer build, or not a settings file at
//!   all produces an `Err` and no write, so what was on disk is still there
//!   and still complete.
//! - A section the file does not mention keeps what it has. A foreign format
//!   knows nothing about most of Sill, and an import that filled the rest in
//!   from defaults would be a reset wearing an import's name.
//! - A credential the file does not carry keeps the one already here. Every
//!   export deliberately omits them, so without this rule importing your own
//!   backup would clear the keys the backup was protecting.
//!
//! # A reset resets one panel
//!
//! `PANELS` names, for each settings panel, the top-level keys of the
//! preferences document that panel is the whole of. A reset replaces exactly
//! those and copies the rest across untouched, so resetting Appearance cannot
//! reach Shortcuts. `every_section_belongs_to_one_panel` fails if a new
//! section is added and left out of the table, which is the shape of the bug:
//! a section nobody assigned is a section no reset can reach, and a section
//! assigned twice is one two panels each quietly reset.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::preferences::Preferences;

/// The preference sections one settings panel owns.
pub struct Panel {
    /// What `settings_index::PANELS` calls this panel, which is also its deep
    /// link and the id the window sends.
    pub id: &'static str,
    /// The top-level keys of the preferences document this panel is the whole
    /// of, in the spelling the document uses.
    pub sections: &'static [&'static str],
}

/// Which settings panel owns which parts of the preferences.
///
/// The panels with nothing of their own are not here, and their absence is the
/// answer rather than an omission: Quicklinks and Snippets keep their entries in
/// their own stores, Advanced offers actions rather than settings, and About
/// is text. Offering "reset" on a panel with nothing to reset would be a
/// button that does nothing.
///
/// Shortcuts owns seven sections because it is genuinely one screen for every
/// key Sill answers to, from the summon hotkey down to per-command aliases.
/// Splitting it here so the table looked neater would mean "reset Shortcuts"
/// left half the keys set.
pub const PANELS: &[Panel] = &[
    Panel {
        id: "general",
        // Private mode is here because it is a fact about the machine rather
        // than about any one feature: it overrides the clipboard, dictation
        // and capture settings at once, and filing it under any of those three
        // would mean resetting that panel decided whether Sill was recording
        // the other two.
        sections: &["general", "privacy"],
    },
    Panel {
        id: "appearance",
        sections: &["appearance"],
    },
    Panel {
        id: "snippets",
        sections: &["snippets"],
    },
    Panel {
        id: "clipboard",
        sections: &["clipboard"],
    },
    Panel {
        id: "emoji",
        sections: &["emoji"],
    },
    Panel {
        id: "shortcuts",
        sections: &[
            "hotkey",
            "bindings",
            "aliases",
            "taps",
            "hyper",
            "actionKeys",
            "navigation",
            // Window layouts are edited under Shortcuts, beside the keys
            // that send windows to them.
            "layouts",
        ],
    },
    Panel {
        id: "screenshot",
        sections: &["screenshot"],
    },
    Panel {
        id: "sources",
        sections: &["sources", "browsers", "webSearch"],
    },
    Panel {
        id: "files",
        sections: &["files"],
    },
    Panel {
        id: "extensions",
        sections: &["store"],
    },
    Panel {
        id: "scripts",
        sections: &["scripts"],
    },
    Panel {
        id: "ai",
        sections: &["ai"],
    },
    Panel {
        id: "dictation",
        sections: &["dictation"],
    },
    Panel {
        id: "tts",
        sections: &["tts"],
    },
    Panel {
        id: "widgets",
        sections: &["widgets"],
    },
    Panel {
        id: "mcp",
        sections: &["mcp"],
    },
];

/// Marks a file as one of Sill's own exports.
const KIND: &str = "sill-preferences";

/// The shape of an export, which is deliberately not the shape of the store.
///
/// Its own number rather than the store's, for the reason `snippets::transfer`
/// keeps its own format: the file people carry between machines and the file
/// Sill keeps on disk answer to different pressures, and tying them together
/// means one cannot change without the other.
const FORMAT: u32 = 1;

/// What a chosen file turns out to be.
///
/// Decided on the bytes rather than on the extension, because the extension is
/// what a person renaming a backup changes and the contents are not. It also
/// means a `.rayconfig` saved as `.zip`, which is what a browser sometimes
/// does with a download, still reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A zip, which is what a `.rayconfig` is.
    Archive,
    /// PowerToys Run's own settings, whose plugin folders sit beside it.
    PowerToysRun,
    /// A Sill export, a `preferences.json`, or nothing Sill knows. Which of
    /// the three is `from_json`'s answer rather than this one's.
    Json,
}

/// Which reader a chosen file should go to.
pub fn kind(file: &[u8]) -> Kind {
    // The local file header every zip begins with.
    if file.starts_with(b"PK\x03\x04") {
        return Kind::Archive;
    }

    let text = String::from_utf8_lossy(file);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);

    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(fields)) if is_power_toys(&fields) => Kind::PowerToysRun,
        _ => Kind::Json,
    }
}

/// What an import worked out, so it can be said rather than guessed at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// The settings sections the file had something to say about.
    ///
    /// Named rather than counted, because "it changed 4 sections" tells
    /// somebody nothing they can check and "appearance, hotkey" does.
    pub sections: Vec<String>,
    /// Credentials the file did not carry, so the ones already here were kept.
    pub kept_keys: usize,
}

/// Sill's settings as a file to keep, send, or read back on another machine.
///
/// **Every credential is left out**, in both the plain form this holds in
/// memory and the sealed form the store file holds. See the module note.
pub fn to_json(prefs: &Preferences) -> Result<String, String> {
    let mut payload = serde_json::to_value(prefs)
        .map_err(|why| format!("Sill could not read its own settings: {why}"))?;

    let withheld = strip_secrets(&mut payload);

    let document = serde_json::json!({
        "sill": KIND,
        "format": FORMAT,
        // Which build wrote it, for somebody looking at a file that will not
        // import. Not read back: the format number is what decides that.
        "app": env!("CARGO_PKG_VERSION"),
        // The file says what it did not carry, in the same dotted spelling
        // `preferences::SEALED` uses, so the answer to "why is my key gone"
        // is in the file rather than in somebody's memory of this decision.
        "withheld": withheld,
        "preferences": payload,
    });

    serde_json::to_string_pretty(&document)
        .map_err(|why| format!("Sill could not write that file: {why}"))
}

/// Reads a Sill export or a `preferences.json`, as the sections it describes.
///
/// Both, because the two files people actually have are an export and a backup
/// of the store, and refusing the second would mean the careful thing Sill
/// does with its own file, keeping an unreadable one aside, produced something
/// nothing could then read back.
pub fn from_json(text: &str) -> Result<Value, String> {
    // A byte order mark is not part of the JSON, and Windows puts one on
    // anything Notepad saved as UTF-8. The same reasoning as `json_store`,
    // which is where a file Sill wrote would have hit this first.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let document: Value = serde_json::from_str(text)
        .map_err(|why| format!("that file is not settings Sill can read: {why}"))?;

    let Some(fields) = document.as_object() else {
        return Err("that file is not settings Sill can read".to_string());
    };

    if fields.get("sill").and_then(Value::as_str) == Some(KIND) {
        let format = fields.get("format").and_then(Value::as_u64).unwrap_or(0);

        // Refused rather than read hopefully. A later build's export may spell
        // a setting differently under the same name, and reading it as though
        // it still means what it used to is how a working setup is quietly
        // changed instead of loudly left alone.
        if format > u64::from(FORMAT) {
            return Err(format!(
                "that file was exported by a newer Sill (format {format}, this build \
                 understands {FORMAT}). Update Sill and try again."
            ));
        }

        return match fields.get("preferences") {
            Some(Value::Object(sections)) => Ok(Value::Object(sections.clone())),
            _ => Err("that export has no settings in it".to_string()),
        };
    }

    if is_power_toys(fields) {
        return Err("that looks like PowerToys Run's settings file. Choose \
                    PowerToysRunSettings.json and Sill will read the plugins beside it."
            .to_string());
    }

    // A store file, then: sections at the top level, and a version beside
    // them. Judged by whether it names a section Sill has rather than by the
    // file's name, so a copy called `preferences-backup.json` still reads.
    if !fields.keys().any(|key| known_section(key)) {
        return Err("that file has no Sill settings in it".to_string());
    }

    let version = fields.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version > u64::from(crate::preferences::SCHEMA.version) {
        return Err(format!(
            "that settings file was written by a newer Sill (version {version}, this \
             build understands {}). Update Sill and try again.",
            crate::preferences::SCHEMA.version
        ));
    }

    let mut sections = fields.clone();
    sections.remove("version");
    Ok(Value::Object(sections))
}

/// Folds what a file describes into the settings held now.
///
/// Pure, and the whole of the policy. The preferences handed in are not
/// touched and nothing reaches disk; the answer is what the caller may save,
/// or the reason there is nothing safe to save. A file that cannot be turned
/// into a complete `Preferences` ends here, which is what makes "the import
/// failed" mean "nothing changed".
pub fn apply(current: &Preferences, patch: &Value) -> Result<(Preferences, Summary), String> {
    let Some(incoming) = patch.as_object() else {
        return Err("that file has no Sill settings in it".to_string());
    };

    let mut sections: Vec<String> = incoming
        .keys()
        .filter(|key| known_section(key))
        .cloned()
        .collect();

    if sections.is_empty() {
        return Err("that file has no Sill settings in it".to_string());
    }

    sections.sort();

    let mut arriving = Value::Object(incoming.clone());

    /*
     * Opened before merging, not after.
     *
     * A `preferences.json` copied off this machine holds sealed credentials.
     * On the machine that sealed them this turns them back into keys, which is
     * what makes restoring your own backup restore a working setup. On any
     * other machine they cannot be opened, and `unseal_secrets` clears them
     * rather than passing a blob through to be sent to a provider as a key.
     * Either way what is left is a key or nothing, and "nothing" is exactly
     * what `restore_keys` below fills from what is already here.
     */
    crate::preferences::unseal_secrets(&mut arriving);

    // Counted from what arrived, before folding turns it into the answer. The
    // number is a fact about the file rather than about how the repair below
    // happens to be implemented, and those are two different things: most
    // sections keep their credential simply because `fold` walks into an
    // object rather than replacing it, and the repair only has work to do
    // where the file replaced a whole list.
    let kept_keys = keys_the_file_did_not_carry(current, &arriving);

    let mut merged = serde_json::to_value(current)
        .map_err(|why| format!("Sill could not read its own settings: {why}"))?;

    fold(&mut merged, arriving);

    restore_keys(current, &mut merged);

    let next: Preferences = serde_json::from_value(merged)
        .map_err(|why| format!("Sill could not apply that file, so nothing changed: {why}"))?;

    Ok((
        next,
        Summary {
            sections,
            kept_keys,
        },
    ))
}

/// One settings panel back to what it shipped with, and nothing else.
///
/// Built by copying the panel's own sections out of a fresh `Preferences` and
/// leaving every other key of the document exactly as it was, rather than by
/// assigning fields. A field added to a section is covered the day it is
/// added; a hand-written reset would go on resetting the fields somebody
/// remembered in the year the panel was written.
pub fn reset(prefs: &Preferences, panel: &str) -> Result<Preferences, String> {
    let Some(found) = PANELS.iter().find(|one| one.id == panel) else {
        return Err(format!("there is nothing to reset in {panel}"));
    };

    let mut current = serde_json::to_value(prefs)
        .map_err(|why| format!("Sill could not read its own settings: {why}"))?;
    let fresh = serde_json::to_value(Preferences::default())
        .map_err(|why| format!("Sill could not read its own defaults: {why}"))?;

    let (Some(current_fields), Some(fresh_fields)) = (current.as_object_mut(), fresh.as_object())
    else {
        return Err("Sill could not read its own settings".to_string());
    };

    for section in found.sections {
        match fresh_fields.get(*section) {
            Some(value) => {
                current_fields.insert((*section).to_string(), value.clone());
            }
            // A section named here that a fresh `Preferences` does not have is
            // a mistake in the table above rather than in the file, and the
            // test below catches it. Removing the key is still the honest
            // answer to "put this back the way it started".
            None => {
                current_fields.remove(*section);
            }
        }
    }

    serde_json::from_value(current)
        .map_err(|why| format!("Sill could not apply that reset, so nothing changed: {why}"))
}

/// Whether a name is one of the top-level keys the preferences document has.
///
/// Answered from `PANELS` rather than from a second list, so there is one
/// place a section is named and the test below ties it to the document itself.
fn known_section(name: &str) -> bool {
    PANELS.iter().any(|panel| panel.sections.contains(&name))
}

/// Takes every credential out of a document, naming the ones that held something.
///
/// Removed rather than emptied. An absent key is a file saying nothing about a
/// credential, which is what lets an import keep the one already there; an
/// empty string would be the file saying "there is no key here", and importing
/// it would clear a working one.
fn strip_secrets(document: &mut Value) -> Vec<String> {
    let mut withheld = Vec::new();

    for path in crate::preferences::SEALED {
        let taken = take(document, path);

        if taken.iter().any(|value| holds_something(value)) {
            withheld.push(path.join("."));
        }
    }

    withheld
}

/// Whether a slot from a document carries a credential rather than nothing.
fn holds_something(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}

/// Follows a sealed path and removes what it leads to, handing back the values.
///
/// The mirror of `preferences::at`, which borrows the slots in place. Removing
/// cannot be done through a borrow of the value, so this walks the same way
/// and takes the leaf out of the object that holds it. `*` yields one leaf per
/// element, which is what makes a list of AI providers with a key each behave
/// the same as a single fixed path.
fn take(root: &mut Value, path: &[&str]) -> Vec<Value> {
    let Some((step, rest)) = path.split_first() else {
        return Vec::new();
    };

    if *step == crate::preferences::EACH {
        let Some(items) = root.as_array_mut() else {
            return Vec::new();
        };

        return items.iter_mut().flat_map(|item| take(item, rest)).collect();
    }

    if rest.is_empty() {
        return root
            .as_object_mut()
            .and_then(|fields| fields.remove(*step))
            .into_iter()
            .collect();
    }

    match root.get_mut(step) {
        Some(node) => take(node, rest),
        None => Vec::new(),
    }
}

/// Merges what a file said over what is held, one object at a time.
///
/// Objects are walked into, so a file naming one field of Appearance changes
/// that field and leaves the rest of Appearance alone. Everything else,
/// including every list, replaces: a file carrying five shortcuts means those
/// five, and merging lists element by element would produce a set nobody wrote
/// and nobody could predict.
fn fold(into: &mut Value, from: Value) {
    match (into, from) {
        (Value::Object(here), Value::Object(there)) => {
            for (key, value) in there {
                match here.get_mut(&key) {
                    Some(slot) => fold(slot, value),
                    None => {
                        here.insert(key, value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}

/// How many credentials the arriving file said nothing about.
///
/// A statement about the file, so it can be put on screen as one: "four keys
/// were not in that file, and the ones already here were kept". Worked out
/// from what arrived rather than from what changed, because a credential that
/// survives because `fold` never touched it and one that had to be put back
/// are the same fact to the person reading it.
fn keys_the_file_did_not_carry(current: &Preferences, arriving: &Value) -> usize {
    let Ok(mut held) = serde_json::to_value(current) else {
        return 0;
    };

    let mut missing = 0;

    for path in crate::preferences::SEALED {
        if path.contains(&crate::preferences::EACH) {
            continue;
        }

        if !take(&mut held, path).iter().any(holds_something) {
            continue;
        }

        if !at_read(arriving, path).is_some_and(holds_something) {
            missing += 1;
        }
    }

    // The providers are their own question, because the file may carry a list
    // in a different order, or no list at all, and each answer means something
    // different for each provider in it.
    let listed = arriving.get("ai").and_then(|ai| ai.get("providers"));

    for provider in &current.ai.providers {
        if provider.api_key.is_empty() {
            continue;
        }

        let carried = listed
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|one| {
                one.get("id").and_then(Value::as_str) == Some(provider.id.as_str())
                    && one.get("apiKey").is_some_and(holds_something)
            });

        if !carried {
            missing += 1;
        }
    }

    missing
}

/// Puts back every credential the merged document ended up without.
///
/// Every export leaves credentials out on purpose, so without this importing
/// your own backup would clear the keys the backup was written to protect. The
/// rule is the narrow one: a credential is restored only where the merged
/// document has none, so a file that genuinely carries a key still wins.
fn restore_keys(current: &Preferences, merged: &mut Value) {
    let Ok(mut held) = serde_json::to_value(current) else {
        return;
    };

    for path in crate::preferences::SEALED {
        if path.contains(&crate::preferences::EACH) {
            continue;
        }

        let Some(existing) = take(&mut held, path).into_iter().find(holds_something) else {
            continue;
        };

        // A path the merged document does not reach at all, because the file
        // replaced the object that held it with one having no such key.
        if !reaches(merged, path) {
            plant(merged, path, existing);
            continue;
        }

        for slot in crate::preferences::at(merged, path) {
            if holds_something(slot) {
                continue;
            }

            *slot = existing.clone();
        }
    }

    restore_provider_keys(current, merged);
}

/// Puts back the key of every AI provider that arrived without one.
///
/// Its own function because the starred path is the one that cannot be treated
/// as a slot: a file carrying the providers as a list replaces the whole list,
/// so every key in it is gone rather than merely blank. Which key belongs
/// where is decided by the provider's id, because a file listing the same
/// services in another order would otherwise be handed the wrong keys in the
/// right shape, which is worse than no keys at all.
fn restore_provider_keys(current: &Preferences, merged: &mut Value) {
    let Some(providers) = merged
        .get_mut("ai")
        .and_then(|ai| ai.get_mut("providers"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };

    for provider in providers {
        let Some(fields) = provider.as_object_mut() else {
            continue;
        };

        if fields.get("apiKey").is_some_and(holds_something) {
            continue;
        }

        let id = fields.get("id").and_then(Value::as_str).unwrap_or_default();
        let Some(existing) = current
            .ai
            .providers
            .iter()
            .find(|held| held.id == id && !held.api_key.is_empty())
        else {
            continue;
        };

        fields.insert("apiKey".to_string(), Value::from(existing.api_key.clone()));
    }
}

/// What a path leads to in a document nobody may change.
///
/// The read-only half of `preferences::at`, which borrows mutably and so
/// cannot be asked about a document that is only being compared against.
fn at_read<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut here = root;

    for step in path {
        here = here.get(step)?;
    }

    Some(here)
}

/// Whether a document has the key a path names, whatever it holds.
fn reaches(root: &Value, path: &[&str]) -> bool {
    let mut here = root;

    for step in path {
        match here.get(step) {
            Some(node) => here = node,
            None => return false,
        }
    }

    true
}

/// Puts a value where a path says, creating the objects on the way.
fn plant(root: &mut Value, path: &[&str], value: Value) {
    let Some((step, rest)) = path.split_first() else {
        return;
    };

    let Some(fields) = root.as_object_mut() else {
        return;
    };

    if rest.is_empty() {
        fields.insert((*step).to_string(), value);
        return;
    }

    let node = fields
        .entry((*step).to_string())
        .or_insert_with(|| Value::Object(Map::new()));

    plant(node, rest, value);
}

/// Whether a document is PowerToys Run's own settings file.
///
/// Two keys rather than one. `Hotkey` alone is a word plenty of files use;
/// `MaxResultsToShow` beside it is PowerToys Run's and nothing else's.
fn is_power_toys(fields: &Map<String, Value>) -> bool {
    fields.contains_key("Hotkey") && fields.contains_key("MaxResultsToShow")
}

/// PowerToys Run's settings, as the files they are spread across.
///
/// Read as text rather than as paths so the mapping below can be checked
/// against real files without one being installed. The plugin settings are
/// optional because PowerToys writes each one only once its plugin has run.
pub struct PowerToys<'a> {
    /// `PowerToys Run/Settings/PowerToysRunSettings.json`.
    pub settings: &'a str,
    /// `Plugins/Microsoft.Plugin.Program/ProgramPluginSettings.json`.
    pub programs: Option<&'a str>,
    /// `Plugins/Microsoft.Plugin.Folder/FolderSettings.json`.
    pub folders: Option<&'a str>,
}

/// What one PowerToys Run install says, in Sill's own settings.
///
/// **Only what maps without guessing.** PowerToys Run has settings Sill has no
/// equivalent for and Sill has settings PowerToys Run never had, and inventing
/// a correspondence for either would produce a launcher configured by
/// arithmetic rather than by the person. A theme number, for instance, is left
/// alone: the two applications do not have the same themes, and picking one
/// would change how Sill looks on the strength of a guess.
pub fn from_power_toys(files: &PowerToys<'_>) -> Result<Value, String> {
    let settings: Value = serde_json::from_str(files.settings.trim_start_matches('\u{feff}'))
        .map_err(|why| format!("that PowerToys Run settings file could not be read: {why}"))?;

    let Some(fields) = settings.as_object() else {
        return Err("that is not a PowerToys Run settings file".to_string());
    };

    if !is_power_toys(fields) {
        return Err("that is not a PowerToys Run settings file".to_string());
    }

    let mut hotkey = Map::new();
    let mut appearance = Map::new();

    if let Some(chord) = fields.get("Hotkey").and_then(Value::as_str) {
        if let Some(accelerator) = accelerator(chord) {
            hotkey.insert("summon".to_string(), Value::from(accelerator));
        }
    }

    if let Some(hide) = fields.get("HideWhenDeactivated").and_then(Value::as_bool) {
        hotkey.insert("dismissOnBlur".to_string(), Value::from(hide));
    }

    if let Some(clear) = fields.get("ClearInputOnLaunch").and_then(Value::as_bool) {
        hotkey.insert("resetOnSummon".to_string(), Value::from(clear));
    }

    // Clamped to what Sill's window can be, which is the same range the
    // Appearance slider offers. PowerToys allows fewer rows than Sill's
    // smallest window, and a value outside the range would be silently
    // clamped later anyway; doing it here means the number that arrives is
    // the number the panel shows.
    if let Some(rows) = fields.get("MaxResultsToShow").and_then(Value::as_u64) {
        appearance.insert("visibleRows".to_string(), Value::from(rows.clamp(4, 16)));
    }

    let mut patch = Map::new();

    if !hotkey.is_empty() {
        patch.insert("hotkey".to_string(), Value::Object(hotkey));
    }

    if !appearance.is_empty() {
        patch.insert("appearance".to_string(), Value::Object(appearance));
    }

    if let Some(sources) = program_sources(files.programs) {
        patch.insert("sources".to_string(), Value::Object(sources));
    }

    if let Some(roots) = folder_roots(files.folders) {
        patch.insert("files".to_string(), Value::Object(roots));
    }

    if patch.is_empty() {
        return Err("there was nothing in that PowerToys Run install Sill could use".to_string());
    }

    Ok(Value::Object(patch))
}

/// Which places PowerToys Run was told to look for programs, as Sill's switches.
///
/// Sill has one switch where PowerToys has two: the Start Menu and the Desktop
/// are one source here, so it is on if either was. Turning it off because only
/// one of the two was on would lose somebody half their applications on the
/// strength of a shape difference.
fn program_sources(text: Option<&str>) -> Option<Map<String, Value>> {
    let parsed: Value = serde_json::from_str(text?.trim_start_matches('\u{feff}')).ok()?;
    let fields = parsed.as_object()?;

    let flag = |name: &str| fields.get(name).and_then(Value::as_bool);

    let mut sources = Map::new();

    match (flag("EnableStartMenuSource"), flag("EnableDesktopSource")) {
        (None, None) => {}
        (start, desktop) => {
            let on = start.unwrap_or(false) || desktop.unwrap_or(false);
            sources.insert("shortcuts".to_string(), Value::from(on));
        }
    }

    // PowerToys' "registry source" is the App Paths key, which is the same
    // thing Sill calls registered executables and the same thing the Run
    // dialog resolves.
    if let Some(on) = flag("EnableRegistrySource") {
        sources.insert("appPaths".to_string(), Value::from(on));
    }

    if let Some(on) = flag("EnablePathEnvironmentVariableSource") {
        sources.insert("pathExecutables".to_string(), Value::from(on));
    }

    (!sources.is_empty()).then_some(sources)
}

/// The folders PowerToys Run was pointed at, as folders Sill indexes.
fn folder_roots(text: Option<&str>) -> Option<Map<String, Value>> {
    let parsed: Value = serde_json::from_str(text?.trim_start_matches('\u{feff}')).ok()?;

    let roots: Vec<Value> = parsed
        .get("FolderLinks")?
        .as_array()?
        .iter()
        .filter_map(|link| link.get("Path").and_then(Value::as_str))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(Value::from)
        .collect();

    if roots.is_empty() {
        return None;
    }

    let mut files = Map::new();
    files.insert("roots".to_string(), Value::Array(roots));
    Some(files)
}

/// PowerToys' spelling of a chord in the spelling Sill's hotkeys use.
///
/// PowerToys writes `Alt + Space` with the spaces and Sill writes `Alt+Space`
/// without, and the accelerator goes straight to the shortcut registration, so
/// the spaces are not cosmetic. A chord naming a modifier neither application
/// recognises is refused rather than half-translated: leaving the summon key
/// alone is recoverable, and setting it to something that will not register
/// is a launcher with no way to open it.
fn accelerator(chord: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    for part in chord.split('+') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }

        let named = match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => "Ctrl",
            "alt" => "Alt",
            "shift" => "Shift",
            "win" | "super" | "meta" => "Super",
            _ => {
                parts.push(part.to_string());
                continue;
            }
        };

        parts.push(named.to_string());
    }

    // A lone modifier is not a chord anything can register.
    if parts.len() < 2 {
        return None;
    }

    Some(parts.join("+"))
}

/// The parts of a `.rayconfig` Sill has somewhere to put.
///
/// Snippets and quicklinks, and nothing else, and the emptiness of the rest is
/// the honest answer rather than a gap. Raycast's archive holds a good deal
/// Sill has no equivalent for, and a mapping invented from the shape of a key
/// name would configure somebody's launcher on a guess. What is here is what
/// the tolerant readers in `snippets::transfer` and `quicklinks::transfer`
/// already understand, which is the same JSON another tool would write.
#[derive(Debug, Default)]
pub struct RayConfig {
    pub snippets: Vec<crate::snippets::store::Snippet>,
    pub quicklinks: Vec<crate::quicklinks::store::Quicklink>,
}

/// Reads a `.rayconfig`, which is a zip of JSON.
///
/// Every member is offered to both readers and kept where it is understood,
/// rather than matched by file name. Names inside an archive are the part
/// most likely to change between versions, and a reader that already decides
/// what it can use from the contents needs no help from one.
///
/// An archive that is not a zip is refused with a sentence rather than read as
/// nothing, because that is what an encrypted export looks like and "nothing
/// happened" would leave somebody clicking the button again.
pub fn from_rayconfig(archive: &[u8]) -> Result<RayConfig, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(|why| {
        format!(
            "that file is not an archive Sill can open, so it may be an encrypted \
             export: {why}"
        )
    })?;

    let mut found = RayConfig::default();

    for index in 0..zip.len() {
        let Ok(mut entry) = zip.by_index(index) else {
            continue;
        };

        if entry.is_dir() {
            continue;
        }

        let mut text = String::new();
        if std::io::Read::read_to_string(&mut entry, &mut text).is_err() {
            continue;
        }

        if let Ok(snippets) = crate::snippets::transfer::parse(&text) {
            if !snippets.is_empty() {
                found.snippets.extend(snippets);
                continue;
            }
        }

        if let Ok(links) = crate::quicklinks::transfer::parse(&text) {
            found.quicklinks.extend(links);
        }
    }

    if found.snippets.is_empty() && found.quicklinks.is_empty() {
        return Err(
            "there were no snippets or quicklinks in that file that Sill could read".to_string(),
        );
    }

    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every top-level key of the preferences document, as serde writes them.
    fn sections_of(prefs: &Preferences) -> Vec<String> {
        serde_json::to_value(prefs)
            .expect("preferences serialise")
            .as_object()
            .expect("preferences are an object")
            .keys()
            .cloned()
            .collect()
    }

    /// One section of a preferences document, for comparing two of them.
    fn section(prefs: &Preferences, name: &str) -> Value {
        serde_json::to_value(prefs)
            .expect("preferences serialise")
            .get(name)
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// A key in every place `preferences::SEALED` names one.
    ///
    /// Written by hand rather than derived, so that a credential added to
    /// `SEALED` and not to this list shows up as a test that no longer covers
    /// it rather than as one that quietly passes.
    const KEYS: &[&str] = &[
        "sk-dictation-0000",
        "sk-provider-1111",
        "sk-tts-2222",
        "ghp_store3333",
    ];

    /// Preferences with a credential in every sealed slot.
    fn with_every_key() -> Preferences {
        let mut prefs = Preferences::default();

        prefs.dictation.provider.api_key = Some(KEYS[0].to_string());
        prefs.ai.providers = vec![
            crate::ai::provider::Provider {
                id: "openai".into(),
                name: "OpenAI".into(),
                api_key: KEYS[1].into(),
                ..Default::default()
            },
            crate::ai::provider::Provider {
                id: "local".into(),
                name: "Local".into(),
                api_key: String::new(),
                ..Default::default()
            },
        ];
        prefs.tts.provider.api_key = Some(KEYS[2].to_string());
        prefs.store.github_token = Some(KEYS[3].to_string());

        prefs
    }

    /// Preferences differing from the defaults in **every** top-level section.
    ///
    /// The guard test below is what makes this worth writing by hand: a reset
    /// test built on a fixture that happens to leave one section at its
    /// default cannot tell a reset that touches that section from one that
    /// does not, and would pass either way.
    fn nothing_default() -> Preferences {
        let mut prefs = with_every_key();

        prefs.general.open_at_login = true;
        prefs.appearance.visible_rows = 12;
        prefs.snippets.expand_keywords = !prefs.snippets.expand_keywords;
        prefs.clipboard.retain_days = 99;
        prefs.emoji.primary = crate::emoji::Primary::Copy;
        prefs.hotkey.summon = "Ctrl+Alt+K".into();
        prefs.bindings = vec![crate::bindings::Binding {
            accelerator: "Ctrl+Alt+B".into(),
            action: "uppercase".into(),
            source: crate::bindings::Source::Selection,
            replace: true,
            argument: None,
        }];
        prefs.aliases = vec![crate::registry::Alias {
            alias: "e".into(),
            command: "explorer".into(),
        }];
        prefs.taps.window_ms = 700;
        prefs.hyper.key = Some(0x14);
        prefs
            .action_keys
            .overrides
            .insert("copy".into(), "Ctrl+Shift+C".into());
        prefs.navigation.numeric = !prefs.navigation.numeric;
        prefs.screenshot.weight = 9;
        prefs.sources.path_executables = true;
        prefs.browsers.max_results = 7;
        prefs.web_search.engine = "ddg".into();
        prefs.files.max_results = 42;
        prefs.store.windows_only = !prefs.store.windows_only;
        prefs.scripts.timeout_seconds = 77;
        prefs.ai.provider = "openai".into();
        prefs.dictation.provider.enabled = true;
        prefs.tts.voice = "echo".into();
        prefs.widgets.fahrenheit = !prefs.widgets.fahrenheit;
        prefs.privacy.paused = !prefs.privacy.paused;
        prefs.mcp.servers = vec![crate::preferences::McpServer {
            name: "notes".into(),
            command: "node".into(),
            args: vec!["server.js".into()],
            actions: Vec::new(),
        }];
        prefs.layouts = vec![crate::layouts::Layout {
            id: "reading".into(),
            name: "Reading".into(),
            x: 0.25,
            y: 0.1,
            width: 0.5,
            height: 0.8,
        }];

        prefs
    }

    /// The fixture the reset tests rest on really does differ everywhere.
    #[test]
    fn the_fixture_differs_from_the_defaults_in_every_section() {
        let fresh = Preferences::default();
        let changed = nothing_default();

        for name in sections_of(&fresh) {
            assert_ne!(
                section(&fresh, &name),
                section(&changed, &name),
                "{name} is at its default in the fixture, so a reset that wrongly \
                 cleared it would look identical to one that left it alone"
            );
        }
    }

    /// Every section of the document belongs to exactly one panel.
    ///
    /// Both directions. A section nobody assigned is one no reset can reach,
    /// and a section two panels claim is one each of them quietly resets.
    #[test]
    fn every_section_belongs_to_one_panel() {
        let sections = sections_of(&Preferences::default());

        for name in &sections {
            let owners: Vec<&str> = PANELS
                .iter()
                .filter(|panel| panel.sections.contains(&name.as_str()))
                .map(|panel| panel.id)
                .collect();

            assert_eq!(
                owners.len(),
                1,
                "{name} is owned by {owners:?}; every section belongs to exactly one \
                 panel so that a reset reaches it and reaches it once"
            );
        }

        for panel in PANELS {
            for named in panel.sections {
                assert!(
                    sections.contains(&(*named).to_string()),
                    "the {} panel claims a {named} section the preferences do not have",
                    panel.id
                );
            }

            assert!(
                crate::settings_index::PANELS.contains(&panel.id),
                "{} is not a settings panel, so nothing can ask to reset it",
                panel.id
            );
        }
    }

    /// No credential reaches an export, in either form it can take.
    ///
    /// The plain form is what Sill holds in memory, and the sealed form is
    /// what its file holds. Both are checked, because the two ways this can go
    /// wrong are writing what is in memory and copying what is on disk.
    #[test]
    fn no_credential_reaches_an_export() {
        let text = to_json(&with_every_key()).expect("an export is written");

        for key in KEYS {
            assert!(
                !text.contains(key),
                "the export carries {key}, which is a credential leaving the machine"
            );
        }

        assert!(
            !text.contains("dpapi:v1:"),
            "the export carries a sealed value, which is a credential leaving the \
             machine and one that could not be opened when it arrived"
        );

        let document: Value = serde_json::from_str(&text).expect("the export is JSON");
        let withheld = document["withheld"]
            .as_array()
            .expect("the export says what it withheld");

        assert_eq!(
            withheld.len(),
            crate::preferences::SEALED.len(),
            "the export withheld {withheld:?}, and every credential in this fixture \
             was set, so every sealed path should be named"
        );
    }

    /// A sealed value in the settings does not reach an export either.
    ///
    /// Guards the other way in: preferences read from a file another Windows
    /// account sealed hold a blob rather than a key, and writing the blob out
    /// is the same leak wearing a different shape.
    #[test]
    fn a_sealed_value_does_not_reach_an_export() {
        let mut prefs = Preferences::default();
        prefs.store.github_token = Some("dpapi:v1:AQAAANCMnd8BFdERjHoAwE".to_string());

        let text = to_json(&prefs).expect("an export is written");

        assert!(
            !text.contains("dpapi:v1:"),
            "a sealed blob reached the export"
        );
        assert!(!text.contains("AQAAANCMnd8BFdERjHoAwE"));
    }

    /// An export reads back as everything but the credentials.
    ///
    /// Onto settings that already hold the same credentials, because that is
    /// the only way an export can come back whole: it carries no key by
    /// design, so restoring one onto a machine with no keys restores no keys,
    /// and asserting otherwise would be asserting the leak.
    #[test]
    fn an_export_reads_back() {
        let prefs = nothing_default();
        let text = to_json(&prefs).expect("an export is written");

        let patch = from_json(&text).expect("the export reads");
        let (back, summary) = apply(&with_every_key(), &patch).expect("it applies");

        assert_eq!(
            serde_json::to_value(&back).expect("serialise"),
            serde_json::to_value(&prefs).expect("serialise"),
            "an export restored over the same credentials should be the settings it \
             was taken from"
        );

        assert!(summary.sections.contains(&"appearance".to_string()));
    }

    /// An export restored onto a machine with no keys restores no keys.
    ///
    /// The other half of the sentence above, said out loud so nobody later
    /// reads the round trip as a promise that credentials travel.
    #[test]
    fn an_export_carries_no_key_to_a_machine_that_has_none() {
        let text = to_json(&with_every_key()).expect("an export is written");
        let patch = from_json(&text).expect("the export reads");

        let (back, summary) = apply(&Preferences::default(), &patch).expect("it applies");

        assert_eq!(back.dictation.provider.api_key.as_deref(), None);
        assert_eq!(back.tts.provider.api_key.as_deref(), None);
        assert_eq!(back.store.github_token.as_deref(), None);
        assert!(back.ai.providers.iter().all(|one| one.api_key.is_empty()));
        assert_eq!(summary.kept_keys, 0);
    }

    /// The keys already here survive importing an export, which carries none.
    #[test]
    fn an_import_keeps_the_credentials_it_was_not_given() {
        let mine = with_every_key();
        let text = to_json(&nothing_default()).expect("an export is written");

        let patch = from_json(&text).expect("the export reads");
        let (back, summary) = apply(&mine, &patch).expect("it applies");

        assert_eq!(back.dictation.provider.api_key.as_deref(), Some(KEYS[0]));
        assert_eq!(back.tts.provider.api_key.as_deref(), Some(KEYS[2]));
        assert_eq!(back.store.github_token.as_deref(), Some(KEYS[3]));
        assert_eq!(
            back.ai
                .providers
                .iter()
                .find(|one| one.id == "openai")
                .map(|one| one.api_key.as_str()),
            Some(KEYS[1])
        );

        assert_eq!(summary.kept_keys, 4);
    }

    /// A key the file does carry wins over the one already here.
    #[test]
    fn a_file_that_carries_a_key_replaces_the_one_here() {
        let mine = with_every_key();

        let patch = serde_json::json!({
            "store": { "githubToken": "ghp_the_new_one" },
        });

        let (back, summary) = apply(&mine, &patch).expect("it applies");

        assert_eq!(back.store.github_token.as_deref(), Some("ghp_the_new_one"));
        // The other three were not mentioned, so they were kept.
        assert_eq!(summary.kept_keys, 3);
    }

    /// A provider's key follows its id rather than its place in the list.
    #[test]
    fn provider_keys_are_matched_by_id() {
        let mine = with_every_key();

        // The same two providers, in the other order and with no keys, which
        // is exactly what this machine's own export looks like.
        let patch = serde_json::json!({
            "ai": { "providers": [
                { "id": "local", "name": "Local" },
                { "id": "openai", "name": "OpenAI" },
            ]},
        });

        let (back, _) = apply(&mine, &patch).expect("it applies");

        assert_eq!(back.ai.providers[0].id, "local");
        assert_eq!(back.ai.providers[0].api_key, "");
        assert_eq!(back.ai.providers[1].id, "openai");
        assert_eq!(
            back.ai.providers[1].api_key, KEYS[1],
            "the key followed the provider it belongs to"
        );
    }

    /// A section the file says nothing about keeps what it had.
    #[test]
    fn a_section_the_file_is_silent_about_is_left_alone() {
        let mine = nothing_default();

        let patch = serde_json::json!({ "appearance": { "visibleRows": 6 } });
        let (back, summary) = apply(&mine, &patch).expect("it applies");

        assert_eq!(back.appearance.visible_rows, 6);
        assert_eq!(back.hotkey.summon, mine.hotkey.summon);
        assert_eq!(back.files.max_results, mine.files.max_results);
        assert_eq!(back.scripts.timeout_seconds, mine.scripts.timeout_seconds);
        assert_eq!(summary.sections, vec!["appearance".to_string()]);
    }

    /// Half a field is still the rest of the section.
    #[test]
    fn a_named_field_does_not_take_the_rest_of_its_section_with_it() {
        let mine = nothing_default();

        let patch = serde_json::json!({ "appearance": { "visibleRows": 6 } });
        let (back, _) = apply(&mine, &patch).expect("it applies");

        assert_eq!(back.appearance.window_width, mine.appearance.window_width);
        assert_eq!(back.appearance.theme, mine.appearance.theme);
    }

    /// Everything a bad file can be, and none of them produce settings.
    #[test]
    fn a_file_that_cannot_be_read_produces_nothing_to_save() {
        let good = to_json(&nothing_default()).expect("an export is written");

        let bad = [
            // Truncated half way, which is what a copy interrupted looks like.
            &good[..good.len() / 2],
            // Empty.
            "",
            // JSON, and not settings.
            "{\"hello\":\"world\"}",
            // A list rather than a document.
            "[1, 2, 3]",
            // Not JSON at all.
            "PK\u{3}\u{4} not really",
        ];

        for text in bad {
            assert!(
                from_json(text).is_err(),
                "{text:?} was read as settings when there are none in it"
            );
        }
    }

    /// A file from a later Sill is refused rather than read hopefully.
    #[test]
    fn a_newer_file_is_refused() {
        let later = serde_json::json!({
            "sill": KIND,
            "format": FORMAT + 1,
            "preferences": { "appearance": { "visibleRows": 6 } },
        });

        let why = from_json(&later.to_string()).expect_err("a newer export is refused");
        assert!(why.contains("newer Sill"), "{why}");

        let store = serde_json::json!({
            "version": crate::preferences::SCHEMA.version + 1,
            "appearance": { "visibleRows": 6 },
        });

        let why = from_json(&store.to_string()).expect_err("a newer store file is refused");
        assert!(why.contains("newer Sill"), "{why}");
    }

    /// An export written to a file imports back from that file.
    ///
    /// Through disk with nothing held in memory in between, because that is
    /// the trip the file actually makes: written on one machine, opened on
    /// another, and read by a process that never saw the settings it came
    /// from.
    #[test]
    fn an_export_survives_being_a_file() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let away = folder.path().join("sill-settings.json");
        let store = crate::preferences::path(folder.path());

        {
            let mine = nothing_default();
            std::fs::write(&away, to_json(&mine).expect("an export is written"))
                .expect("the export reaches disk");
            mine.save(&store).expect("settings are written");
        }

        // A different machine: the settings file is gone, and only the export
        // is left.
        std::fs::remove_file(&store).expect("the settings file goes");

        let text = std::fs::read_to_string(&away).expect("the export is read back");
        let patch = from_json(&text).expect("the export reads");
        let (next, _) = apply(&Preferences::load(&store), &patch).expect("it applies");
        next.save(&store).expect("settings are written");

        let landed = Preferences::load(&store);

        assert_eq!(landed.appearance.visible_rows, 12);
        assert_eq!(landed.hotkey.summon, "Ctrl+Alt+K");
        assert_eq!(landed.files.max_results, 42);
        assert_eq!(landed.aliases.len(), 1);
        assert_eq!(landed.bindings.len(), 1);
        assert_eq!(landed.scripts.timeout_seconds, 77);

        // And still no credentials, because the file never had any.
        assert_eq!(landed.store.github_token.as_deref(), None);
        assert!(landed.ai.providers.iter().all(|one| one.api_key.is_empty()));
    }

    /// A bad import leaves the settings on disk readable and complete.
    ///
    /// Through a real file, with the object that wrote it dropped in between,
    /// because the property is about what survives on disk rather than about
    /// what a variable still holds.
    #[test]
    fn a_bad_import_leaves_the_previous_settings_whole() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let file = crate::preferences::path(folder.path());

        let before = {
            let mine = nothing_default();
            mine.save(&file).expect("settings are written");
            serde_json::to_value(&mine).expect("serialise")
        };

        // Everything in memory is gone; only the file is left.
        let held = Preferences::load(&file);

        for text in ["", "{\"hello\":\"world\"}", "{\"appearance\": 3}"] {
            let outcome = from_json(text).and_then(|patch| apply(&held, &patch));
            assert!(outcome.is_err(), "{text:?} was applied");
        }

        drop(held);

        let after = serde_json::to_value(Preferences::load(&file)).expect("serialise");

        assert_eq!(
            after, before,
            "the settings on disk changed while every import was failing"
        );
    }

    /// Resetting one panel changes that panel and nothing else.
    ///
    /// Every panel, against a fixture where every section differs from its
    /// default, so a reset reaching a neighbour has nowhere to hide.
    #[test]
    fn a_reset_changes_one_panel_and_nothing_else() {
        let mine = nothing_default();
        let fresh = Preferences::default();

        for panel in PANELS {
            let after = reset(&mine, panel.id).expect("a panel resets");

            for name in sections_of(&mine) {
                if panel.sections.contains(&name.as_str()) {
                    assert_eq!(
                        section(&after, &name),
                        section(&fresh, &name),
                        "resetting {} left {name} at something other than its default",
                        panel.id
                    );
                } else {
                    assert_eq!(
                        section(&after, &name),
                        section(&mine, &name),
                        "resetting {} also reset {name}, which belongs to another panel",
                        panel.id
                    );
                }
            }
        }
    }

    /// A reset survives the trip through disk it will really make.
    #[test]
    fn a_reset_is_still_one_panel_after_a_save_and_a_load() {
        let folder = tempfile::tempdir().expect("a temporary folder");
        let file = crate::preferences::path(folder.path());

        let mine = nothing_default();
        let before = serde_json::to_value(&mine).expect("serialise");

        reset(&mine, "appearance")
            .expect("a panel resets")
            .save(&file)
            .expect("settings are written");

        drop(mine);

        let back = serde_json::to_value(Preferences::load(&file)).expect("serialise");
        let fresh = serde_json::to_value(Preferences::default()).expect("serialise");

        assert_eq!(back["appearance"], fresh["appearance"]);
        assert_eq!(back["hotkey"], before["hotkey"]);
        assert_eq!(back["files"], before["files"]);
        assert_eq!(back["widgets"], before["widgets"]);
    }

    /// Resetting a panel with nothing of its own is refused, not silently done.
    #[test]
    fn a_panel_with_nothing_to_reset_says_so() {
        assert!(reset(&Preferences::default(), "about").is_err());
        assert!(reset(&Preferences::default(), "advanced").is_err());
        assert!(reset(&Preferences::default(), "quicklinks").is_err());
        assert!(reset(&Preferences::default(), "nonsense").is_err());
    }

    /// PowerToys Run's own files, copied from an installed PowerToys 0.90.1.
    ///
    /// Verbatim rather than minimal, because the point of this fixture is that
    /// the reader meets the keys a real install actually writes, in the order
    /// and the spelling it writes them.
    const PT_SETTINGS: &str = r#"{
      "PreviousHotkey": "",
      "Hotkey": "Alt \u002B Space",
      "UseCentralizedKeyboardHook": false,
      "SearchQueryResultsWithDelay": true,
      "Theme": 0,
      "StartupPosition": 0,
      "ShouldUsePinyin": false,
      "WindowLeft": 960,
      "WindowTop": 334,
      "MaxResultsToShow": 4,
      "ActivateTimes": 2,
      "HideWhenDeactivated": true,
      "ClearInputOnLaunch": false,
      "TabSelectsContextButtons": true,
      "RememberLastLaunchLocation": false,
      "ShowPluginsOverview": 0,
      "TitleFontSize": 16,
      "IgnoreHotkeysOnFullscreen": false,
      "StartedFromPowerToysRunner": true,
      "GenerateThumbnailsFromFiles": true,
      "LastQueryMode": "Selected"
    }"#;

    const PT_PROGRAMS: &str = r#"{
      "LastIndexTime": "2025-04-23T00:00:00-07:00",
      "ProgramSources": [],
      "DisabledProgramSources": [],
      "ProgramSuffixes": ["bat", "appref-ms", "exe", "lnk", "url"],
      "EnableStartMenuSource": true,
      "EnableDesktopSource": true,
      "EnableRegistrySource": true,
      "EnablePathEnvironmentVariableSource": false,
      "MinScoreThreshold": 0.75
    }"#;

    const PT_FOLDERS: &str = r#"{
      "FolderLinks": [
        { "Path": "C:\\Projects", "Nickname": "projects" },
        { "Path": "  ", "Nickname": "blank" }
      ],
      "MaxFolderResults": 50,
      "MaxFileResults": 50
    }"#;

    #[test]
    fn a_real_power_toys_run_install_reads() {
        let patch = from_power_toys(&PowerToys {
            settings: PT_SETTINGS,
            programs: Some(PT_PROGRAMS),
            folders: Some(PT_FOLDERS),
        })
        .expect("PowerToys Run reads");

        let (back, summary) = apply(&Preferences::default(), &patch).expect("it applies");

        assert_eq!(back.hotkey.summon, "Alt+Space");
        assert!(back.hotkey.dismiss_on_blur);
        assert!(!back.hotkey.reset_on_summon);
        // Four is below the smallest window Sill draws, so it arrives clamped
        // rather than as a number the panel would silently disagree with.
        assert_eq!(back.appearance.visible_rows, 4);
        assert!(back.sources.shortcuts);
        assert!(back.sources.app_paths);
        assert!(!back.sources.path_executables);
        assert_eq!(back.files.roots, vec!["C:\\Projects".to_string()]);

        assert!(summary.sections.contains(&"sources".to_string()));
    }

    /// Nothing PowerToys does not say is invented.
    #[test]
    fn power_toys_leaves_alone_what_it_says_nothing_about() {
        let mine = nothing_default();

        let patch = from_power_toys(&PowerToys {
            settings: PT_SETTINGS,
            programs: None,
            folders: None,
        })
        .expect("PowerToys Run reads");

        let (back, _) = apply(&mine, &patch).expect("it applies");

        assert_eq!(back.appearance.theme, mine.appearance.theme);
        assert_eq!(back.files.max_results, mine.files.max_results);
        assert_eq!(back.sources.path_executables, mine.sources.path_executables);
        assert_eq!(back.scripts.timeout_seconds, mine.scripts.timeout_seconds);
    }

    #[test]
    fn a_power_toys_chord_becomes_an_accelerator() {
        assert_eq!(accelerator("Alt + Space").as_deref(), Some("Alt+Space"));
        assert_eq!(
            accelerator("Ctrl + Shift + Space").as_deref(),
            Some("Ctrl+Shift+Space")
        );
        assert_eq!(accelerator("Win + R").as_deref(), Some("Super+R"));
        // A modifier on its own registers nothing, and neither does an empty
        // string, so neither is allowed near the one key that opens Sill.
        assert_eq!(accelerator("Alt"), None);
        assert_eq!(accelerator(""), None);
        assert_eq!(accelerator("Alt + "), None);
    }

    /// A settings file that is PowerToys' is named as such, not read as ours.
    #[test]
    fn power_toys_is_not_mistaken_for_a_sill_file() {
        let why = from_json(PT_SETTINGS).expect_err("PowerToys is not a Sill settings file");
        assert!(why.contains("PowerToys"), "{why}");
    }

    /// A chosen file goes to the reader its own bytes call for.
    #[test]
    fn a_file_is_sorted_by_what_is_in_it() {
        let export = to_json(&Preferences::default()).expect("an export is written");

        assert_eq!(kind(export.as_bytes()), Kind::Json);
        assert_eq!(kind(PT_SETTINGS.as_bytes()), Kind::PowerToysRun);
        assert_eq!(
            kind(&rayconfig(&[("snippets.json", "[]")])),
            Kind::Archive,
            "a zip is an archive whatever it is called"
        );

        // A byte order mark on the front of a hand-edited file must not stop
        // it being recognised, for the reason `json_store` skips one.
        let marked = format!("\u{feff}{PT_SETTINGS}");
        assert_eq!(kind(marked.as_bytes()), Kind::PowerToysRun);

        // Anything else goes to the JSON reader, which is the one that says
        // what is wrong with it.
        assert_eq!(kind(b"not anything at all"), Kind::Json);
    }

    /// A `.rayconfig` is a zip, and what Sill takes from it are its snippets
    /// and quicklinks.
    fn rayconfig(members: &[(&str, &str)]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            for (name, text) in members {
                zip.start_file(*name, options).expect("a member");
                std::io::Write::write_all(&mut zip, text.as_bytes()).expect("written");
            }

            zip.finish().expect("the archive closes");
        }
        out.into_inner()
    }

    #[test]
    fn a_rayconfig_gives_up_its_snippets_and_quicklinks() {
        let archive = rayconfig(&[
            (
                "snippets.json",
                r#"[{"name":"Address","keyword":";addr","text":"1 High Street"}]"#,
            ),
            (
                "quicklinks.json",
                r#"[{"name":"Issues","link":"https://example.test/{query}"}]"#,
            ),
            ("meta.json", r#"{"exportedBy":"raycast","version":3}"#),
        ]);

        let found = from_rayconfig(&archive).expect("a rayconfig reads");

        assert_eq!(found.snippets.len(), 1);
        assert_eq!(found.snippets[0].name, "Address");
        assert_eq!(found.snippets[0].content, "1 High Street");
        assert_eq!(found.quicklinks.len(), 1);
        assert_eq!(found.quicklinks[0].link, "https://example.test/{query}");
    }

    /// An encrypted export is not a zip, and says so rather than doing nothing.
    #[test]
    fn a_rayconfig_that_is_not_an_archive_says_so() {
        let why = from_rayconfig(b"not a zip at all").expect_err("refused");
        assert!(why.contains("encrypted"), "{why}");

        let empty = rayconfig(&[("readme.txt", "nothing useful in here")]);
        assert!(from_rayconfig(&empty).is_err());
    }
}
