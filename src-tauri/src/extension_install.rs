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
    /// What it declares it needs from npm, which is where the API version is.
    ///
    /// Read only for `@raycast/api`. Everything else in here is
    /// [`crate::store::install::packages_in`]'s business.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
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
    /// What the command wants typed before it starts.
    ///
    /// Raycast asks for these in the launcher's own bar and hands them to the
    /// command as `props.arguments`. They were not read at all, so a command
    /// declaring a required argument started with an empty object and threw on
    /// the destructure in its first line, which reads as the extension being
    /// broken rather than as Sill not having asked.
    #[serde(default)]
    pub arguments: Vec<Argument>,
}

/// One setting an extension declares.
///
/// `type` and `required` were the two fields that were not read, and both
/// decide how a settings screen has to draw it: a `checkbox` is a switch and a
/// `password` must not be shown, and a required one with no value is the
/// reason the command will fail before it renders.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preference {
    pub name: String,
    pub default: Option<Value>,
    /// `textfield`, `password`, `checkbox`, `dropdown`, `appPicker`, `file`,
    /// `directory`. Kept as written rather than as an enum, because a type
    /// Raycast adds must draw as a text field rather than fail to parse.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// What a checkbox says beside itself, which is where its real label is.
    pub label: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// The choices, for a dropdown.
    #[serde(default)]
    pub data: Vec<Choice>,
}

/// One option of a dropdown preference or argument.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct Choice {
    pub title: String,
    pub value: Value,
}

/// One thing a command asks for before it runs.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Argument {
    pub name: String,
    /// `text`, `password` or `dropdown`.
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub data: Vec<Choice>,
}

/// The API level Sill's host implements, in Raycast's own numbering.
///
/// The worker already tells every extension this through
/// `environment.raycastVersion`, and it was a literal in one file with nothing
/// else agreeing with it. Named here because installing is the point at which
/// an extension asking for something newer can be said out loud.
pub const RAYCAST_API_LEVEL: &str = "1.104.0";

/// Sill's own extension API version.
///
/// Separate from the number above on purpose. That one is a claim about
/// somebody else's surface; this is a claim about Sill's, and the two move for
/// different reasons: adopting a newer `@raycast/api` raises the first, and
/// changing what an extension may rely on here raises the second.
///
/// Written into every install's origin, so an extension built by a Sill that
/// promised something this one does not can be recognised rather than run.
pub const SILL_API_VERSION: u32 = 1;

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

    for preference in declared_preferences(manifest, command) {
        if let Some(default) = preference.default {
            collected.insert(preference.name, default);
        }
    }

    Value::Object(collected)
}

/// Every setting this command has, declared rather than defaulted.
///
/// The same precedence as [`default_preferences`] and for the same reason, but
/// carrying the whole declaration: a screen that lets somebody set these needs
/// the type to know what control to draw and the title to know what to call
/// it, and neither survives being reduced to a value.
///
/// The extension's own first, so a settings screen reads in the order the
/// manifest was written. A command redeclaring one replaces it in place rather
/// than adding a second row with the same name.
pub fn declared_preferences(manifest: &Manifest, command: &ManifestCommand) -> Vec<Preference> {
    let mut collected: Vec<Preference> = Vec::new();

    for preference in manifest.preferences.iter().chain(&command.preferences) {
        match collected
            .iter_mut()
            .find(|held| held.name == preference.name)
        {
            Some(held) => *held = preference.clone(),
            None => collected.push(preference.clone()),
        }
    }

    collected
}

// ------------------------------------------------------------- what will run

/// Why Sill cannot run this command, or nothing when it can.
///
/// Two reasons and they are different, so they say different things. A
/// `menu-bar` command is a status item beside the clock, which is a place a
/// launcher does not have; an unknown mode is something Raycast added after
/// this build, which is a fact about Sill rather than about the extension.
///
/// Asked of [`crate::exthost::CommandMode`], because that type is the one
/// place that decides what can run and a second opinion here is how the store
/// and the loader end up disagreeing.
pub fn why_not_runnable(mode: &str) -> Option<String> {
    if crate::exthost::CommandMode::from_manifest(mode).is_some() {
        return None;
    }

    Some(if mode == "menu-bar" {
        "it is a menu bar command, which is a status item beside the clock, and \
         a launcher has nowhere to put one"
            .to_string()
    } else {
        format!("this version of Sill does not know the mode \"{mode}\"")
    })
}

/// Every command in a manifest Sill would refuse, and why.
///
/// Refused **at install**, which is the point. A command whose mode nothing
/// here understands used to be built, listed and then loaded as a view, so the
/// first anybody knew of it was a window waiting for a tree that never
/// arrived. It is now never built and never in the index, and the install says
/// which ones went and why.
pub fn refused_commands(manifest: &Manifest) -> Vec<(String, String)> {
    manifest
        .commands
        .iter()
        .filter_map(|command| {
            why_not_runnable(&command.mode)
                .map(|because| (command.name.clone(), format!("{}: {because}", command.name)))
        })
        .collect()
}

/// What is said when refusing every command leaves nothing to install.
///
/// A separate answer from skipping one, because installing an extension that
/// adds nothing is indistinguishable from an install that silently failed.
pub fn nothing_left_to_install(manifest: &Manifest) -> Option<String> {
    let refused = refused_commands(manifest);

    if refused.len() != manifest.commands.len() || refused.is_empty() {
        return None;
    }

    Some(format!(
        "{} has no command Sill can run, so there is nothing to install. {}.",
        manifest.name,
        refused
            .iter()
            .map(|(_, said)| said.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

// ------------------------------------------------------------- api versions

/// The lowest version a dependency range accepts, as three numbers.
///
/// Deliberately not a semver implementation. What a manifest writes is
/// `^1.104.0` in almost every case and `>=1.50.0` in the rest, and the only
/// question being asked is "does this need something newer than Sill
/// implements". A range this cannot read answers `None`, which is treated as
/// no claim rather than as a failure: refusing an install over a version
/// string nobody can act on would be the worse mistake.
pub fn lowest_accepted(range: &str) -> Option<(u32, u32, u32)> {
    let first = range.split_whitespace().next()?;
    let digits = first.trim_start_matches(['^', '~', '>', '=', '<', 'v', ' ']);

    let mut parts = digits.split('.').map(|part| part.parse::<u32>().ok());
    let major = parts.next()??;

    Some((
        major,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

/// Said when an extension asks for a newer `@raycast/api` than Sill implements.
///
/// A warning rather than a refusal. The version an extension pins is the one
/// its author happened to have installed, not a list of what it uses, so
/// refusing on it would block extensions that work perfectly. What it buys is
/// that "a function is undefined" has an explanation somebody was shown before
/// they installed it.
pub fn api_ahead_of_sill(declared: Option<&str>, level: &str) -> Option<String> {
    let wanted = lowest_accepted(declared?)?;
    let ours = lowest_accepted(level)?;

    (wanted > ours).then(|| {
        format!(
            "It asks for @raycast/api {} and Sill implements {level}. \
             Anything added since then will be missing.",
            declared.unwrap_or_default()
        )
    })
}

/// What this manifest says about `@raycast/api`, if anything.
pub fn declared_api(manifest: &Manifest) -> Option<&str> {
    manifest
        .dependencies
        .get("@raycast/api")
        .map(String::as_str)
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
        manifest: Some(Box::new(crate::registry::Declared {
            preferences: declared_preferences(manifest, command),
            own: command
                .preferences
                .iter()
                .map(|it| it.name.clone())
                .collect(),
            arguments: command.arguments.clone(),
        })),
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

/// The index after installing one extension's commands over whatever it had.
///
/// **This is what an update means for the index, and [`merged_index`] alone is
/// not it.** Merging replaces an id it sees again and leaves an id it does not,
/// so a version that dropped a command left that command listed, pointing at a
/// bundle the new build no longer produces. Searching for it found it, running
/// it either failed or ran code the author had removed, and nothing on the
/// screen said the extension no longer had it.
///
/// So the extension's old entries go first and the new ones go in after: what
/// is listed for an extension is exactly what its manifest declares now.
/// Everything belonging to any other extension is untouched, which is the rule
/// [`merged_index`] already keeps.
pub fn reinstalled_index(
    existing: Vec<CommandRecord>,
    extension: &str,
    installing: Vec<CommandRecord>,
) -> Vec<CommandRecord> {
    merged_index(without_extension(existing, extension), installing)
}

/// Whether a folder is the one being installed into, or inside it.
///
/// Installing clears its destination, so a folder install pointed at the
/// installed copy of an extension would delete the source it was about to
/// build. Answered over resolved paths so the caller owns the filesystem
/// question and this stays a comparison.
pub fn inside(source: &Path, dest: &Path) -> bool {
    source.starts_with(dest)
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
    /// The commands that were refused, one sentence each.
    ///
    /// Said rather than silently dropped. An extension that declares four
    /// commands and installs three has to say which one went and why, or the
    /// missing one reads as the install having half worked.
    #[serde(default)]
    pub refused: Vec<String>,
}

/// How far along an install is, for the window to say while it happens.
///
/// npm and esbuild are the two slow parts and neither said anything until it
/// finished, so an install of a large extension was a word and a spinner for
/// a minute and a half. Their own output is the useful content: npm names the
/// package it is fetching and esbuild names the file it is on.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "stage", rename_all = "camelCase")]
pub enum Progress {
    /// npm is fetching this extension's dependencies. `said` is its own line.
    Dependencies { said: String },
    /// esbuild is on one command, this many of that many.
    Building {
        command: String,
        done: usize,
        total: usize,
    },
    /// What esbuild said on the way.
    Bundling { said: String },
}

/// Somewhere for an install to report to.
///
/// A plain function rather than a channel or an app handle, so nothing about
/// building an extension has to know a window exists. The command layer passes
/// the closure that emits, which is the same seam `finish` already keeps with
/// esbuild and Node.
pub type Report<'a> = &'a dyn Fn(Progress);

/// How long esbuild gets for one command.
///
/// A bundle of a large extension is a second or two, so a minute is not a
/// budget, it is the point at which something is wrong. What it protects
/// against is esbuild waiting on something that is not coming: it reads paths
/// out of a tsconfig this code hands it, and a path that resolves onto a dead
/// network share does not fail, it hangs.
pub const BUNDLE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);

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
    install_into_reporting(esbuild, home, source, origin, &|_| {})
}

/// The same, saying what it is doing as it does it.
///
/// ## An update replaces rather than adds to
///
/// This used to build into the installed directory and merge into the index,
/// which meant a version was **added to** the one before it. A command the new
/// manifest no longer declares kept its bundle on disk and its entry in the
/// index, so it was still searchable and still ran, which is the author having
/// removed a command and Sill still offering it. That is not untidiness; it is
/// code running that somebody withdrew.
///
/// So the build happens in a directory of its own beside the destination and
/// replaces it whole. Nothing from the previous version survives, and a build
/// that fails leaves the previous version exactly as it was, which building in
/// place could not promise either.
#[cfg(windows)]
pub fn install_into_reporting(
    esbuild: &Path,
    home: &Path,
    source: &Path,
    origin: &crate::store::Origin,
    report: Report<'_>,
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

    // Refused here rather than found out later. A command whose mode nothing
    // can run was built, listed and then loaded as a view, so the first sign
    // of it was a window waiting for a tree that never arrives.
    if let Some(said) = nothing_left_to_install(&manifest) {
        return Err(said);
    }
    let refused: Vec<String> = refused_commands(&manifest)
        .into_iter()
        .map(|(_, said)| said)
        .collect();

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

    // Installing clears its destination, so a folder install pointed at the
    // installed copy would delete the source it is about to read. Resolved
    // first, because `extensions\demo` and `extensions\.\demo` are the same
    // directory and only one of them looks like it.
    let resolved = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if inside(&resolved(source), &resolved(&dest)) {
        return Err(format!(
            "{} is already where Sill installs extensions, and installing clears \
             that folder first. Build it from the folder you are editing.",
            source.display()
        ));
    }

    // Beside the destination rather than inside it, so what is there stays
    // whole until this succeeds. Dot-prefixed because `pins` walks this
    // directory and a half-built extension is not an installed one.
    let building = home.join(format!(".{}.installing", manifest.name));
    let _ = std::fs::remove_dir_all(&building);
    std::fs::create_dir_all(&building)
        .map_err(|err| format!("could not make {}: {err}", building.display()))?;

    std::fs::write(building.join("package.json"), COMMONJS_MARKER)
        .map_err(|err| format!("could not write the module marker: {err}"))?;

    // `environment.assetsPath` is a real directory or it is a lie. Extensions
    // read icons and templates out of it, and it pointed at nothing because
    // installing kept only the bundles.
    copy_tree(&source.join("assets"), &building.join("assets"))
        .map_err(|err| format!("could not copy this extension's assets: {err}"))?;

    let runnable: Vec<&ManifestCommand> = manifest
        .commands
        .iter()
        .filter(|command| why_not_runnable(&command.mode).is_none())
        .collect();

    let mut records = Vec::new();

    for (at, command) in runnable.iter().enumerate() {
        let Some(relative) =
            entrypoint_for(&command.name, |candidate| source.join(candidate).is_file())
        else {
            return Err(format!(
                "{} declares the command \"{}\" and has no source for it under src/.",
                manifest.name, command.name
            ));
        };

        report(Progress::Building {
            command: command
                .title
                .clone()
                .unwrap_or_else(|| command.name.clone()),
            done: at + 1,
            total: runnable.len(),
        });

        bundle(
            esbuild,
            &source.join(relative),
            &building.join(format!("{}.js", command.name)),
            &aliases,
            report,
        )?;

        // The record names where the bundle will be, not where it is being
        // written. Nothing runs out of the build directory.
        records.push(record_for(
            &manifest,
            command,
            &dest.join(format!("{}.js", command.name)),
        ));
    }

    // Written before the swap, so the directory that lands is complete: an
    // extension that is listed is always one whose provenance is recorded.
    crate::store::write_origin_into(&building, origin)?;

    // The replacement. Removing first because a rename onto an existing
    // directory fails on Windows, and the pair is what makes an update leave
    // nothing of the version before it.
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|err| {
            format!(
                "could not clear {} to install over it: {err}",
                dest.display()
            )
        })?;
    }
    std::fs::rename(&building, &dest)
        .map_err(|err| format!("could not put {} in place: {err}", dest.display()))?;

    let index_path = crate::store::index_file(home);
    let merged = reinstalled_index(
        crate::registry::load_index(&index_path),
        &manifest.name,
        records,
    );

    let written = serde_json::to_string_pretty(&merged)
        .map_err(|err| format!("could not write the extension index: {err}"))?;
    std::fs::write(&index_path, format!("{written}\n"))
        .map_err(|err| format!("could not write {}: {err}", index_path.display()))?;

    Ok(Installed {
        title: manifest
            .title
            .clone()
            .unwrap_or_else(|| manifest.name.clone()),
        commands: runnable
            .iter()
            .map(|c| c.title.clone().unwrap_or_else(|| c.name.clone()))
            .collect(),
        refused,
        extension: manifest.name,
    })
}

/// Copies one directory into another, or does nothing if it is not there.
///
/// For `assets/`, which most extensions have and some do not. Absent is the
/// ordinary case rather than a failure.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }

    std::fs::create_dir_all(to)?;

    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());

        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }

    Ok(())
}

/// Run esbuild over one command.
///
/// Its own diagnostics are the useful ones when an extension will not build,
/// so they are carried out rather than replaced with a summary: "Could not
/// resolve ./helpers" names the line to fix, and "the extension failed to
/// build" names nothing.
///
/// Bounded, because it was not. `output()` waits for the child's pipes to
/// close and has no way to stop waiting, so an esbuild that hangs is an
/// install that never ends and a window that says "Installing" until Sill is
/// quit.
#[cfg(windows)]
fn bundle(
    esbuild: &Path,
    entry: &Path,
    outfile: &Path,
    aliases: &[(String, String)],
    report: Report<'_>,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    // No console window for a subprocess of a launcher.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut command = std::process::Command::new(esbuild);
    command
        .args(esbuild_args(entry, outfile, aliases))
        .creation_flags(CREATE_NO_WINDOW);

    let ran = crate::bounded::run(&mut command, BUNDLE_DEADLINE, &mut |line| {
        report(Progress::Bundling {
            said: line.to_string(),
        })
    })?;

    if ran.ok {
        return Ok(());
    }

    let said = ran.said.trim();

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

#[cfg(test)]
mod reads_the_manifest {
    use super::*;

    fn manifest(json: &str) -> Manifest {
        serde_json::from_str(json).expect("manifest parses")
    }

    /// The two refusals say different things, because they are different.
    #[test]
    fn a_mode_sill_cannot_run_says_which_kind_of_problem_it_is() {
        assert_eq!(why_not_runnable("view"), None);
        assert_eq!(why_not_runnable("no-view"), None);

        let bar = why_not_runnable("menu-bar").expect("menu-bar is refused");
        assert!(
            bar.contains("beside the clock"),
            "a refusal has to be actionable, and this one is about where it \
             would go: {bar}"
        );

        let unknown = why_not_runnable("floating-window").expect("an unknown mode is refused");
        assert!(
            unknown.contains("floating-window"),
            "an unknown mode is named, or nobody can look it up: {unknown}"
        );
        assert!(
            !unknown.contains("clock"),
            "and it is not described as a menu bar item"
        );
    }

    /// Refused at install rather than found out at run.
    #[test]
    fn a_menu_bar_command_is_named_before_anything_is_built() {
        let parsed = manifest(
            r#"{
                "name": "demo",
                "commands": [
                    { "name": "search", "mode": "view" },
                    { "name": "bar", "mode": "menu-bar" }
                ]
            }"#,
        );

        let refused = refused_commands(&parsed);

        assert_eq!(refused.len(), 1, "only the one Sill cannot run");
        assert_eq!(refused[0].0, "bar");
        assert!(refused[0].1.starts_with("bar:"), "{}", refused[0].1);
        assert_eq!(
            nothing_left_to_install(&parsed),
            None,
            "one runnable command is still an extension worth installing"
        );
    }

    /// An install that adds nothing is indistinguishable from a failed one.
    #[test]
    fn an_extension_with_nothing_runnable_is_refused_whole() {
        let parsed = manifest(
            r#"{
                "name": "clockface",
                "commands": [
                    { "name": "bar", "mode": "menu-bar" },
                    { "name": "other", "mode": "menu-bar" }
                ]
            }"#,
        );

        let said = nothing_left_to_install(&parsed).expect("there is nothing to install");

        assert!(said.contains("clockface"), "{said}");
        assert!(said.contains("bar"), "it names which commands: {said}");
        assert!(said.contains("other"), "all of them: {said}");
    }

    /// The fields that were not read at all.
    #[test]
    fn a_preference_carries_its_type_and_whether_it_is_required() {
        let parsed = manifest(
            r#"{
                "name": "demo",
                "preferences": [
                    {
                        "name": "token",
                        "type": "password",
                        "title": "API Key",
                        "description": "From your account page",
                        "required": true
                    },
                    {
                        "name": "size",
                        "type": "dropdown",
                        "data": [
                            { "title": "Small", "value": "s" },
                            { "title": "Large", "value": "l" }
                        ]
                    }
                ],
                "commands": [{ "name": "run", "mode": "view" }]
            }"#,
        );

        let declared = declared_preferences(&parsed, &parsed.commands[0]);

        assert_eq!(declared[0].kind.as_deref(), Some("password"));
        assert_eq!(declared[0].title.as_deref(), Some("API Key"));
        assert!(declared[0].required, "required was never read");
        assert_eq!(declared[1].data.len(), 2, "a dropdown's choices");
        assert_eq!(declared[1].data[0].title, "Small");
        assert!(!declared[1].required, "absent is not required");
    }

    /// A command redeclaring one replaces it rather than adding a second row.
    #[test]
    fn a_command_redeclaring_a_preference_replaces_it_in_place() {
        let parsed = manifest(
            r#"{
                "name": "demo",
                "preferences": [
                    { "name": "shared", "type": "textfield" },
                    { "name": "after", "type": "textfield" }
                ],
                "commands": [{
                    "name": "run",
                    "mode": "view",
                    "preferences": [{ "name": "shared", "type": "password" }]
                }]
            }"#,
        );

        let declared = declared_preferences(&parsed, &parsed.commands[0]);
        let names: Vec<&str> = declared.iter().map(|it| it.name.as_str()).collect();

        assert_eq!(
            names,
            ["shared", "after"],
            "one row each, in manifest order"
        );
        assert_eq!(
            declared[0].kind.as_deref(),
            Some("password"),
            "the command's own declaration is the one that stands"
        );
    }

    /// `arguments` was not read, so a command that wanted one got nothing.
    #[test]
    fn a_commands_arguments_are_read() {
        let parsed = manifest(
            r#"{
                "name": "demo",
                "commands": [{
                    "name": "run",
                    "mode": "view",
                    "arguments": [
                        {
                            "name": "query",
                            "type": "text",
                            "placeholder": "Search",
                            "required": true
                        },
                        {
                            "name": "scope",
                            "type": "dropdown",
                            "data": [{ "title": "All", "value": "all" }]
                        }
                    ]
                }]
            }"#,
        );

        let arguments = &parsed.commands[0].arguments;

        assert_eq!(arguments.len(), 2);
        assert_eq!(arguments[0].name, "query");
        assert!(arguments[0].required);
        assert_eq!(arguments[0].placeholder.as_deref(), Some("Search"));
        assert_eq!(arguments[1].kind.as_deref(), Some("dropdown"));
        assert_eq!(arguments[1].data[0].value, "all");
    }

    #[test]
    fn a_manifest_with_none_of_the_new_fields_still_reads() {
        // Every extension published before any of this was read. A field that
        // fails an install is how a store stops working.
        let parsed =
            manifest(r#"{ "name": "demo", "commands": [{ "name": "r", "mode": "view" }] }"#);

        assert!(parsed.commands[0].arguments.is_empty());
        assert!(parsed.dependencies.is_empty());
        assert!(declared_api(&parsed).is_none());
    }
}

#[cfg(test)]
mod api_versions {
    use super::*;

    #[test]
    fn the_ranges_a_manifest_actually_writes_are_understood() {
        assert_eq!(lowest_accepted("^1.104.0"), Some((1, 104, 0)));
        assert_eq!(lowest_accepted("~1.50.2"), Some((1, 50, 2)));
        assert_eq!(lowest_accepted(">=1.50.0 <2.0.0"), Some((1, 50, 0)));
        assert_eq!(lowest_accepted("1.99"), Some((1, 99, 0)));
        assert_eq!(lowest_accepted("2"), Some((2, 0, 0)));
    }

    /// A range this cannot read is no claim rather than a failed install.
    #[test]
    fn a_range_that_cannot_be_read_says_nothing() {
        for unreadable in ["*", "latest", "", "workspace:*", "^x.y.z"] {
            assert_eq!(lowest_accepted(unreadable), None, "{unreadable} was read");
        }

        assert_eq!(api_ahead_of_sill(Some("*"), RAYCAST_API_LEVEL), None);
        assert_eq!(api_ahead_of_sill(None, RAYCAST_API_LEVEL), None);
    }

    #[test]
    fn asking_for_more_than_sill_implements_is_said_out_loud() {
        let said = api_ahead_of_sill(Some("^1.120.0"), "1.104.0").expect("it is ahead");

        assert!(said.contains("1.120.0"), "what it asked for: {said}");
        assert!(said.contains("1.104.0"), "and what it gets: {said}");
    }

    #[test]
    fn asking_for_what_sill_has_or_less_says_nothing() {
        assert_eq!(api_ahead_of_sill(Some("^1.104.0"), "1.104.0"), None);
        assert_eq!(api_ahead_of_sill(Some("^1.50.0"), "1.104.0"), None);
        assert!(
            api_ahead_of_sill(Some("^1.104.1"), "1.104.0").is_some(),
            "a patch above is still above"
        );
    }
}

#[cfg(test)]
mod the_index_after_an_update {
    use super::*;

    fn record(id: &str) -> CommandRecord {
        let (extension, command) = id.split_once(':').expect("id is extension:command");
        let parsed: Manifest = serde_json::from_str(&format!(
            r#"{{ "name": "{extension}", "commands": [{{ "name": "{command}", "mode": "view" }}] }}"#
        ))
        .expect("manifest parses");

        record_for(&parsed, &parsed.commands[0], Path::new("x.js"))
    }

    /// The residue. An id the new manifest no longer declares is a command
    /// that still runs code its author removed.
    #[test]
    fn a_command_the_new_version_dropped_leaves_the_index() {
        let after = reinstalled_index(
            vec![record("demo:kept"), record("demo:removed")],
            "demo",
            vec![record("demo:kept")],
        );

        let ids: Vec<&str> = after.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            ["demo:kept"],
            "demo:removed is still listed, so searching finds a command the \
             author took away"
        );
    }

    /// And nothing else moves.
    #[test]
    fn updating_one_extension_leaves_every_other_one_listed() {
        let after = reinstalled_index(
            vec![
                record("demo:gone"),
                record("other:thing"),
                record("github:issues"),
            ],
            "demo",
            vec![record("demo:new")],
        );

        let ids: Vec<&str> = after.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["demo:new", "github:issues", "other:thing"]);
    }

    /// An extension whose name begins another's must not take it with it.
    #[test]
    fn updating_git_does_not_touch_github() {
        let after = reinstalled_index(
            vec![record("git:log"), record("github:issues")],
            "git",
            vec![record("git:log")],
        );

        let ids: Vec<&str> = after.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["git:log", "github:issues"]);
    }

    #[test]
    fn a_first_install_is_the_same_operation() {
        let after = reinstalled_index(Vec::new(), "demo", vec![record("demo:run")]);
        assert_eq!(after.len(), 1);
    }

    /// Installing from the folder Sill installs into would delete the source.
    #[test]
    fn a_source_inside_the_destination_is_recognised() {
        let dest = Path::new(r"C:\data\extensions\demo");

        assert!(inside(dest, dest), "the destination itself");
        assert!(inside(&dest.join("src"), dest), "and anything under it");
        assert!(!inside(Path::new(r"C:\work\demo"), dest));
        assert!(
            !inside(Path::new(r"C:\data\extensions\demo-two"), dest),
            "a sibling whose name begins with the destination's is not inside it"
        );
    }
}

/// What an install actually leaves on the disk, built with the real esbuild.
///
/// Every other test here is a function over values, which is the right shape
/// for nearly all of this. Residue is the exception: whether a bundle from the
/// version before is still on disk is a question about the disk, and no
/// arrangement of pure functions can answer it.
///
/// The extensions are hand-written and tiny, so this costs one esbuild run per
/// command and nothing else. It needs the esbuild a development build already
/// has, which is what `npm --prefix host ci` installs and what
/// `.github/workflows/verify.yml` runs before any of this.
#[cfg(all(test, windows))]
mod installs_to_disk {
    use super::*;

    /// The esbuild a development checkout has.
    fn esbuild() -> PathBuf {
        if let Some(named) = std::env::var_os("SILL_ESBUILD") {
            return PathBuf::from(named);
        }

        let found = crate::host::dev_root()
            .join("host")
            .join("node_modules")
            .join("@esbuild")
            .join("win32-x64")
            .join("esbuild.exe");

        assert!(
            found.is_file(),
            "these tests build a real extension and there is no esbuild at {}. \
             Run `npm --prefix host ci`, or set SILL_ESBUILD",
            found.display()
        );

        found
    }

    /// Writes an extension whose manifest declares exactly these commands.
    fn write_extension(at: &Path, name: &str, commands: &[&str]) {
        std::fs::create_dir_all(at.join("src")).expect("a source folder");

        let declared: Vec<String> = commands
            .iter()
            .map(|command| format!(r#"{{ "name": "{command}", "mode": "no-view" }}"#))
            .collect();

        std::fs::write(
            at.join("package.json"),
            format!(
                r#"{{ "name": "{name}", "commands": [{}] }}"#,
                declared.join(",")
            ),
        )
        .expect("a manifest");

        for command in commands {
            std::fs::write(
                at.join("src").join(format!("{command}.ts")),
                format!("export default function run() {{ return \"{command}\"; }}\n"),
            )
            .expect("a source file");
        }
    }

    fn ids_in(home: &Path) -> Vec<String> {
        crate::registry::load_index(&crate::store::index_file(home))
            .into_iter()
            .map(|record| record.id)
            .collect()
    }

    /// The whole of `P4-06`'s first half, on the disk.
    ///
    /// Install two commands, then install a manifest that declares one of
    /// them, and the other has to be gone from both places it was written. It
    /// used to survive in both: the bundle because the build wrote into a
    /// directory nobody cleared, and the index entry because merging replaces
    /// what it sees again and keeps what it does not.
    #[test]
    fn updating_leaves_nothing_of_the_version_before_it() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let home = scratch.path().join("extensions");
        let source = scratch.path().join("source");

        // Another extension, to prove an update is about one of them.
        let other = scratch.path().join("other-source");
        write_extension(&other, "other", &["stays"]);
        install_into(
            &esbuild(),
            &home,
            &other,
            &crate::store::Origin::folder(&other, 0),
        )
        .expect("the other extension installs");

        write_extension(&source, "demo", &["kept", "removed"]);
        install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect("the first version installs");

        let installed = home.join("demo");
        assert!(installed.join("kept.js").is_file());
        assert!(installed.join("removed.js").is_file());
        assert!(ids_in(&home).contains(&"demo:removed".to_string()));

        // The new version, which no longer declares `removed`. The source file
        // stays where it is, which is the realistic shape: the author deleted
        // the entry from the manifest.
        write_extension(&source, "demo", &["kept"]);
        let done = install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect("the second version installs");

        assert_eq!(done.commands, ["kept"]);

        assert!(
            installed.join("kept.js").is_file(),
            "the command that is still declared has to still be built"
        );
        assert!(
            !installed.join("removed.js").exists(),
            "the bundle of a command the author removed is still on disk, so \
             the index entry beside it would still run"
        );

        let ids = ids_in(&home);
        assert!(
            !ids.contains(&"demo:removed".to_string()),
            "a withdrawn command is still searchable and still runs: {ids:?}"
        );
        assert!(ids.contains(&"demo:kept".to_string()));
        assert!(
            ids.contains(&"other:stays".to_string()),
            "updating one extension took another out of the index: {ids:?}"
        );
        assert!(
            home.join("other").join("stays.js").is_file(),
            "and off the disk"
        );

        // Nothing is left half-built beside the destination either.
        let leftovers: Vec<String> = std::fs::read_dir(&home)
            .expect("readable")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    }

    /// A build that fails must not take the working version with it.
    #[test]
    fn a_failed_update_leaves_the_installed_version_alone() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let home = scratch.path().join("extensions");
        let source = scratch.path().join("source");

        write_extension(&source, "demo", &["works"]);
        install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect("the good version installs");

        // A second command whose source will not compile.
        write_extension(&source, "demo", &["works", "broken"]);
        std::fs::write(
            source.join("src").join("broken.ts"),
            "import { nothing } from \"./no-such-file\";\nexport default nothing;\n",
        )
        .expect("a source file that cannot build");

        assert!(
            install_into(
                &esbuild(),
                &home,
                &source,
                &crate::store::Origin::folder(&source, 0),
            )
            .is_err(),
            "an unresolvable import is a failed build"
        );

        assert!(
            home.join("demo").join("works.js").is_file(),
            "a failed update deleted the version that was working"
        );
        assert!(ids_in(&home).contains(&"demo:works".to_string()));
    }

    /// `environment.assetsPath` has to point at something.
    #[test]
    fn an_extensions_assets_arrive_with_it_and_are_replaced_by_an_update() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let home = scratch.path().join("extensions");
        let source = scratch.path().join("source");

        write_extension(&source, "demo", &["run"]);
        std::fs::create_dir_all(source.join("assets").join("icons")).expect("an assets folder");
        std::fs::write(source.join("assets").join("icon.png"), b"first").expect("an asset");
        std::fs::write(
            source.join("assets").join("icons").join("deep.svg"),
            b"nested",
        )
        .expect("a nested asset");

        install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect("it installs");

        let assets = home.join("demo").join("assets");
        assert_eq!(
            std::fs::read(assets.join("icon.png")).expect("the asset arrived"),
            b"first"
        );
        assert!(
            assets.join("icons").join("deep.svg").is_file(),
            "a nested asset arrived too"
        );

        // The next version drops one and changes the other.
        std::fs::remove_file(source.join("assets").join("icons").join("deep.svg")).unwrap();
        std::fs::write(source.join("assets").join("icon.png"), b"second").expect("a new asset");

        install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect("it updates");

        assert_eq!(
            std::fs::read(assets.join("icon.png")).expect("still there"),
            b"second"
        );
        assert!(
            !assets.join("icons").join("deep.svg").exists(),
            "an asset the new version dropped is still on disk"
        );
    }

    /// Installing clears its destination, so this would delete the source.
    #[test]
    fn installing_from_the_folder_it_installs_into_is_refused() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let home = scratch.path().join("extensions");
        let source = home.join("demo");

        write_extension(&source, "demo", &["run"]);

        let said = install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect_err("installing over its own source has to be refused");

        assert!(said.contains("installs extensions"), "{said}");
        assert!(
            source.join("package.json").is_file(),
            "the refusal came after the source had already been deleted"
        );
    }

    /// Refused at install, so nothing is built and nothing is listed.
    #[test]
    fn a_menu_bar_command_is_never_built_and_never_indexed() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let home = scratch.path().join("extensions");
        let source = scratch.path().join("source");

        write_extension(&source, "demo", &["search"]);
        std::fs::write(
            source.join("package.json"),
            r#"{ "name": "demo", "commands": [
                { "name": "search", "mode": "no-view" },
                { "name": "bar", "mode": "menu-bar" }
            ] }"#,
        )
        .expect("a manifest with a menu bar command");
        std::fs::write(
            source.join("src").join("bar.ts"),
            "export default () => 1;\n",
        )
        .expect("a source file for it");

        let done = install_into(
            &esbuild(),
            &home,
            &source,
            &crate::store::Origin::folder(&source, 0),
        )
        .expect("the runnable half still installs");

        assert_eq!(done.commands, ["search"]);
        assert_eq!(done.refused.len(), 1, "and it says what it refused");
        assert!(done.refused[0].contains("bar"), "{:?}", done.refused);

        assert!(
            !home.join("demo").join("bar.js").exists(),
            "a command Sill cannot run was built anyway"
        );
        assert_eq!(ids_in(&home), ["demo:search"]);
    }

    /// The one thing a build directory beside the destination must not do.
    #[test]
    fn a_half_built_extension_is_never_reported_as_installed() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let home = scratch.path().join("extensions");

        // What the build directory looks like at its most complete: an origin
        // written, waiting for the rename.
        let building = home.join(".demo.installing");
        crate::store::write_origin_into(
            &building,
            &crate::store::Origin::store("demo", "extensions/demo", "sha", Vec::new(), 0),
        )
        .expect("an origin");

        assert!(
            crate::store::pins(&home).is_empty(),
            "a directory that is still being built was listed as an install"
        );
    }
}
