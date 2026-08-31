//! What the model can look at.
//!
//! Every tool here reads. Nothing in this file changes a file, launches a
//! program, types into a window or touches the machine, and that is a property
//! of the set rather than of each one: a read that goes wrong wastes a turn,
//! and a write that goes wrong is somebody's afternoon. Doing things comes
//! through the action registry, which already declares a capability per action
//! and already knows how to undo one, and it arrives with the card that asks
//! first.
//!
//! ## Why these and not thirty
//!
//! A tool list is sent on every request and read by the model before every
//! answer, so it is both a cost and a distraction. These are the questions a
//! launcher is uniquely able to answer about the machine it is running on:
//! what is installed, what is open, what was copied, what is on screen, what
//! is in a folder. Anything a model can work out for itself is not here.

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};

/// One tool, as the model is told about it.
pub struct Tool {
    pub name: &'static str,
    /// Written for the model, not for a person.
    ///
    /// It is the only thing deciding whether a tool is reached for at the
    /// right moment, so it says when to use it rather than what it does.
    pub description: &'static str,
    /// JSON Schema for the arguments.
    pub schema: fn() -> Value,
}

/// How much of anything one call may answer with.
///
/// The answer is pasted into the conversation and paid for on every request
/// after it, so an unbounded read is a bill as well as a context. Generous
/// enough that a real question is answered in one call.
const MOST_ROWS: usize = 30;
const MOST_BYTES: usize = 20_000;

pub const CATALOGUE: &[Tool] = &[
    Tool {
        name: "search_sill",
        description: "Search everything Sill has indexed on this machine: installed \
                      applications, Windows settings pages, Sill's own commands, saved \
                      snippets and quicklinks. Use this to find out whether something \
                      is installed or where a setting lives.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to look for, as somebody would type it."
                    }
                },
                "required": ["query"]
            })
        },
    },
    Tool {
        name: "find_files",
        description: "Find files and folders anywhere on this machine by name. Instant, \
                      because it reads an existing index rather than walking the disk. \
                      Use this before asking where something is.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Part of a name. Supports wildcards."
                    }
                },
                "required": ["query"]
            })
        },
    },
    Tool {
        name: "read_file",
        description: "Read the text of one file. Use it after find_files has given you a \
                      path. Refuses anything that is not text.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The full path." }
                },
                "required": ["path"]
            })
        },
    },
    Tool {
        name: "list_directory",
        description: "What is in a folder: names, whether each is a folder, and how big. \
                      Use this to look around rather than to search.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "The full path of the folder." }
                },
                "required": ["path"]
            })
        },
    },
    Tool {
        name: "read_clipboard",
        description: "The most recent things copied on this machine, newest first, each \
                      with the application it came from. Use this when asked about \
                      something that was copied, or to work with what somebody just \
                      copied without making them paste it.",
        schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional words to filter by. Omit for the most recent."
                    }
                }
            })
        },
    },
    Tool {
        name: "list_windows",
        description: "Every window open right now, with its application, whether it is \
                      minimised and which display it is on. Use this to answer what is \
                      open or what somebody is working on.",
        schema: || json!({ "type": "object", "properties": {} }),
    },
    Tool {
        name: "system_state",
        description: "How this machine is set right now: volume, whether it is muted, \
                      dark mode, wifi and bluetooth. Use this before answering anything \
                      about the state of the machine.",
        schema: || json!({ "type": "object", "properties": {} }),
    },
    Tool {
        name: "read_selection",
        description: "The text selected in whatever application was in front before Sill \
                      opened. Empty when nothing was selected. Use this when somebody \
                      says 'this' or 'the selected text'.",
        schema: || json!({ "type": "object", "properties": {} }),
    },
    Tool {
        name: "read_screen",
        description: "The words currently on screen, read by looking at it. Slower than \
                      the others and worth it only when the answer is somewhere no other \
                      tool can reach, such as inside an image or an application that \
                      keeps its text to itself.",
        schema: || json!({ "type": "object", "properties": {} }),
    },
];

/// The tool list, in the shape a request carries it.
pub fn as_request() -> Value {
    Value::Array(
        CATALOGUE
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": (tool.schema)(),
                    }
                })
            })
            .collect(),
    )
}

/// Whether Sill knows this tool at all.
///
/// A model will occasionally call something that is not here, usually a name
/// close to one that is. Saying so plainly gets a corrected call on the next
/// step; a silent empty answer gets the same wrong call again.
pub fn known(name: &str) -> bool {
    CATALOGUE.iter().any(|tool| tool.name == name)
}

/// Runs one, and answers with what it found.
///
/// Every failure is an answer rather than an error. A tool that cannot read
/// something has told the model a fact about the machine, and the turn should
/// continue with that fact in it: a folder that is not there, a file that is
/// not text, a clipboard with nothing in it. Only a name Sill does not know is
/// worth stopping for, and even that comes back as a message the model can act
/// on.
pub async fn run(app: &AppHandle, name: &str, args: &Value) -> Value {
    let text = |key: &str| -> String {
        args.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string()
    };

    match name {
        "search_sill" => search_sill(app, &text("query")).await,
        "find_files" => find_files(&text("query")),
        "read_file" => read_file(&text("path")),
        "list_directory" => list_directory(&text("path")),
        "read_clipboard" => read_clipboard(app, &text("query")),
        "list_windows" => list_windows(),
        "system_state" => system_state(),
        "read_selection" => read_selection(app),
        "read_screen" => read_screen(),
        other => json!({ "error": format!("Sill has no tool called {other}.") }),
    }
}

async fn search_sill(app: &AppHandle, query: &str) -> Value {
    if query.is_empty() {
        return json!({ "error": "Give it something to search for." });
    }

    let state = app.state::<crate::state::RegistryState>();
    let registry = state.inner.lock().await;

    let found: Vec<Value> = crate::registry::search(
        &registry.everything().cloned().collect::<Vec<_>>(),
        query,
        &registry.frecency,
        crate::state::now_seconds(),
        MOST_ROWS,
    )
    .into_iter()
    .take(MOST_ROWS)
    .map(|hit| {
        json!({
            "name": hit.command.title,
            "what": kind_of(&hit.command.mode),
            "detail": hit.command.subtitle,
        })
    })
    .collect();

    json!({ "found": found.len(), "results": found })
}

/// What a row is, said the way somebody would say it.
fn kind_of(mode: &str) -> &'static str {
    match mode {
        "app" => "application",
        "exe" => "command line program",
        "setting" => "Windows setting",
        "sill-setting" => "Sill setting",
        "builtin" => "Sill command",
        "snippet" => "saved snippet",
        "quicklink" | "quicklink-arg" => "saved link",
        "system" => "system switch",
        "file" => "file",
        "window" => "open window",
        "view" | "no-view" => "extension command",
        _ => "result",
    }
}

fn find_files(query: &str) -> Value {
    if query.is_empty() {
        return json!({ "error": "Give it something to search for." });
    }

    if !crate::files::available() {
        return json!({
            "error": "File search is not available: Everything is not running on this machine."
        });
    }

    let found: Vec<Value> = crate::files::search(query, MOST_ROWS)
        .into_iter()
        .map(|hit| json!({ "name": hit.name, "path": hit.path, "folder": hit.is_dir }))
        .collect();

    json!({ "found": found.len(), "results": found })
}

fn read_file(path: &str) -> Value {
    if path.is_empty() {
        return json!({ "error": "Name a file." });
    }

    let path = std::path::Path::new(path);

    let Ok(data) = std::fs::read(path) else {
        return json!({ "error": format!("Could not read {}.", path.display()) });
    };

    let whole = data.len();
    let cut = data.len() > MOST_BYTES;

    // Lossy on purpose. A file that is nearly text, or one cut mid-character
    // by the limit below, is still worth reading; refusing it because of one
    // byte is not.
    let text = String::from_utf8_lossy(&data[..whole.min(MOST_BYTES)]).to_string();

    // Enough replacement characters means it was never text. A model handed
    // the bytes of a PNG as a string will try to reason about them.
    let noise = text.chars().filter(|c| *c == char::REPLACEMENT_CHARACTER).count();
    if noise > text.chars().count() / 20 {
        return json!({
            "error": format!("{} is not a text file.", path.display()),
            "bytes": whole,
        });
    }

    json!({ "path": path.to_string_lossy(), "bytes": whole, "truncated": cut, "text": text })
}

fn list_directory(path: &str) -> Value {
    if path.is_empty() {
        return json!({ "error": "Name a folder." });
    }

    let Ok(reading) = std::fs::read_dir(path) else {
        return json!({ "error": format!("Could not open {path}.") });
    };

    let mut entries: Vec<Value> = Vec::new();

    for found in reading.flatten().take(MOST_ROWS * 4) {
        let meta = found.metadata().ok();
        entries.push(json!({
            "name": found.file_name().to_string_lossy(),
            "folder": meta.as_ref().is_some_and(|m| m.is_dir()),
            "bytes": meta.as_ref().map(|m| m.len()),
        }));

        if entries.len() >= MOST_ROWS {
            break;
        }
    }

    json!({ "path": path, "found": entries.len(), "entries": entries })
}

fn read_clipboard(app: &AppHandle, query: &str) -> Value {
    // Not running is an ordinary state rather than a fault: somebody can turn
    // clipboard history off, and the answer should say so instead of failing.
    let Some(clipboard) = app.try_state::<crate::clipboard::monitor::Clipboard>() else {
        return json!({ "error": "Clipboard history is not running on this machine." });
    };

    let Ok(entries) = clipboard.store().search(query, None, MOST_ROWS) else {
        return json!({ "error": "Could not read the clipboard history." });
    };

    let found: Vec<Value> = entries
        .iter()
        .map(|entry| {
            json!({
                "what": entry.kind.as_str(),
                "from": entry.app,
                "text": entry.text.chars().take(400).collect::<String>(),
            })
        })
        .collect();

    json!({ "found": found.len(), "entries": found })
}

fn list_windows() -> Value {
    let found: Vec<Value> = crate::windowing::records()
        .into_iter()
        .take(MOST_ROWS)
        .map(|row| json!({ "title": row.title, "application": row.extension_title }))
        .collect();

    json!({ "found": found.len(), "windows": found })
}

fn system_state() -> Value {
    let state = crate::system::state();
    let live = crate::system::live();

    json!({
        "volume_percent": state.volume,
        "muted": state.muted,
        "dark_mode": state.dark,
        "wifi_on": crate::system::toggle_state("system.radio:wifi", &live),
        "bluetooth_on": crate::system::toggle_state("system.radio:bluetooth", &live),
    })
}

fn read_selection(app: &AppHandle) -> Value {
    match crate::selection::capture(app) {
        Some(text) if !text.trim().is_empty() => {
            json!({ "text": text.chars().take(MOST_BYTES).collect::<String>() })
        }
        _ => json!({ "text": "", "note": "Nothing was selected." }),
    }
}

fn read_screen() -> Value {
    let (left, top, width, height) = crate::capture::virtual_screen();

    if width <= 0 || height <= 0 {
        return json!({ "error": "No screens were found." });
    }

    let shot = match crate::capture::region(left, top, width, height) {
        Ok(shot) => shot,
        Err(why) => return json!({ "error": format!("Could not look at the screen: {why}") }),
    };

    match crate::ocr::read_bgra(&shot.pixels, shot.width, shot.height) {
        Ok(text) if text.trim().is_empty() => {
            json!({ "text": "", "note": "Nothing readable was on screen." })
        }
        Ok(text) => json!({ "text": text.chars().take(MOST_BYTES).collect::<String>() }),
        Err(why) => json!({ "error": format!("Could not read the screen: {why}") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod what_is_offered {
        use super::*;

        #[test]
        fn every_tool_has_a_name_no_other_has() {
            let mut names: Vec<&str> = CATALOGUE.iter().map(|tool| tool.name).collect();
            let count = names.len();
            names.sort();
            names.dedup();
            assert_eq!(names.len(), count, "two tools share a name");
        }

        /// The description is the only thing deciding whether a tool is
        /// reached for at the right moment, so an empty or terse one is a tool
        /// that never gets used and is paid for on every request anyway.
        #[test]
        fn every_tool_explains_when_to_use_it() {
            for tool in CATALOGUE {
                assert!(
                    tool.description.len() > 60,
                    "{} says too little for a model to choose it",
                    tool.name,
                );
            }
        }

        /// Names go over the wire and into a match arm. A service will reject
        /// anything that is not this shape, and it is easy to write a space.
        #[test]
        fn every_name_is_plain() {
            for tool in CATALOGUE {
                assert!(
                    tool.name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{} is not a plain name",
                    tool.name,
                );
            }
        }

        #[test]
        fn every_tool_is_known_by_its_own_name() {
            for tool in CATALOGUE {
                assert!(known(tool.name), "{} is not findable", tool.name);
            }
            assert!(!known("something_else"));
        }

        /// Every schema is an object with properties, because that is what the
        /// services accept. One written as a bare type is rejected by the
        /// provider with a message about the request rather than about the
        /// tool, which is a long way from the mistake.
        #[test]
        fn every_schema_is_an_object() {
            for tool in CATALOGUE {
                let schema = (tool.schema)();
                assert_eq!(schema["type"], "object", "{} is not an object", tool.name);
                assert!(schema.get("properties").is_some(), "{} has no properties", tool.name);
            }
        }

        /// Whatever is required must exist, or the model is told to send a
        /// field the schema does not describe.
        #[test]
        fn everything_required_is_described() {
            for tool in CATALOGUE {
                let schema = (tool.schema)();
                let Some(required) = schema.get("required").and_then(Value::as_array) else {
                    continue;
                };

                for name in required {
                    let name = name.as_str().unwrap_or_default();
                    assert!(
                        schema["properties"].get(name).is_some(),
                        "{} requires {name}, which it never describes",
                        tool.name,
                    );
                }
            }
        }

        #[test]
        fn the_request_carries_every_tool_in_the_documented_shape() {
            let sent = as_request();
            let list = sent.as_array().expect("an array");
            assert_eq!(list.len(), CATALOGUE.len());

            for (at, tool) in CATALOGUE.iter().enumerate() {
                assert_eq!(list[at]["type"], "function");
                assert_eq!(list[at]["function"]["name"], tool.name);
                assert!(list[at]["function"]["parameters"].is_object());
            }
        }
    }

    mod reading_a_file {
        use super::*;

        fn a_file(name: &str, data: &[u8]) -> std::path::PathBuf {
            let path = std::env::temp_dir().join(format!("sill-tool-{name}"));
            std::fs::write(&path, data).expect("written");
            path
        }

        #[test]
        fn text_comes_back() {
            let path = a_file("plain.txt", b"hello there");
            let said = read_file(&path.to_string_lossy());
            assert_eq!(said["text"], "hello there");
            assert_eq!(said["truncated"], false);
        }

        /// A model handed the bytes of a PNG as a string will try to reason
        /// about them, and it has no way to tell that it should not.
        #[test]
        fn something_that_is_not_text_says_so() {
            let path = a_file("picture.png", &[0x89, 0x50, 0x4e, 0x47, 0x00, 0xff, 0xfe, 0xfd]);
            let said = read_file(&path.to_string_lossy());
            assert!(said["error"].as_str().unwrap_or_default().contains("not a text file"));
        }

        /// The answer is pasted into the conversation and paid for on every
        /// request after it, so it has a ceiling and says when it hit one.
        #[test]
        fn a_long_file_is_cut_and_says_so() {
            let path = a_file("long.txt", "a".repeat(MOST_BYTES * 2).as_bytes());
            let said = read_file(&path.to_string_lossy());
            assert_eq!(said["truncated"], true);
            assert_eq!(said["bytes"], MOST_BYTES * 2);
            assert_eq!(said["text"].as_str().unwrap_or_default().len(), MOST_BYTES);
        }

        /// Not an error the turn stops on. A file that is not there is a fact
        /// about the machine, and the answer is better for having it.
        #[test]
        fn a_file_that_is_not_there_is_an_answer() {
            let said = read_file("C:/nothing/here/at/all.txt");
            assert!(said["error"].is_string());
        }

        #[test]
        fn naming_nothing_says_so() {
            assert!(read_file("")["error"].is_string());
        }
    }

    mod listing_a_folder {
        use super::*;

        #[test]
        fn a_folder_that_is_not_there_is_an_answer() {
            assert!(list_directory("C:/nothing/here")["error"].is_string());
        }

        #[test]
        fn what_is_in_one_comes_back() {
            let dir = std::env::temp_dir().join("sill-tool-listing");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("inner")).expect("a directory");
            std::fs::write(dir.join("a.txt"), b"x").expect("written");

            let said = list_directory(&dir.to_string_lossy());
            let entries = said["entries"].as_array().expect("entries");
            assert_eq!(entries.len(), 2);

            let folders = entries.iter().filter(|e| e["folder"] == true).count();
            assert_eq!(folders, 1, "the folder was not marked as one");
        }

        /// An answer that is a whole directory of a thousand files is a bill
        /// on every request after it.
        #[test]
        fn a_huge_folder_is_capped() {
            let dir = std::env::temp_dir().join("sill-tool-huge");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("a directory");
            for n in 0..(MOST_ROWS + 20) {
                std::fs::write(dir.join(format!("{n}.txt")), b"x").expect("written");
            }

            let said = list_directory(&dir.to_string_lossy());
            assert_eq!(said["entries"].as_array().expect("entries").len(), MOST_ROWS);
        }
    }
}
