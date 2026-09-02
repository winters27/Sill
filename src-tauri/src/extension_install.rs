//! Installing an extension from a folder on this machine.
//!
//! `scripts/build-extension.mjs` has done this since the beginning and can only
//! ever do it here: it imports esbuild out of `host/node_modules` and writes
//! into the working tree, so an installed Sill had a loader for extensions and
//! no way to acquire one. This is the same build, in Rust, against directories
//! an installed copy actually has.
//!
//! ## What it does and does not do
//!
//! It bundles, it does not resolve a registry. Pointing at a folder is the
//! whole install path for now, which covers a developer building their own and
//! anybody who has cloned one. A store is P3 and deliberately later: it is a
//! different problem, made of trust and updates rather than of transpiling.
//!
//! ## Why esbuild rather than a parser of our own
//!
//! Extensions are TypeScript and JSX and Node runs neither. The alternative to
//! a real transpiler is writing one, and the input is somebody else's code, so
//! its long tail of silent wrongness would surface as an extension that builds
//! and then behaves oddly, which is the least reportable bug there is. esbuild
//! was already the choice `build-extension.mjs` made; this makes it a shipped
//! one rather than a development one.
//!
//! ## Why the pure parts are pure
//!
//! Everything that decides anything (which file is a command's entrypoint,
//! what its record says, how an index merges, what a tsconfig aliases) is a
//! function over data with no filesystem and no subprocess in it. Only
//! `bundle` and `install` touch the machine. That is what makes the awkward
//! cases testable at all: a manifest with two commands sharing a preference
//! name is a value, not a directory somebody has to create.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::registry::CommandRecord;

/// The parts of an extension's `package.json` that matter here.
///
/// Deliberately not the whole manifest. Everything Raycast puts in one that
/// Sill has no use for is left unread rather than modelled, so a field added
/// upstream cannot fail an install.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub title: Option<String>,
    #[serde(default)]
    pub commands: Vec<ManifestCommand>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestCommand {
    pub name: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub mode: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub preferences: Vec<Preference>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Preference {
    pub name: String,
    pub default: Option<Value>,
}

/// The file extensions a command's source may have, in the order Raycast looks.
const SOURCE_EXTENSIONS: [&str; 4] = ["tsx", "ts", "jsx", "js"];

/// Which file under `src/` is this command, given what exists.
///
/// Takes a predicate rather than reading the disk so the ordering is testable:
/// an extension carrying both `foo.ts` and `foo.js` must resolve to the same
/// one every time, and the only way to be sure is to ask with both present.
pub fn entrypoint_for(command: &str, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    SOURCE_EXTENSIONS
        .iter()
        .map(|ext| PathBuf::from("src").join(format!("{command}.{ext}")))
        .find(|candidate| exists(candidate))
}

/// What `getPreferenceValues()` answers before anybody has changed anything.
///
/// The extension's own preferences first, then the command's over the top,
/// because a command that redeclares one means its own. **Only preferences
/// that declare a default appear**: one that does not is genuinely unset until
/// somebody sets it, and inventing a value would be worse than the `undefined`
/// the extension already has to guard against.
pub fn default_preferences(manifest: &Manifest, command: &ManifestCommand) -> Value {
    let mut collected = Map::new();

    for list in [&manifest.preferences, &command.preferences] {
        for preference in list {
            if let Some(default) = &preference.default {
                collected.insert(preference.name.clone(), default.clone());
            }
        }
    }

    Value::Object(collected)
}

/// The index record for one built command.
///
/// `CommandRecord` itself rather than a shape of this module's own, because
/// the index is read back into exactly that type. A second struct here would
/// be two spellings of one file format with nothing keeping them in step, and
/// the failure would be an extension that installs and cannot be found.
pub fn record_for(
    manifest: &Manifest,
    command: &ManifestCommand,
    entrypoint: &Path,
) -> CommandRecord {
    let extension_title = manifest
        .title
        .clone()
        .unwrap_or_else(|| manifest.name.clone());

    CommandRecord {
        id: format!("{}:{}", manifest.name, command.name),
        extension: manifest.name.clone(),
        extension_title: extension_title.clone(),
        command: command.name.clone(),
        title: command
            .title
            .clone()
            .unwrap_or_else(|| command.name.clone()),
        subtitle: command.subtitle.clone().unwrap_or(extension_title),
        description: command.description.clone().unwrap_or_default(),
        mode: command.mode.clone(),
        // Forward slashes, because this string is handed to Node.
        entrypoint: entrypoint.to_string_lossy().replace('\\', "/"),
        keywords: command.keywords.clone(),
        icon: None,
        toggle: None,
        panel: None,
        preferences: default_preferences(manifest, command),
    }
}

/// The index after installing these records into it.
///
/// Replaces by id rather than appending, so installing an extension a second
/// time updates it instead of listing every command twice, and sorts so the
/// file does not churn between installs. **Anything already there and not
/// being replaced is kept**: an install is about one extension and must not
/// quietly uninstall the others, which is the same rule the snippet and
/// quicklink imports follow.
pub fn merged_index(
    existing: Vec<CommandRecord>,
    installing: Vec<CommandRecord>,
) -> Vec<CommandRecord> {
    let mut by_id: BTreeMap<String, CommandRecord> = existing
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect();

    for record in installing {
        by_id.insert(record.id.clone(), record);
    }

    by_id.into_values().collect()
}

/// The index with one extension's commands taken out.
///
/// The inverse of [`merged_index`] and the other half of being able to trust a
/// store: code that can arrive has to be able to leave. Matched on the
/// `extension` field rather than on the id prefix, because an extension named
/// `git` and one named `github` share a prefix and removing the first would
/// take half of the second with it.
pub fn without_extension(existing: Vec<CommandRecord>, extension: &str) -> Vec<CommandRecord> {
    existing
        .into_iter()
        .filter(|record| record.extension != extension)
        .collect()
}

/// The path aliases an extension declares, as esbuild wants them.
///
/// Extensions commonly alias `@/...` to their own `src`. esbuild does not read
/// tsconfig paths reliably for bare aliases across versions, so they are lifted
/// out and passed explicitly.
///
/// A tsconfig that cannot be understood yields nothing rather than failing the
/// install: aliases are an optimisation on top of an extension that mostly
/// imports relatively, and refusing to install over a comment we could not
/// strip would be a poor trade.
pub fn aliases_from_tsconfig(text: &str) -> Vec<(String, String)> {
    // tsconfig allows comments, which JSON rejects. Only whole-line ones, the
    // same subset the script this replaces handled: a general comment stripper
    // has to understand strings, and getting that wrong would silently change
    // a path rather than fail.
    let stripped: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    let Ok(parsed) = serde_json::from_str::<Value>(&stripped) else {
        return Vec::new();
    };

    let options = &parsed["compilerOptions"];
    let base_url = options["baseUrl"].as_str().unwrap_or(".").to_string();

    let Some(paths) = options["paths"].as_object() else {
        return Vec::new();
    };

    paths
        .iter()
        .filter_map(|(pattern, targets)| {
            let target = match targets {
                Value::Array(list) => list.first()?.as_str()?,
                Value::String(one) => one.as_str(),
                _ => return None,
            };

            Some((
                pattern.trim_end_matches("/*").to_string(),
                format!(
                    "{}/{}",
                    base_url.trim_end_matches('/'),
                    target.trim_end_matches("/*")
                ),
            ))
        })
        .collect()
}

/// The arguments esbuild is run with for one command.
///
/// Separated from running it so the flags can be asserted. Three of them are
/// load-bearing and none is obvious from reading the call:
///
/// - `--format=cjs`, because every Raycast extension bundle is CommonJS.
/// - `--external:` for `@raycast/api` and React, because **the host supplies
///   both at runtime**. Bundling either gives the worker a second copy of
///   React and hooks fail in ways that read as the extension's fault.
/// - `--jsx=automatic`, since extensions do not import React to use JSX.
pub fn esbuild_args(entry: &Path, outfile: &Path, aliases: &[(String, String)]) -> Vec<String> {
    let mut args = vec![
        entry.to_string_lossy().into_owned(),
        "--bundle".into(),
        "--platform=node".into(),
        "--format=cjs".into(),
        "--target=node20".into(),
        "--jsx=automatic".into(),
        "--jsx-import-source=react".into(),
        "--log-level=warning".into(),
        format!("--outfile={}", outfile.to_string_lossy()),
    ];

    for external in [
        "@raycast/api",
        "react",
        "react/jsx-runtime",
        "react/jsx-dev-runtime",
    ] {
        args.push(format!("--external:{external}"));
    }

    for (from, to) in aliases {
        args.push(format!("--alias:{from}={to}"));
    }

    args
}

/// The marker that pins a built bundle back to CommonJS.
///
/// Sill's own `package.json` declares `"type": "module"` and that scope reaches
/// down into wherever the bundles land, so without this Node loads a CommonJS
/// bundle as an ES module and fails on `module`. Written beside the output
/// rather than into it.
pub const COMMONJS_MARKER: &str = "{\n  \"type\": \"commonjs\",\n  \"private\": true\n}\n";

/// What an install did, for the window to report.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub extension: String,
    pub title: String,
    /// Named rather than counted. "Installed 3 commands" is a number; the
    /// titles are what somebody types next.
    pub commands: Vec<String>,
}

/// Where esbuild is, if this copy of Sill has one.
///
/// The same three-candidate chain [`crate::host::host_js`] uses and for the
/// same reason: a development build finds it in `host/node_modules`, an
/// installed one finds the bundled resource, and an override exists so a
/// different build can be pointed at without reinstalling. **Only a candidate
/// that exists is returned.** Handing back a path that is not there produces a
/// spawn failure several steps later, naming a file nobody chose.
#[cfg(windows)]
pub fn esbuild_exe(app: &tauri::AppHandle) -> Option<PathBuf> {
    use tauri::Manager;

    let bundled = app
        .path()
        .resolve("esbuild/esbuild.exe", tauri::path::BaseDirectory::Resource)
        .ok();

    let candidates = [
        std::env::var_os("SILL_ESBUILD").map(PathBuf::from),
        bundled,
        Some(
            crate::host::dev_root()
                .join("host")
                .join("node_modules")
                .join("@esbuild")
                .join("win32-x64")
                .join("esbuild.exe"),
        ),
    ];

    candidates.into_iter().flatten().find(|path| path.exists())
}

/// Said when there is no esbuild to build with.
///
/// Names what is missing and what it is for, because "install failed" about a
/// transpiler nobody knew was involved is not something a person can act on.
pub const NO_ESBUILD: &str =
    "Installing an extension needs esbuild, which is missing from this copy of Sill. \
     A development build gets it from `npm --prefix host ci`.";

/// Build one extension out of a folder and list its commands in the index.
///
/// The whole install path, and it is deliberately one function: reading a
/// manifest, bundling and writing the index are steps of a single operation
/// that is either done or not. Splitting them across commands would let a
/// window get half of it right.
///
/// `origin` says where the folder came from and is written beside the build.
/// **The store goes through this function rather than around it.** Acquiring
/// an extension and building one are separate problems, and having a second
/// installer for the store would be two answers to "what does installed mean"
/// with nothing keeping them in step.
#[cfg(windows)]
pub fn install(
    app: &tauri::AppHandle,
    source: &Path,
    origin: &crate::store::Origin,
) -> Result<Installed, String> {
    let Some(esbuild) = esbuild_exe(app) else {
        return Err(NO_ESBUILD.to_string());
    };

    install_into(
        &esbuild,
        &crate::store::extensions_home(&crate::state::data_dir(app)),
        source,
        origin,
    )
}

/// The same, told where everything is rather than asking a window.
///
/// The split exists so the install path can be exercised without a running
/// application, which is rule 20: the interesting behaviour here is fetching,
/// resolving, bundling and merging an index, and none of it is about a window.
/// [`install`] is the two lookups this cannot do for itself.
#[cfg(windows)]
pub fn install_into(
    esbuild: &Path,
    home: &Path,
    source: &Path,
    origin: &crate::store::Origin,
) -> Result<Installed, String> {
    let manifest_path = source.join("package.json");
    let text = std::fs::read_to_string(&manifest_path).map_err(|_| {
        format!(
            "No package.json in {}, so this is not an extension.",
            source.display()
        )
    })?;

    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|err| format!("{} could not be read: {err}", manifest_path.display()))?;

    if manifest.commands.is_empty() {
        return Err(format!("{} declares no commands.", manifest.name));
    }

    let aliases = std::fs::read_to_string(source.join("tsconfig.json"))
        .map(|text| aliases_from_tsconfig(&text))
        .unwrap_or_default();

    // Aliases are written relative to the extension, and esbuild is run from
    // wherever Sill happens to be.
    let aliases: Vec<(String, String)> = aliases
        .into_iter()
        .map(|(from, to)| (from, source.join(to).to_string_lossy().into_owned()))
        .collect();

    let dest = home.join(&manifest.name);
    std::fs::create_dir_all(&dest)
        .map_err(|err| format!("could not make {}: {err}", dest.display()))?;
    std::fs::write(dest.join("package.json"), COMMONJS_MARKER)
        .map_err(|err| format!("could not write the module marker: {err}"))?;

    let mut records = Vec::new();

    for command in &manifest.commands {
        let Some(relative) =
            entrypoint_for(&command.name, |candidate| source.join(candidate).is_file())
        else {
            return Err(format!(
                "{} declares the command \"{}\" and has no source for it under src/.",
                manifest.name, command.name
            ));
        };

        let outfile = dest.join(format!("{}.js", command.name));
        bundle(esbuild, &source.join(relative), &outfile, &aliases)?;
        records.push(record_for(&manifest, command, &outfile));
    }

    // Written before the index, so an extension that is listed is always one
    // whose provenance is recorded. The other order leaves a command in the
    // index that nothing can say where it came from.
    crate::store::write_origin(home, &manifest.name, origin)?;

    let index_path = crate::store::index_file(home);
    let merged = merged_index(crate::registry::load_index(&index_path), records);

    let written = serde_json::to_string_pretty(&merged)
        .map_err(|err| format!("could not write the extension index: {err}"))?;
    std::fs::write(&index_path, format!("{written}\n"))
        .map_err(|err| format!("could not write {}: {err}", index_path.display()))?;

    Ok(Installed {
        title: manifest
            .title
            .clone()
            .unwrap_or_else(|| manifest.name.clone()),
        commands: manifest
            .commands
            .iter()
            .map(|c| c.title.clone().unwrap_or_else(|| c.name.clone()))
            .collect(),
        extension: manifest.name,
    })
}

/// Run esbuild over one command.
///
/// Its own diagnostics are the useful ones when an extension will not build,
/// so they are carried out rather than replaced with a summary: "Could not
/// resolve ./helpers" names the line to fix, and "the extension failed to
/// build" names nothing.
#[cfg(windows)]
fn bundle(
    esbuild: &Path,
    entry: &Path,
    outfile: &Path,
    aliases: &[(String, String)],
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    // No console window for a subprocess of a launcher.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = std::process::Command::new(esbuild)
        .args(esbuild_args(entry, outfile, aliases))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("could not run esbuild: {err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim();

    Err(if said.is_empty() {
        format!("esbuild refused {} and said nothing", entry.display())
    } else {
        said.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).expect("manifest parses")
    }

    /// A `.ts` and a `.js` for one command must not resolve differently on two
    /// runs, which is what an unordered search would do.
    #[test]
    fn the_first_extension_in_order_wins_when_several_exist() {
        let both = |path: &Path| {
            matches!(
                path.to_string_lossy().replace('\\', "/").as_str(),
                "src/thing.js" | "src/thing.ts"
            )
        };

        assert_eq!(
            entrypoint_for("thing", both),
            Some(PathBuf::from("src").join("thing.ts")),
            "ts is looked for before js"
        );
    }

    #[test]
    fn a_command_with_no_source_resolves_to_nothing_rather_than_guessing() {
        assert_eq!(entrypoint_for("absent", |_| false), None);
    }

    /// The command's own preference wins, and one without a default stays out.
    #[test]
    fn a_command_preference_beats_the_extensions_and_undefaulted_ones_are_absent() {
        let parsed = manifest(
            r#"{
                "name": "demo",
                "preferences": [
                    { "name": "shared", "default": "extension" },
                    { "name": "nodefault" }
                ],
                "commands": [{
                    "name": "run",
                    "mode": "view",
                    "preferences": [{ "name": "shared", "default": "command" }]
                }]
            }"#,
        );

        let defaults = default_preferences(&parsed, &parsed.commands[0]);

        assert_eq!(defaults["shared"], "command");
        assert!(
            defaults.get("nodefault").is_none(),
            "a preference with no default is unset, not invented"
        );
    }

    #[test]
    fn a_record_falls_back_through_title_and_never_leaves_one_empty() {
        let parsed =
            manifest(r#"{ "name": "demo", "commands": [{ "name": "run", "mode": "view" }] }"#);

        let record = record_for(&parsed, &parsed.commands[0], Path::new("out/run.js"));

        assert_eq!(record.id, "demo:run");
        assert_eq!(
            record.title, "run",
            "a command with no title is named by its command"
        );
        assert_eq!(
            record.subtitle, "demo",
            "and its subtitle by the extension, which has no title either"
        );
        assert_eq!(record.entrypoint, "out/run.js");
    }

    /// Windows paths reach Node, which wants forward slashes.
    #[test]
    fn a_backslash_never_reaches_the_entrypoint() {
        let parsed =
            manifest(r#"{ "name": "demo", "commands": [{ "name": "run", "mode": "view" }] }"#);

        let record = record_for(
            &parsed,
            &parsed.commands[0],
            Path::new(r"C:\Users\x\extensions\demo\run.js"),
        );

        assert!(
            !record.entrypoint.contains('\\'),
            "got {}",
            record.entrypoint
        );
    }

    fn record(id: &str, title: &str) -> CommandRecord {
        let parsed = manifest(&format!(
            r#"{{ "name": "{}", "commands": [{{ "name": "{}", "mode": "view", "title": "{title}" }}] }}"#,
            id.split(':').next().unwrap(),
            id.split(':').nth(1).unwrap(),
        ));
        record_for(&parsed, &parsed.commands[0], Path::new("x.js"))
    }

    /// Installing twice updates rather than duplicating.
    #[test]
    fn installing_the_same_command_again_replaces_it() {
        let merged = merged_index(
            vec![record("demo:run", "old")],
            vec![record("demo:run", "new")],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].title, "new");
    }

    /// An extension whose name begins another's must not take it with it.
    #[test]
    fn removing_one_extension_matches_the_whole_name_not_a_prefix() {
        let kept = without_extension(
            vec![record("git:log", "a"), record("github:issues", "b")],
            "git",
        );

        let ids: Vec<&str> = kept.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["github:issues"]);
    }

    #[test]
    fn removing_something_that_is_not_there_changes_nothing() {
        let kept = without_extension(vec![record("demo:run", "a")], "absent");
        assert_eq!(kept.len(), 1);
    }

    /// The rule the snippet and quicklink imports follow: adding never removes.
    #[test]
    fn installing_one_extension_leaves_the_others_alone() {
        let merged = merged_index(
            vec![record("other:thing", "kept")],
            vec![record("demo:run", "added")],
        );

        let ids: Vec<&str> = merged.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["demo:run", "other:thing"],
            "sorted, and nothing dropped"
        );
    }

    #[test]
    fn a_tsconfig_with_comments_still_yields_its_aliases() {
        let aliases = aliases_from_tsconfig(
            r#"{
                // the alias every extension seems to use
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": { "@/*": ["src/*"] }
                }
            }"#,
        );

        assert_eq!(aliases, vec![("@".to_string(), "./src".to_string())]);
    }

    #[test]
    fn a_tsconfig_that_cannot_be_read_is_no_aliases_rather_than_a_failed_install() {
        assert!(aliases_from_tsconfig("{ this is not json").is_empty());
        assert!(aliases_from_tsconfig("{}").is_empty());
    }

    /// The three flags that decide whether a built extension actually runs.
    #[test]
    fn the_host_supplied_modules_are_left_out_of_the_bundle() {
        let args = esbuild_args(Path::new("in.tsx"), Path::new("out.js"), &[]);

        for external in ["@raycast/api", "react", "react/jsx-runtime"] {
            assert!(
                args.contains(&format!("--external:{external}")),
                "{external} must not be bundled, the host supplies it"
            );
        }

        assert!(args.contains(&"--format=cjs".to_string()));
        assert!(args.contains(&"--jsx=automatic".to_string()));
    }

    #[test]
    fn an_alias_reaches_esbuild_in_the_form_it_wants() {
        let args = esbuild_args(
            Path::new("in.tsx"),
            Path::new("out.js"),
            &[("@".into(), "./src".into())],
        );

        assert!(args.contains(&"--alias:@=./src".to_string()), "{args:?}");
    }
}
