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
//! **This screen is now the grant.** Agreeing to it writes the `grants` column
//! of each row it listed into [`crate::exthost::grants`], which is what the
//! worker checks before it answers a call and before it hands out `fs`, `net`
//! or `child_process`. Saying yes here is the answer to every card that would
//! otherwise have been raised for those.
//!
//! It was not always. The first version described what the code reached and
//! enforced nothing, while grants defaulted to nothing and the worker refused
//! at `require`, which is module load and has no RPC to hang a card on. So an
//! extension died before it rendered: **86 of the 104 commands in the twelve
//! most-installed extensions**, measured. Two halves using one word for
//! different things and never meeting.
//!
//! **There is still no sandbox**, and that is the part [`NOT_ENFORCED`] says
//! out loud next to the list. What is granted is what the code appeared to
//! reach, read by substring search over its own source, and a dependency can
//! do anything the extension can. The permission layer decides what the host
//! hands over; it does not confine a process.
//!
//! What is real:
//!
//! - The source shown is the source installed, at a commit that is recorded.
//! - `npm` runs with `--ignore-scripts`, so no package's install hook executes.
//! - Everything an extension asks **of Sill** goes through
//!   [`crate::host_bridge`], which is one file, and every method on it is
//!   named below. A test refuses to compile a bridge method nobody named.
//! - Nothing here grants a permission the host does not enforce, and a test
//!   reads `exthost::permission::NEEDED` to be sure of it.
//! - The loudest ones are deliberately withheld. Accepting the clipboard does
//!   not accept typing into somebody else's window.
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

/// The vocabulary the host enforces in, aliased so this file's own
/// `Capability` (a row on the screen) and the launcher's (a permission the
/// worker checks) cannot be mistaken for one another.
use crate::action::Capability as Permission;

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
    /// What agreeing to this actually grants.
    ///
    /// **This column is what makes the install screen mean something.** Before
    /// it existed, the screen described what the code reached and enforced
    /// nothing, while `exthost::grants` refused everything until somebody
    /// answered a card. The two used the same word for different things and
    /// never met, so 86 of the 104 commands in the most-installed extensions
    /// died at `require` before they rendered.
    ///
    /// Written here rather than mapped somewhere else, so the row that says
    /// what a capability is also says what it costs. Empty means agreeing
    /// grants nothing: reading its own settings and its own storage are not
    /// permissions and there is nothing to hand over.
    ///
    /// Deliberately narrower than the description in places. `clipboard`
    /// grants reading and writing and **not** `InputInjection`, because
    /// pasting types into whatever window is in front, and that one is worth
    /// asking about at the moment it happens even for an extension somebody
    /// already accepted.
    grants: &'static [Permission],
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
        grants: &[Permission::ProcessLaunch],
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
        grants: &[Permission::FileRead, Permission::FileWrite],
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
        grants: &[Permission::Network],
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
        grants: &[Permission::ClipboardRead, Permission::ClipboardWrite],
    },
    Capability {
        id: "selection",
        title: "Read the text you have selected",
        detail: "Whatever is highlighted in the window Sill came up over.",
        tokens: &["getSelectedText"],
        bridge: &["selected_text"],
        grants: &[Permission::SelectionRead],
    },
    Capability {
        id: "secrets",
        title: "Read its own settings",
        detail: "Anything you type into it, including passwords and API keys.",
        tokens: &["getPreferenceValues"],
        bridge: &[],
        grants: &[],
    },
    Capability {
        id: "oauth",
        title: "Sign you in to another service",
        detail: "Opens a browser to authorise it, and keeps the token.",
        tokens: &["OAuth"],
        bridge: &[],
        grants: &[Permission::Network],
    },
    Capability {
        id: "open",
        title: "Open files and links",
        detail: "Hands a path or an address to whatever program handles it.",
        tokens: &["open(", "trash("],
        bridge: &["open"],
        grants: &[Permission::ProcessLaunch],
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
        grants: &[Permission::FileRead],
    },
    Capability {
        id: "storage",
        title: "Keep data between runs",
        detail: "Its own store on disk, which survives closing the launcher.",
        tokens: &["LocalStorage", "Cache"],
        bridge: &[],
        grants: &[],
    },
    Capability {
        id: "ai",
        title: "Send text to a language model",
        detail: "Whatever it decides to send, on your account.",
        tokens: &["AI.ask", "canAccess(AI"],
        bridge: &[],
        grants: &[Permission::Network],
    },
    Capability {
        id: "browser",
        title: "Read what your browser has open",
        detail: "Needs Raycast's browser extension, which Sill does not have.",
        tokens: &["BrowserExtension"],
        bridge: &[],
        grants: &[],
    },
    Capability {
        id: "windows",
        title: "See and move windows",
        detail: "Sill does not answer this yet, so it will fail rather than act.",
        tokens: &["WindowManagement"],
        bridge: &[],
        // Nothing, because nothing answers it. `WindowManagement` is not on
        // the bridge and no method needs `WindowControl`, so granting it would
        // write a permission into the file that gates nothing and reads as
        // though somebody had handed over their windows. The row still appears
        // on the screen, saying the extension expects something Sill does not
        // do, which is the useful half.
        grants: &[],
    },
    Capability {
        id: "dialog",
        title: "Put a dialog in front of you",
        detail: "Asks a yes or no question and waits for an answer.",
        tokens: &["confirmAlert"],
        bridge: &["confirm"],
        grants: &[Permission::Ui],
    },
];

/// What agreeing to these capabilities grants, with nothing repeated.
///
/// The join between the screen somebody reads and the permissions the host
/// enforces. Both sides speak [`Capability`](crate::action::Capability), which
/// is the whole reason this is a lookup rather than a translation.
///
/// Ids it does not recognise are skipped rather than refused. The only source
/// of them is [`reached`], so an unknown one means this table changed under a
/// record written by an older build, and the safe reading of "a permission I
/// have never heard of" is to not grant it.
pub fn granted_by(ids: &[String]) -> Vec<Permission> {
    let mut out: Vec<Permission> = Vec::new();

    for id in ids {
        let Some(capability) = CAPABILITIES.iter().find(|it| it.id == *id) else {
            continue;
        };

        for granted in capability.grants {
            if !out.contains(granted) {
                out.push(*granted);
            }
        }
    }

    out
}

/// The capabilities the worker enforces at `require`, rather than per call.
///
/// These are the ones a **dependency** can need without the extension's own
/// source mentioning them, which is the whole reason the bundle is scanned as
/// well as the source. Named by id rather than by module, because the module
/// list belongs to `patch-require.ts` and this is about which rows of the
/// table above can be reached that way.
const GATED_AT_REQUIRE: &[&str] = &["processes", "filesystem", "network"];

/// What a built bundle actually requires, which is not what its source says.
///
/// **The gap this closes.** [`reached`] reads the extension's own source,
/// deliberately: scanning `node_modules` would report that every extension
/// does everything. But the bundle esbuild produces has the dependencies
/// inlined, so `require("fs")` can appear in the thing that runs without
/// appearing in anything a person wrote.
///
/// Measured across the twelve most-installed extensions: after granting
/// exactly what the source appeared to reach, **23 of 124 commands still died
/// at `require`** on a module a dependency wanted. `google-search`, `notion`,
/// `spotify-player` and `chatgpt` were all refused something their own code
/// never mentions.
///
/// Only the three that are gated at load are looked for. A bundle is hundreds
/// of kilobytes of somebody else's code and every token in the table would
/// match something in it, so widening this would grant far more than it
/// should. What is gated per call is still asked about on the card.
pub fn required_by_bundle(text: &str) -> Vec<String> {
    CAPABILITIES
        .iter()
        .filter(|capability| GATED_AT_REQUIRE.contains(&capability.id))
        .filter(|capability| capability.tokens.iter().any(|token| mentions(text, token)))
        .map(|capability| capability.id.to_string())
        .collect()
}

/// Every permission an extension can be given, in the order the table lists.
///
/// The union of the `grants` column, which makes it derived rather than a
/// fourth hand-written list of permissions. What can be granted is exactly
/// what installing can grant, and the test above already refuses a row that
/// grants something the host does not enforce.
///
/// This is what the settings screen offers. Anything absent is absent because
/// nothing checks it, and offering a switch that changes nothing is worse than
/// offering none.
pub fn grantable() -> Vec<Permission> {
    let mut out: Vec<Permission> = Vec::new();

    for capability in CAPABILITIES {
        for granted in capability.grants {
            if !out.contains(granted) {
                out.push(*granted);
            }
        }
    }

    out
}

/// The sentence that keeps the list above honest.
///
/// Shown next to it, always, and not behind a disclosure. A capability list
/// that looks like a permission screen and enforces nothing is worse than no
/// list, because it invites a trust nothing here has earned.
pub const NOT_ENFORCED: &str = "Installing grants these, and Sill refuses them until you do. \
    It is not a sandbox: an extension runs as a Node program with your account's access, this \
    is what its own code appears to use, and a dependency it installs can do anything it does. \
    Anything not listed is still asked for when it happens. Install what you would run.";

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

    /// **The guard on the join.**
    ///
    /// Every permission this table hands out has to be one the host actually
    /// enforces, or it is a word in a file that stops nothing while looking
    /// like it stops something.
    ///
    /// There are **two** things that enforce, which is the part worth writing
    /// down. `exthost::permission::NEEDED` covers the methods an extension can
    /// call, and `host/src/worker/patch-require.ts` covers the Node modules it
    /// can load. Checking only the first is how this test first failed:
    /// `FileWrite` is needed by no method at all, because writing a file is
    /// never an RPC. It is `require("fs")`, which the worker gates.
    ///
    /// Both are read rather than repeated. The TypeScript names are the ones
    /// Rust serialises, which is what lets a `Capability` be compared against
    /// them at all, and that agreement is itself worth failing over.
    #[test]
    fn nothing_is_granted_that_neither_half_of_the_host_enforces() {
        let by_method: Vec<Permission> = crate::exthost::permission::NEEDED
            .iter()
            .flat_map(|(_, needs)| needs.iter().copied())
            .collect();

        const GATE: &str = include_str!("../../../host/src/worker/patch-require.ts");

        let named = |permission: &Permission| {
            serde_json::to_value(permission)
                .ok()
                .and_then(|it| it.as_str().map(str::to_string))
                .expect("a capability serialises to a name")
        };

        for capability in CAPABILITIES {
            for granted in capability.grants {
                let by_module = GATE.contains(&format!("\"{}\"", named(granted)));

                assert!(
                    by_method.contains(granted) || by_module,
                    "{} grants {granted:?}, which no method in \
                     exthost::permission::NEEDED asks for and no module in \
                     patch-require.ts gates, so granting it means nothing",
                    capability.id
                );
            }
        }
    }

    /// The two halves have to agree on what a permission is called.
    ///
    /// The worker compares strings it was handed against names Rust wrote, so
    /// a rename on either side turns every gated module into one nobody can
    /// ever be granted, silently: the extension is refused, the person grants
    /// it, and it is refused again.
    #[test]
    fn the_module_gate_spells_permissions_the_way_rust_serialises_them() {
        const GATE: &str = include_str!("../../../host/src/worker/patch-require.ts");

        // Every name the gate asks for, taken out of its own `needs` lists.
        let mut asked: Vec<String> = Vec::new();
        for piece in GATE.split("needs: [").skip(1) {
            let list = piece.split(']').next().unwrap_or_default();
            for name in list.split('"').skip(1).step_by(2) {
                if !asked.iter().any(|it| it == name) {
                    asked.push(name.to_string());
                }
            }
        }

        assert!(
            asked.len() >= 3,
            "only found {asked:?} in the gate, so this test is parsing rather than checking"
        );

        let known: Vec<String> = crate::exthost::permission::NEEDED
            .iter()
            .flat_map(|(_, needs)| needs.iter())
            .chain(CAPABILITIES.iter().flat_map(|it| it.grants.iter()))
            .filter_map(|permission| {
                serde_json::to_value(permission)
                    .ok()?
                    .as_str()
                    .map(str::to_string)
            })
            .collect();

        for name in &asked {
            assert!(
                known.contains(name),
                "the module gate asks for {name:?}, which is not how Rust spells any \
                 capability, so it can never be granted"
            );
        }
    }

    /// Pasting types into whatever window is in front, and that is worth a
    /// question at the moment it happens even for an extension somebody has
    /// already accepted.
    #[test]
    fn accepting_the_clipboard_does_not_also_accept_typing() {
        let granted = granted_by(&["clipboard".to_string()]);

        assert!(granted.contains(&Permission::ClipboardRead));
        assert!(granted.contains(&Permission::ClipboardWrite));
        assert!(
            !granted.contains(&Permission::InputInjection),
            "pasting is a keystroke into somebody else's window"
        );
    }

    #[test]
    fn what_reaches_nothing_outside_itself_grants_nothing() {
        // Its own settings and its own store. Asking about these would teach
        // people to click through the ones that matter.
        assert!(granted_by(&["secrets".to_string(), "storage".to_string()]).is_empty());
    }

    #[test]
    fn a_capability_from_an_older_build_grants_nothing_rather_than_failing() {
        assert!(granted_by(&["invented-later".to_string()]).is_empty());
    }

    #[test]
    fn the_same_permission_reached_two_ways_is_granted_once() {
        // `open` and `processes` both come down to launching something.
        let granted = granted_by(&["open".to_string(), "processes".to_string()]);

        assert_eq!(
            granted
                .iter()
                .filter(|it| **it == Permission::ProcessLaunch)
                .count(),
            1
        );
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
            assert!(
                seen.insert(capability.id),
                "{} is listed twice",
                capability.id
            );
        }
    }

    #[test]
    fn a_token_inside_a_longer_word_is_not_a_use_of_it() {
        assert!(mentions("await open(url)", "open("));
        assert!(
            !mentions("fs.open(path)", "open("),
            "that is a different open"
        );
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

        assert_eq!(
            found,
            vec!["processes".to_string(), "clipboard".to_string()]
        );
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
