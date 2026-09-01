//! What an extension will be able to do, said before it is installed.
//!
//! ## Why this exists at all
//!
//! An extension is a Node program that Sill starts. It runs as the person
//! using it, with everything that person can reach. That is true of Raycast,
//! true of Vicinae, and true here; what is different is that nobody says so.
//! Installing arbitrary code from a store without naming what it can touch is
//! the part that would be indefensible, so the naming comes first and the
//! store comes after it.
//!
//! ## What is enforced, and what is not
//!
//! **Enforced: nothing.** There is no sandbox. This does not stop an extension
//! doing anything; it reads the code and reports what it found. Say that
//! plainly rather than letting a capability list imply a permission system,
//! which is the failure mode of every "permissions" screen that grants
//! everything anyway. [`NOT_ENFORCED`] is the sentence the window shows.
//!
//! What is real:
//!
//! - The source shown is the source installed, at a commit that is recorded.
//! - `npm` runs with `--ignore-scripts`, so no package's install hook executes.
//! - Everything an extension asks **of Sill** goes through
//!   [`crate::host_bridge`], which is one file, and every method on it is
//!   named below. A test refuses to compile a bridge method nobody named.
//!
//! ## How it reads the code
//!
//! Substring search over the extension's own source, and that is a deliberate
//! floor rather than an aspiration. A parser would answer the same question
//! more precisely and would still be wrong the moment an extension builds a
//! module name at runtime or a dependency does the work for it. So this
//! **over-reports rather than under-reports**: a token in a comment counts, and
//! the honest statement about depth is [`NOT_ENFORCED`] rather than a claim
//! that the analysis is complete.

/// One thing an extension can reach, and what gives it away.
#[derive(Debug, Clone, Copy)]
pub struct Capability {
    pub id: &'static str,
    /// What it lets the extension do, in the words somebody deciding wants.
    pub title: &'static str,
    /// The part that is not obvious from the title.
    pub detail: &'static str,
    /// The identifiers in an extension's source that reveal it.
    tokens: &'static [&'static str],
    /// The [`crate::exthost::Bridge`] methods this covers.
    ///
    /// Empty for the capabilities that do not go through Sill at all, which is
    /// most of the dangerous ones: an extension opening a file does it in Node
    /// and never asks.
    bridge: &'static [&'static str],
}

/// Everything an extension can reach, worst first.
///
/// Ordered by what somebody deciding would want to see at the top rather than
/// alphabetically. Running another program is the first line of the list
/// because it is the one that ends the conversation.
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        id: "processes",
        title: "Run other programs",
        detail: "Starts anything on this machine, with your account's access.",
        tokens: &[
            "child_process",
            "execSync",
            "execFileSync",
            "spawnSync",
            "execFile",
            "spawn(",
            "exec(",
        ],
        bridge: &[],
    },
    Capability {
        id: "filesystem",
        title: "Read and write files",
        detail: "Any file you can open, not only its own.",
        tokens: &[
            "node:fs",
            "\"fs\"",
            "'fs'",
            "readFileSync",
            "writeFileSync",
            "existsSync",
            "readdirSync",
            "homedir(",
        ],
        bridge: &[],
    },
    Capability {
        id: "network",
        title: "Reach the internet",
        detail: "Sends and receives whatever it likes, to wherever it likes.",
        tokens: &[
            "fetch(",
            "axios",
            "node-fetch",
            "undici",
            "node:https",
            "node:http",
            "XMLHttpRequest",
        ],
        bridge: &[],
    },
    Capability {
        id: "clipboard",
        title: "Read and change the clipboard",
        detail: "Including pasting into whatever window you were in.",
        tokens: &["Clipboard"],
        bridge: &[
            "clipboard_write",
            "clipboard_read",
            "clipboard_clear",
            "clipboard_paste",
        ],
    },
    Capability {
        id: "selection",
        title: "Read the text you have selected",
        detail: "Whatever is highlighted in the window Sill came up over.",
        tokens: &["getSelectedText"],
        bridge: &["selected_text"],
    },
    Capability {
        id: "secrets",
        title: "Read its own settings",
        detail: "Anything you type into it, including passwords and API keys.",
        tokens: &["getPreferenceValues"],
        bridge: &[],
    },
    Capability {
        id: "oauth",
        title: "Sign you in to another service",
        detail: "Opens a browser to authorise it, and keeps the token.",
        tokens: &["OAuth"],
        bridge: &[],
    },
    Capability {
        id: "open",
        title: "Open files and links",
        detail: "Hands a path or an address to whatever program handles it.",
        tokens: &["open(", "trash("],
        bridge: &["open"],
    },
    Capability {
        id: "applications",
        title: "See what is installed",
        detail: "The list of programs on this machine, and which one opens what.",
        tokens: &[
            "getApplications",
            "getDefaultApplication",
            "getFrontmostApplication",
        ],
        bridge: &["applications", "default_application"],
    },
    Capability {
        id: "storage",
        title: "Keep data between runs",
        detail: "Its own store on disk, which survives closing the launcher.",
        tokens: &["LocalStorage", "Cache"],
        bridge: &[],
    },
    Capability {
        id: "ai",
        title: "Send text to a language model",
        detail: "Whatever it decides to send, on your account.",
        tokens: &["AI.ask", "canAccess(AI"],
        bridge: &[],
    },
    Capability {
        id: "browser",
        title: "Read what your browser has open",
        detail: "Needs Raycast's browser extension, which Sill does not have.",
        tokens: &["BrowserExtension"],
        bridge: &[],
    },
    Capability {
        id: "windows",
        title: "See and move windows",
        detail: "Sill does not answer this yet, so it will fail rather than act.",
        tokens: &["WindowManagement"],
        bridge: &[],
    },
    Capability {
        id: "dialog",
        title: "Put a dialog in front of you",
        detail: "Asks a yes or no question and waits for an answer.",
        tokens: &["confirmAlert"],
        bridge: &["confirm"],
    },
];

/// The sentence that keeps the list above honest.
///
/// Shown next to it, always, and not behind a disclosure. A capability list
/// that looks like a permission screen and enforces nothing is worse than no
/// list, because it invites a trust nothing here has earned.
pub const NOT_ENFORCED: &str = "Sill does not sandbox extensions. This is what the code \
    appears to use, not a limit on what it can do: an extension runs as a Node program with \
    your account's access, and a dependency it installs can do anything it does. Install what \
    you would run.";

/// A capability the source appears to reach, and where it was seen.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reached {
    pub id: String,
    pub title: String,
    pub detail: String,
    /// Up to a few files it was seen in, so the claim can be checked.
    pub seen_in: Vec<String>,
    /// Whether this one goes through Sill at all.
    ///
    /// The distinction worth showing, and the only one here that is a fact
    /// about the architecture rather than about the extension. A mediated
    /// capability passes through [`crate::host_bridge`], which is one file, can
    /// be logged, and is where a permission check would go if there is ever one.
    /// An unmediated one is the extension using Node directly: Sill does not
    /// see it, cannot log it, and could not refuse it. Reading a file is the
    /// second kind, which is exactly why saying so matters.
    pub mediated: bool,
}

/// How many files are named per capability.
///
/// Three. The point of naming any is that somebody can go and look; a list of
/// forty file names is not something anybody looks at.
const NAMED: usize = 3;

/// Whether `text` uses `token` as an identifier rather than as part of a
/// longer word.
///
/// The preceding character decides it. Without this, `open(` matches
/// `fs.open(` and `Cache` matches `NoCache`, and a capability list that
/// reports things the extension never touches is one nobody reads.
pub fn mentions(text: &str, token: &str) -> bool {
    let mut from = 0;

    while let Some(offset) = text[from..].find(token) {
        let at = from + offset;
        let before = text[..at].chars().last();

        if before.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '$' && c != '.') {
            return true;
        }

        from = at + token.len();
    }

    false
}

/// The extensions of files that are read.
///
/// The extension's own source only. `node_modules` is deliberately not walked:
/// it is not fetched at scan time, it is megabytes of somebody else's code,
/// and reporting on it would produce a list that says every extension does
/// everything.
pub const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs"];

/// Whether a file is one of the extension's own sources.
pub fn is_source(path: &str) -> bool {
    let lower = path.to_lowercase();

    if lower.contains("node_modules/") || lower.contains("node_modules\\") {
        return false;
    }

    SOURCE_EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(&format!(".{ext}")))
}

/// What these sources appear to reach.
///
/// Takes the files as values rather than reading a directory, which is what
/// makes every case here a test instead of a tree somebody has to build.
pub fn reached(sources: &[(String, String)]) -> Vec<Reached> {
    CAPABILITIES
        .iter()
        .filter_map(|capability| {
            let seen: Vec<String> = sources
                .iter()
                .filter(|(path, body)| {
                    is_source(path) && capability.tokens.iter().any(|token| mentions(body, token))
                })
                .map(|(path, _)| path.clone())
                .take(NAMED)
                .collect();

            (!seen.is_empty()).then(|| Reached {
                id: capability.id.to_string(),
                title: capability.title.to_string(),
                detail: capability.detail.to_string(),
                mediated: !capability.bridge.is_empty(),
                seen_in: seen,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(body: &str) -> Vec<(String, String)> {
        vec![("src/index.tsx".to_string(), body.to_string())]
    }

    fn ids(sources: &[(String, String)]) -> Vec<String> {
        reached(sources).into_iter().map(|it| it.id).collect()
    }

    /// **The guard that keeps this file honest.**
    ///
    /// Every capability an extension can ask of Sill goes through one trait,
    /// and this reads that trait's source and refuses a method nobody named
    /// here. Without it the seam grows a method, the store keeps showing the
    /// old list, and the omission is invisible: the store still renders, the
    /// extension still runs, and the thing that was supposed to be the whole
    /// point quietly stops being true.
    #[test]
    fn every_bridge_method_is_named_by_some_capability() {
        const BRIDGE: &str = include_str!("../exthost/bridge.rs");

        let start = BRIDGE
            .find("pub trait Bridge")
            .expect("the bridge trait is still called Bridge");
        // Its methods run to the first closing brace at the start of a line,
        // which is where a top level item ends.
        let body = &BRIDGE[start..];
        let end = body.find("\n}").expect("the trait closes");

        let declared: Vec<&str> = body[..end]
            .split("fn ")
            .skip(1)
            .filter_map(|rest| rest.split('(').next())
            .map(str::trim)
            .collect();

        assert!(
            declared.len() >= 9,
            "only found {declared:?}, so the parse is wrong rather than the list"
        );

        let named: Vec<&str> = CAPABILITIES
            .iter()
            .flat_map(|capability| capability.bridge.iter().copied())
            .collect();

        for method in &declared {
            assert!(
                named.contains(method),
                "Bridge::{method} is something an extension can do and no capability \
                 in CAPABILITIES names it, so the store would not tell anybody about it"
            );
        }

        for method in &named {
            assert!(
                declared.contains(method),
                "CAPABILITIES names Bridge::{method}, which no longer exists"
            );
        }
    }

    #[test]
    fn every_capability_has_at_least_one_thing_that_reveals_it() {
        for capability in CAPABILITIES {
            assert!(
                !capability.tokens.is_empty(),
                "{} can never be reported",
                capability.id
            );
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for capability in CAPABILITIES {
            assert!(seen.insert(capability.id), "{} is listed twice", capability.id);
        }
    }

    #[test]
    fn a_token_inside_a_longer_word_is_not_a_use_of_it() {
        assert!(mentions("await open(url)", "open("));
        assert!(!mentions("fs.open(path)", "open("), "that is a different open");
        assert!(!mentions("reopen(x)", "open("));

        assert!(mentions("import { Cache } from \"@raycast/api\"", "Cache"));
        assert!(!mentions("class NoCache {}", "Cache"));
    }

    #[test]
    fn the_source_of_an_extension_that_reaches_nothing_reports_nothing() {
        assert!(reached(&source("export default function () { return null; }")).is_empty());
    }

    #[test]
    fn running_a_program_and_reading_files_are_both_found() {
        let found = ids(&source(
            "import { execSync } from \"child_process\";\nimport { readFileSync } from \"node:fs\";",
        ));

        assert!(found.contains(&"processes".to_string()));
        assert!(found.contains(&"filesystem".to_string()));
    }

    #[test]
    fn the_worst_thing_it_can_do_is_reported_first() {
        let found = ids(&source(
            "import { Clipboard } from \"@raycast/api\";\nimport { execSync } from \"child_process\";",
        ));

        assert_eq!(found, vec!["processes".to_string(), "clipboard".to_string()]);
    }

    /// A dependency is not scanned, and must not be scanned: the report would
    /// say every extension does everything.
    #[test]
    fn nothing_under_node_modules_is_read() {
        let found = ids(&[(
            "node_modules/left-pad/index.js".to_string(),
            "require(\"child_process\")".to_string(),
        )]);

        assert!(found.is_empty());
    }

    #[test]
    fn only_source_files_are_read() {
        assert!(is_source("src/index.tsx"));
        assert!(is_source("src/lib/helper.ts"));
        assert!(!is_source("README.md"));
        assert!(!is_source("assets/icon.png"));
        assert!(!is_source("package.json"));
    }

    /// The distinction the trust panel turns on: what Sill can see and what it
    /// never does.
    #[test]
    fn a_capability_says_whether_sill_is_even_in_the_way() {
        let through = reached(&source("import { Clipboard } from \"@raycast/api\";"));
        assert!(through[0].mediated, "the clipboard goes through the bridge");

        let around = reached(&source("import { execSync } from \"child_process\";"));
        assert!(
            !around[0].mediated,
            "running a program is Node, and Sill neither sees nor could refuse it"
        );
    }

    #[test]
    fn a_capability_says_where_it_was_seen_and_names_only_a_few() {
        let sources: Vec<(String, String)> = (0..10)
            .map(|n| (format!("src/{n}.ts"), "execSync(\"x\")".to_string()))
            .collect();

        let found = reached(&sources);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].seen_in.len(), NAMED);
        assert_eq!(found[0].seen_in[0], "src/0.ts");
    }
}
