//! Installing an extension the store found, in two steps on purpose.
//!
//! ## Why two steps
//!
//! Step one fetches the source and reads it. Step two runs npm and builds.
//! Between them the window shows what the code appears to reach and waits for
//! an answer, which is the only point at which "show what it will be able to
//! do before installing" can honestly happen: before the fetch there is
//! nothing to read but a description somebody typed, and after the build it is
//! already here.
//!
//! **Nothing executes before the answer.** Fetching is downloads and reading
//! is reading. npm does not run, the bundler does not run, and no lifecycle
//! script exists to run because npm is invoked with `--ignore-scripts`.
//!
//! ## Why npm has to run at all
//!
//! esbuild bundles what it is pointed at, so an unresolved import is a build
//! failure rather than a warning. `uuid-generator` imports `uuid`, `typeid-js`
//! and `ulidx`; without its dependencies on disk, one of its nine commands
//! builds and the rest do not. This is the same thing
//! `.github/workflows/verify.yml` does before the view gate and for exactly
//! the same reason.
//!
//! Node is already required to run any extension at all, and npm arrives with
//! Node, so this adds no requirement that was not already there.
//!
//! ## Why the source does not stay
//!
//! Installing leaves the built bundles and nothing else. A staged extension is
//! its source plus a `node_modules` that measured **45 MB for `uuid-generator`
//! alone**, and keeping that per installed extension is the kind of resting
//! cost rule 23 exists to refuse. The bundle is self contained apart from the
//! two modules the host supplies, so nothing is lost by deleting the rest.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::{capability, source, Listing, Origin};

/// Where an extension is assembled before it is built.
///
/// Under the data directory rather than the system temp folder, because it
/// holds tens of megabytes between two calls and a cleaner that runs while
/// somebody is deciding would be a confusing failure.
///
/// Takes the data directory rather than a window, so the whole install path can
/// be exercised without one. That is rule 20: fetching an extension, reading
/// it, resolving its dependencies and building it are the interesting parts,
/// and none of them is about a window.
pub fn staging_home(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("staging")
}

/// Whether a name may be used as a directory.
///
/// It arrives from the window as a string and is joined onto a path, so it is
/// checked rather than trusted. Store slugs are lowercase words and hyphens;
/// this allows a little more than that and nothing that navigates.
pub fn safe_name(name: &str) -> Option<&str> {
    let ok = !name.is_empty()
        // No leading dot, which rules out `.` and `..` and one more thing: an
        // install builds into `.<name>.installing` beside its destination, and
        // `pins` skips dot-prefixed directories because of it. A store slug
        // that could begin with a dot would make that skip a lie.
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');

    ok.then_some(name)
}

/// The modules an extension does not have to fetch.
///
/// The host supplies both at runtime and `esbuild_args` marks them external,
/// so listing them among the packages an install pulls in would be wrong twice
/// over: they are not downloaded, and naming React in a list of third party
/// code somebody is being asked to accept is noise.
const SUPPLIED: &[&str] = &["@raycast/api", "react", "react-dom"];

/// The third party packages this install will fetch from npm.
///
/// `dependencies` only. Development dependencies are not installed, which is
/// what `--omit=dev` below is for, and listing packages that never arrive
/// would make the report longer and less true.
pub fn packages_in(manifest: &Value) -> Vec<String> {
    let Some(dependencies) = manifest.get("dependencies").and_then(Value::as_object) else {
        return Vec::new();
    };

    dependencies
        .keys()
        .filter(|name| !SUPPLIED.contains(&name.as_str()))
        .cloned()
        .collect()
}

/// The settings it will ask for that are stored as secrets.
///
/// Read out of the manifest rather than guessed from names. An extension
/// declaring a `password` preference is asking for an API key or a token, and
/// somebody deciding whether to install it should be told that before they are
/// asked for one.
pub fn secrets_in(manifest: &Value) -> Vec<String> {
    let mut found = Vec::new();

    let lists = std::iter::once(manifest.get("preferences")).chain(
        manifest
            .get("commands")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|command| command.get("preferences")),
    );

    for list in lists.flatten() {
        for preference in list.as_array().into_iter().flatten() {
            if preference.get("type").and_then(Value::as_str) != Some("password") {
                continue;
            }

            let named = ["title", "label", "name"]
                .into_iter()
                .find_map(|key| preference.get(key).and_then(Value::as_str))
                .unwrap_or("an unnamed setting");

            if !found.iter().any(|it| it == named) {
                found.push(named.to_string());
            }
        }
    }

    found
}

/// One command, as its own manifest declares it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCommand {
    pub name: String,
    pub title: String,
    pub description: String,
    pub mode: String,
    pub runnable: bool,
}

/// The commands the fetched manifest declares.
///
/// From the manifest rather than from the catalogue, because this is the file
/// that is about to be built and the catalogue is a description of it that can
/// be a commit or two behind.
pub fn commands_in(manifest: &Value) -> Vec<PreparedCommand> {
    manifest
        .get("commands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|command| {
            let name = command.get("name")?.as_str()?.to_string();
            let mode = command
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            Some(PreparedCommand {
                title: command
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(&name)
                    .to_string(),
                description: command
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                runnable: crate::exthost::CommandMode::from_manifest(&mode).is_some(),
                mode,
                name,
            })
        })
        .collect()
}

/// What was fetched and what it appears to do, for the window to show before
/// anybody agrees to it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preparation {
    pub name: String,
    pub title: String,
    pub revision: String,
    pub folder: String,
    pub icon: String,
    pub source_url: String,
    pub files: usize,
    pub bytes: u64,
    pub commands: Vec<PreparedCommand>,
    /// What the code appears to reach. Never a permission list.
    pub capabilities: Vec<capability::Reached>,
    /// The npm packages step two will fetch.
    pub packages: Vec<String>,
    /// Settings it will ask for that hold a credential.
    pub secrets: Vec<String>,
    /// Said when it asks for a newer `@raycast/api` than Sill implements.
    ///
    /// A warning rather than a refusal: the version an extension pins is the
    /// one its author had installed, not a list of what it uses. What it buys
    /// is that "a function is undefined" has an explanation somebody saw
    /// before they installed it.
    pub api_warning: Option<String>,
    /// The commands Sill will refuse to install, one sentence each.
    ///
    /// On the screen that asks, because an extension whose menu bar command is
    /// dropped installs three of its four and looks half broken otherwise.
    pub refused: Vec<String>,
    /// The sentence that says what none of this enforces.
    pub not_enforced: &'static str,
}

/// The largest source file that is read for the capability scan.
///
/// Half a megabyte. A generated or minified file that size tells nobody
/// anything and reading a committed bundle would report every capability there
/// is.
const MAX_SCANNED: u64 = 512 * 1024;

/// Every source file in a staged extension, as text.
fn sources_under(root: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, at: &Path, into: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(at) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                walk(root, &path, into);
                continue;
            }

            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let named = relative.to_string_lossy().replace('\\', "/");

            if !capability::is_source(&named) {
                continue;
            }
            if entry.metadata().map(|it| it.len()).unwrap_or(0) > MAX_SCANNED {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                into.push((named, text));
            }
        }
    }

    let mut found = Vec::new();
    walk(root, root, &mut found);
    // Read order from the filesystem is not an order, and the report names the
    // first few files a capability was seen in.
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Step one: fetch the source and read it. Nothing is executed.
pub async fn prepare(
    data_dir: &Path,
    listing: &Listing,
    token: Option<&str>,
) -> Result<Preparation, String> {
    let name = safe_name(&listing.name)
        .ok_or_else(|| format!("{} is not a name this can install", listing.name))?;

    // The whole staging area, not only this extension's corner of it. Only one
    // install is ever being decided at a time, and anything left here is from a
    // run that did not finish.
    let home = staging_home(data_dir);
    let _ = std::fs::remove_dir_all(&home);

    let staged = home.join(name);
    std::fs::create_dir_all(&staged)
        .map_err(|err| format!("could not make room for {name}: {err}"))?;

    let client = crate::dictation::fetch::client();
    let files = source::list(&client, &listing.folder, &listing.revision, token).await?;
    let fetched =
        source::download(&client, &listing.folder, &listing.revision, &files, &staged).await?;

    let manifest_text = std::fs::read_to_string(staged.join("package.json"))
        .map_err(|_| format!("{name} has no package.json at {}", listing.revision))?;
    let manifest: Value = serde_json::from_str(&manifest_text)
        .map_err(|err| format!("{name} has a package.json that cannot be read: {err}"))?;

    // Written now rather than after the build, so step two knows what it is
    // finishing without the window having to carry it back.
    let capabilities = capability::reached(&sources_under(&staged));

    // Written before the screen is shown, so what gets granted is exactly what
    // was on it. Deriving it again at install time would scan the same source
    // twice and give somebody a permission they were never shown if the two
    // ever disagreed.
    let origin = Origin::store(
        &listing.name,
        &listing.folder,
        &listing.revision,
        capabilities.iter().map(|it| it.id.clone()).collect(),
        crate::state::now_seconds(),
    );
    super::write_origin(&home, name, &origin)?;

    Ok(Preparation {
        title: manifest
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(&listing.title)
            .to_string(),
        name: listing.name.clone(),
        revision: listing.revision.clone(),
        folder: listing.folder.clone(),
        icon: listing.icon.clone(),
        source_url: listing.source_url(),
        files: fetched.files,
        bytes: fetched.bytes,
        commands: commands_in(&manifest),
        capabilities,
        packages: packages_in(&manifest),
        secrets: secrets_in(&manifest),
        api_warning: api_warning_for(&manifest),
        refused: refused_in(&manifest),
        not_enforced: capability::NOT_ENFORCED,
    })
}

/// What the fetched manifest says about `@raycast/api`, judged.
///
/// Read out of the fetched `package.json` rather than the catalogue, for the
/// reason `commands_in` is: this is the file about to be built, and the
/// catalogue is a description of it that can be a commit or two behind.
pub fn api_warning_for(manifest: &Value) -> Option<String> {
    let declared = manifest
        .get("dependencies")
        .and_then(|it| it.get("@raycast/api"))
        .and_then(Value::as_str)?;

    crate::extension_install::api_ahead_of_sill(
        Some(declared),
        crate::extension_install::RAYCAST_API_LEVEL,
    )
}

/// The commands the install will refuse, said before it happens.
pub fn refused_in(manifest: &Value) -> Vec<String> {
    commands_in(manifest)
        .into_iter()
        .filter_map(|command| {
            crate::extension_install::why_not_runnable(&command.mode)
                .map(|because| format!("{}: {because}", command.name))
        })
        .collect()
}

/// Throws away what [`prepare`] staged.
pub fn discard(data_dir: &Path) {
    let _ = std::fs::remove_dir_all(staging_home(data_dir));
}

// ---------------------------------------------------------------------- npm

/// What npm is run with, and why each flag is there.
///
/// - `ci` when there is a lock file, because it installs exactly what the
///   repository pinned; `install` when there is not, because `ci` refuses
///   without one.
/// - **`--ignore-scripts`, which is the security decision.** A package's
///   `postinstall` hook is arbitrary code that runs at install time, before
///   anybody has agreed to anything and before the extension has ever been
///   launched. Turning it off is the one real limit this store places on what
///   an install can do, and it is worth the rare native package that needs a
///   build step and will now fail loudly instead.
/// - `--no-audit --no-fund`, which are two more network round trips and a
///   paragraph of output for a thing nobody is reading.
///
/// **`--omit=dev` is deliberately absent, and it was there once.** The
/// reasoning for it was sound and wrong: development dependencies are supposed
/// to be a linter, a formatter and a type checker, and `@raycast/api` alone is
/// 24 MB and marked external anyway. But an extension's source is free to
/// import whatever its own `package.json` lists, wherever it lists it, and
/// esbuild bundles what it is pointed at, so an import it cannot resolve is a
/// failed build rather than a warning.
///
/// Found by installing the twelve most-installed extensions: **`github` puts
/// `graphql-tag` in `devDependencies` and imports it from generated source**,
/// so it was the one of the twelve that would not build. The saving was
/// temporary disk in a directory that is deleted either way, and the cost was
/// an extension that cannot be installed for a reason nobody could act on.
pub fn npm_args(has_lock: bool) -> Vec<String> {
    [
        if has_lock { "ci" } else { "install" },
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Where npm's entry point is, given a Node.
///
/// Asked of Node rather than looked for on `PATH`. npm on Windows is
/// `npm.cmd`, a batch file, and `CreateProcessW` cannot execute one: reaching
/// it from `PATH` means going through a shell. Its JavaScript entry point sits
/// beside the interpreter in every standard layout, and running that with the
/// Node already found needs no shell and no second interpreter.
#[cfg(windows)]
fn npm_cli(node: &Path) -> Result<PathBuf, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // `node_exe` can answer with the bare name when it found one on `PATH`,
    // which is not a directory anything can be resolved against.
    let output = std::process::Command::new(node)
        .args(["-p", "process.execPath"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("could not ask Node where it is: {err}"))?;

    let real = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());

    let cli = real
        .parent()
        .ok_or_else(|| "Node is in no directory, which cannot be".to_string())?
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");

    if cli.is_file() {
        return Ok(cli);
    }

    Err(format!(
        "npm is not beside the Node at {}. An extension's dependencies cannot be \
         installed without it; reinstalling Node.js from nodejs.org includes npm.",
        real.display()
    ))
}

/// How long npm gets before Sill stops waiting for it.
///
/// The one call here that reaches the network, and the only step of an install
/// that can wait forever on something outside this machine. Measured, npm for
/// `uuid-generator` is 110 packages in two seconds; the extensions with the
/// largest trees are tens of seconds on a cold cache. Five minutes is not a
/// budget, it is the point past which nothing is coming.
///
/// It matters because there was no limit at all. `output()` waits for the
/// child's pipes to close, and a registry that accepts a connection and then
/// says nothing gives an install with no end: the window says "Installing"
/// until somebody quits Sill.
pub const NPM_DEADLINE: std::time::Duration = std::time::Duration::from_secs(300);

/// Installs the staged extension's dependencies.
#[cfg(windows)]
fn npm_install(
    node: &Path,
    staged: &Path,
    report: crate::extension_install::Report<'_>,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let cli = npm_cli(node)?;
    let args = npm_args(staged.join("package-lock.json").is_file());

    let mut command = std::process::Command::new(node);
    command
        .arg(&cli)
        .args(&args)
        .current_dir(staged)
        .creation_flags(CREATE_NO_WINDOW);

    let ran = crate::bounded::run(&mut command, NPM_DEADLINE, &mut |line| {
        report(crate::extension_install::Progress::Dependencies {
            said: line.to_string(),
        })
    })?;

    if ran.ok {
        return Ok(());
    }

    // npm's own message is the useful one, the same bargain
    // `extension_install::bundle` makes with esbuild's: "could not resolve the
    // dependency tree" names the problem and "the install failed" does not.
    let said = ran.said.trim();

    Err(if said.is_empty() {
        "npm refused to install this extension's dependencies and said nothing".to_string()
    } else {
        format!("npm could not install this extension's dependencies:\n{said}")
    })
}

// -------------------------------------------------------------------- steps

/// What an install produced.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Done {
    #[serde(flatten)]
    pub installed: crate::extension_install::Installed,
    pub revision: String,
    /// The capability ids that were agreed to, for the caller to grant.
    ///
    /// Handed back rather than granted here, because granting reaches a
    /// service and this function takes paths. The command layer owns that
    /// seam, which is the same division `finish` already keeps with esbuild.
    pub capabilities: Vec<String>,
}

/// Step two: install the dependencies, build, and record where it came from.
///
/// `esbuild` and `node` are passed in for the same reason `data_dir` is: they
/// are the things here that have to be found rather than known, and the
/// command layer is where that lookup belongs. Node in particular is found by
/// running it, which is a process this must not spawn per install.
#[cfg(windows)]
pub fn finish(data_dir: &Path, esbuild: &Path, node: &Path, name: &str) -> Result<Done, String> {
    finish_reporting(data_dir, esbuild, node, name, &|_| {})
}

/// The same, saying what it is doing while it does it.
///
/// npm and esbuild are the whole of the wait and neither said anything until
/// it finished, so a large extension was one word and a spinner for a minute
/// and a half. Their own output is the content worth showing: npm names the
/// package it is fetching, esbuild names the file it is on.
#[cfg(windows)]
pub fn finish_reporting(
    data_dir: &Path,
    esbuild: &Path,
    node: &Path,
    name: &str,
    report: crate::extension_install::Report<'_>,
) -> Result<Done, String> {
    let name = safe_name(name).ok_or_else(|| format!("{name} is not a name this can install"))?;

    let home = staging_home(data_dir);
    let staged = home.join(name);

    if !staged.join("package.json").is_file() {
        return Err("Nothing is staged to install. Fetch it again.".to_string());
    }

    let origin = super::origin_of(&home, name)
        .ok_or_else(|| "Nothing recorded what was staged. Fetch it again.".to_string())?;

    npm_install(node, &staged, report)?;

    let home = super::extensions_home(data_dir);
    let installed =
        crate::extension_install::install_into_reporting(esbuild, &home, &staged, &origin, report)?;

    /*
     * What the built bundles actually require, added to what the source said.
     *
     * A dependency can need `fs` without the extension's own code mentioning
     * it, and the source scan deliberately does not walk `node_modules`. So
     * after the bundle exists, it is read for the three modules the worker
     * gates at `require`, and anything found is granted too.
     *
     * That is a wider grant than the screen listed, and the screen says so:
     * it names dependencies as code that can do anything the extension does.
     * The alternative was measured, and it is 23 of 124 commands dying on a
     * module nobody could have known to ask about.
     */
    let mut capabilities = origin.capabilities.clone();
    for extra in bundled_requirements(&home.join(&installed.extension)) {
        if !capabilities.contains(&extra) {
            capabilities.push(extra);
        }
    }

    // Rewritten so the record matches what was granted rather than only what
    // was forecast, which is what the settings screen reads back.
    if capabilities != origin.capabilities {
        let mut updated = origin.clone();
        updated.capabilities = capabilities.clone();
        super::write_origin(&home, &installed.extension, &updated)?;
    }

    // Only once the build succeeded. Deleting earlier would take the source
    // away from a build that still needs it, and leaving it costs 45 MB per
    // extension for something nothing reads again.
    discard(data_dir);

    Ok(Done {
        revision: origin.revision.clone(),
        capabilities,
        installed,
    })
}

/// What the built bundles in one directory require at load.
///
/// Reads the `.js` esbuild produced rather than the source, because that is
/// what Node will actually execute.
#[cfg(windows)]
fn bundled_requirements(directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut found: Vec<String> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|it| it != "js") {
            continue;
        }

        // No size cap here, unlike the source scan. A bundle is meant to be
        // large, and skipping the big ones would skip exactly the extensions
        // with the most dependencies, which are the ones this exists for.
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };

        for id in capability::required_by_bundle(&text) {
            if !found.contains(&id) {
                found.push(id);
            }
        }
    }

    found
}

/// Removes an installed extension, its commands, and everything it saved.
///
/// A store you can install from and not remove from is not one anybody should
/// trust. Three parts and every one of them matters: the directory holds the
/// bundles, the index is what the launcher searches, and `LocalStorage` holds
/// whatever the extension wrote there.
///
/// **The third is the one that was missing, and it is not tidiness.** An
/// extension's `LocalStorage` is where it keeps an API token, a search
/// history, a list of what somebody looked at. Removing the extension and
/// leaving that behind keeps a person's data on their machine after they asked
/// for the thing that collected it to go, and reinstalling later hands it
/// straight back to code they have not agreed to since.
///
/// The store is passed in rather than opened here. It is a file the running
/// application already has open, and a second connection to it would be a
/// second answer to what an extension's storage is.
pub fn uninstall(
    data_dir: &Path,
    storage: &crate::exthost::Storage,
    extension: &str,
) -> Result<bool, String> {
    let extension =
        safe_name(extension).ok_or_else(|| format!("{extension} is not an extension name"))?;

    // Before the files, so an extension whose bundles refuse to go does not
    // keep its data as well. The directory going is the recoverable half.
    storage
        .clear(extension)
        .map_err(|err| format!("could not clear what {extension} had saved: {err}"))?;

    // The same argument, for the other two places an extension leaves things:
    // what somebody typed into its settings, which for half the store is an
    // API key, and the folder it was given to write files in.
    let mut preferences = crate::exthost::preferences::load(data_dir);
    if preferences.forget(extension) {
        crate::exthost::preferences::save(data_dir, &preferences)?;
    }
    let _ = std::fs::remove_dir_all(crate::exthost::preferences::support_path(
        data_dir, extension,
    ));

    let home = super::extensions_home(data_dir);
    let directory = home.join(extension);
    let had = directory.is_dir();

    if had {
        std::fs::remove_dir_all(&directory)
            .map_err(|err| format!("could not remove {}: {err}", directory.display()))?;
    }

    let index = super::index_file(&home);
    let listed = crate::registry::load_index(&index);
    let before = listed.len();
    let kept = crate::extension_install::without_extension(listed, extension);

    // Only when something actually left it. Rewriting otherwise means a
    // machine with no extensions at all fails to remove one that is already
    // gone, because there is no directory to write the file into.
    if kept.len() != before {
        let written = serde_json::to_string_pretty(&kept)
            .map_err(|err| format!("could not write the extension index: {err}"))?;
        std::fs::write(&index, format!("{written}\n"))
            .map_err(|err| format!("could not write {}: {err}", index.display()))?;
    }

    Ok(had)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Value {
        serde_json::from_str(json).expect("parses")
    }

    /// This becomes a directory under the data folder.
    #[test]
    fn a_name_that_navigates_is_refused() {
        assert_eq!(safe_name("uuid-generator"), Some("uuid-generator"));
        assert_eq!(safe_name("a.b_c-1"), Some("a.b_c-1"));

        for bad in ["", ".", "..", "../x", "a/b", r"a\b", "C:x", "a b", "a*b"] {
            assert_eq!(safe_name(bad), None, "{bad} was accepted");
        }
    }

    /// The two the host supplies are not downloads and must not be listed as
    /// third party code somebody is agreeing to.
    #[test]
    fn the_packages_reported_are_the_ones_actually_fetched() {
        let parsed = manifest(
            r#"{"dependencies":{
                "@raycast/api":"^1.90.0","react":"^18","uuid":"^11","ulidx":"^2"
            },"devDependencies":{"eslint":"^8"}}"#,
        );

        let mut packages = packages_in(&parsed);
        packages.sort();

        assert_eq!(packages, vec!["ulidx".to_string(), "uuid".to_string()]);
    }

    #[test]
    fn an_extension_with_no_dependencies_reports_none() {
        assert!(packages_in(&manifest("{}")).is_empty());
        assert!(packages_in(&manifest(r#"{"dependencies":{}}"#)).is_empty());
    }

    #[test]
    fn a_credential_it_will_ask_for_is_named_before_it_is_installed() {
        let parsed = manifest(
            r#"{
                "preferences":[
                    {"name":"apiKey","type":"password","title":"API Key"},
                    {"name":"count","type":"textfield","title":"How many"}
                ],
                "commands":[{"name":"c","preferences":[
                    {"name":"token","type":"password","title":"Access Token"}
                ]}]
            }"#,
        );

        assert_eq!(
            secrets_in(&parsed),
            vec!["API Key".to_string(), "Access Token".to_string()],
            "the command's own is found too, and an ordinary field is not"
        );
    }

    #[test]
    fn a_password_preference_with_no_title_is_still_reported() {
        let parsed = manifest(r#"{"preferences":[{"name":"key","type":"password"}]}"#);
        assert_eq!(secrets_in(&parsed), vec!["key".to_string()]);
    }

    #[test]
    fn the_same_credential_declared_twice_is_named_once() {
        let parsed = manifest(
            r#"{
                "preferences":[{"name":"apiKey","type":"password","title":"API Key"}],
                "commands":[{"name":"c","preferences":[
                    {"name":"apiKey","type":"password","title":"API Key"}
                ]}]
            }"#,
        );

        assert_eq!(secrets_in(&parsed), vec!["API Key".to_string()]);
    }

    /// The manifest is what is about to be built, and it can be ahead of the
    /// catalogue's description of it.
    #[test]
    fn commands_come_from_the_manifest_and_say_which_can_run() {
        let parsed = manifest(
            r#"{"commands":[
                {"name":"a","title":"A","mode":"view"},
                {"name":"b","mode":"menu-bar"},
                {"name":"c","mode":"no-view","description":"d"}
            ]}"#,
        );

        let commands = commands_in(&parsed);

        assert_eq!(commands.len(), 3);
        assert!(commands[0].runnable);
        assert!(!commands[1].runnable, "Sill has nowhere to put a menu bar");
        assert_eq!(
            commands[1].title, "b",
            "an untitled command is named by its name"
        );
        assert!(commands[2].runnable);
    }

    /// The flag that stops a package running code at install time.
    #[test]
    fn npm_never_runs_a_packages_install_hook() {
        for has_lock in [true, false] {
            let args = npm_args(has_lock);
            assert!(
                args.contains(&"--ignore-scripts".to_string()),
                "a postinstall hook is arbitrary code running before anybody agreed to it"
            );
        }
    }

    /// The one that cost an extension.
    ///
    /// `github` imports `graphql-tag` from generated source and declares it as
    /// a development dependency. Skipping those saved temporary disk in a
    /// directory that is deleted anyway, and lost an extension nobody could
    /// have diagnosed from the error.
    #[test]
    fn development_dependencies_are_installed_because_extensions_import_them() {
        for has_lock in [true, false] {
            assert!(
                !npm_args(has_lock).contains(&"--omit=dev".to_string()),
                "an extension may import anything its own manifest lists, and esbuild \
                 fails on an import it cannot resolve"
            );
        }
    }

    #[test]
    fn a_lock_file_decides_whether_the_install_is_reproducible() {
        assert_eq!(npm_args(true)[0], "ci");
        assert_eq!(npm_args(false)[0], "install");
    }

    /// The build directory an install renames into place.
    #[test]
    fn a_name_that_could_hide_among_the_build_directories_is_refused() {
        for hidden in [".demo", ".demo.installing"] {
            assert_eq!(
                safe_name(hidden),
                None,
                "{hidden} was accepted, and `pins` skips anything dot-prefixed"
            );
        }
    }

    /// Said before the install rather than found after it.
    #[test]
    fn a_manifest_asking_for_a_newer_api_is_reported() {
        let ahead = manifest(r#"{"dependencies":{"@raycast/api":"^1.400.0"}}"#);
        let said = api_warning_for(&ahead).expect("it is ahead of Sill");
        assert!(said.contains("1.400.0"), "{said}");

        assert_eq!(
            api_warning_for(&manifest(r#"{"dependencies":{"@raycast/api":"^1.50.0"}}"#)),
            None
        );
        assert_eq!(api_warning_for(&manifest("{}")), None);
    }

    #[test]
    fn the_commands_that_will_not_be_installed_are_named_on_the_screen() {
        let parsed = manifest(
            r#"{"commands":[
                {"name":"a","mode":"view"},
                {"name":"b","mode":"menu-bar"}
            ]}"#,
        );

        let refused = refused_in(&parsed);
        assert_eq!(refused.len(), 1);
        assert!(refused[0].starts_with("b:"), "{}", refused[0]);
    }
}

/// Removing an extension, and what it leaves.
///
/// Its own module because these touch the disk and a storage database, where
/// everything above is a function over values.
#[cfg(all(test, windows))]
mod removing {
    use super::*;
    use serde_json::json;

    /// Makes an installed extension without building one.
    ///
    /// The bundles are not the subject here. What is, is that the three places
    /// an extension leaves something all get emptied.
    fn pretend_installed(root: &Path, name: &str) {
        let home = super::super::extensions_home(root);
        std::fs::create_dir_all(home.join(name)).expect("a directory");
        std::fs::write(home.join(name).join("run.js"), "1;").expect("a bundle");

        super::super::write_origin(
            &home,
            name,
            &Origin::store(name, &format!("extensions/{name}"), "sha", Vec::new(), 0),
        )
        .expect("an origin");

        let index = format!(
            r#"[{{"id":"{name}:run","extension":"{name}","extensionTitle":"{name}",
                 "command":"run","title":"Run","mode":"view","entrypoint":"{name}/run.js"}}]"#
        );
        std::fs::write(super::super::index_file(&home), index).expect("an index");
    }

    /// **The security half of `P4-06`.**
    ///
    /// `LocalStorage` is where an extension keeps an API token, a search
    /// history, a list of what somebody looked at. Removing the extension and
    /// leaving that behind keeps a person's data after they asked for the
    /// thing that collected it to go, and hands it back to the next install
    /// under the same name.
    #[test]
    fn removing_an_extension_empties_what_it_had_saved() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let root = scratch.path();
        pretend_installed(root, "demo");

        let storage = crate::exthost::Storage::memory().expect("a store");
        storage
            .set("demo", "token", &json!("sk-live-abcdef"))
            .expect("saved");
        storage
            .set("other", "token", &json!("not this one"))
            .expect("saved");

        uninstall(root, &storage, "demo").expect("it is removed");

        assert_eq!(
            storage.get("demo", "token"),
            serde_json::Value::Null,
            "an extension's saved token outlived the extension"
        );
        assert_eq!(
            storage.get("other", "token"),
            json!("not this one"),
            "and removing one extension emptied another's storage"
        );
    }

    /// The other two places somebody's data ends up.
    #[test]
    fn removing_an_extension_takes_its_settings_and_its_support_folder() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let root = scratch.path();
        pretend_installed(root, "demo");
        pretend_installed(root, "demo-two");

        let declared: Vec<crate::extension_install::Preference> =
            serde_json::from_str(r#"[{ "name": "host", "type": "textfield" }]"#).unwrap();

        let mut held = crate::exthost::preferences::Values::default();
        held.set("demo", "host", json!("example.test"), &declared);
        held.set("demo:run", "host", json!("also this"), &declared);
        held.set("demo-two", "host", json!("kept"), &declared);
        crate::exthost::preferences::save(root, &held).expect("saved");

        let support = crate::exthost::preferences::support_path(root, "demo");
        std::fs::create_dir_all(&support).expect("a support folder");
        std::fs::write(support.join("cache.db"), b"data").expect("something in it");

        let storage = crate::exthost::Storage::memory().expect("a store");
        uninstall(root, &storage, "demo").expect("it is removed");

        let after = crate::exthost::preferences::load(root);
        assert!(after.in_scope("demo").is_none(), "its settings stayed");
        assert!(
            after.in_scope("demo:run").is_none(),
            "and so did its command's"
        );
        assert!(
            after.in_scope("demo-two").is_some(),
            "an extension whose name begins with the removed one's went too"
        );
        assert!(!support.exists(), "the folder it wrote files in stayed");
    }

    /// Removing something already gone is the end state somebody asked for.
    #[test]
    fn removing_what_is_not_installed_is_not_a_failure() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let storage = crate::exthost::Storage::memory().expect("a store");

        assert_eq!(uninstall(scratch.path(), &storage, "absent"), Ok(false));
    }
}
