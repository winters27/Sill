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
        && name != "."
        && name != ".."
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
    let fetched = source::download(&client, &listing.folder, &listing.revision, &files, &staged).await?;

    let manifest_text = std::fs::read_to_string(staged.join("package.json"))
        .map_err(|_| format!("{name} has no package.json at {}", listing.revision))?;
    let manifest: Value = serde_json::from_str(&manifest_text)
        .map_err(|err| format!("{name} has a package.json that cannot be read: {err}"))?;

    // Written now rather than after the build, so step two knows what it is
    // finishing without the window having to carry it back.
    let origin = Origin::store(
        &listing.name,
        &listing.folder,
        &listing.revision,
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
        capabilities: capability::reached(&sources_under(&staged)),
        packages: packages_in(&manifest),
        secrets: secrets_in(&manifest),
        not_enforced: capability::NOT_ENFORCED,
    })
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
/// - `--omit=dev`, because the development dependencies are a linter, a
///   formatter and a type checker. `@raycast/api` alone is 24 MB and is marked
///   external anyway.
/// - `--no-audit --no-fund`, which are two more network round trips and a
///   paragraph of output for a thing nobody is reading.
pub fn npm_args(has_lock: bool) -> Vec<String> {
    [
        if has_lock { "ci" } else { "install" },
        "--omit=dev",
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

/// Installs the staged extension's dependencies.
#[cfg(windows)]
fn npm_install(node: &Path, staged: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let cli = npm_cli(node)?;
    let args = npm_args(staged.join("package-lock.json").is_file());

    let output = std::process::Command::new(node)
        .arg(&cli)
        .args(&args)
        .current_dir(staged)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("could not run npm: {err}"))?;

    if output.status.success() {
        return Ok(());
    }

    // npm's own message is the useful one, the same bargain
    // `extension_install::bundle` makes with esbuild's: "could not resolve the
    // dependency tree" names the problem and "the install failed" does not.
    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim();

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
}

/// Step two: install the dependencies, build, and record where it came from.
///
/// `esbuild` is passed in for the same reason `data_dir` is: it is the one
/// thing here that has to be found rather than known, and the command layer is
/// where that lookup belongs.
#[cfg(windows)]
pub fn finish(data_dir: &Path, esbuild: &Path, name: &str) -> Result<Done, String> {
    let name = safe_name(name).ok_or_else(|| format!("{name} is not a name this can install"))?;

    let home = staging_home(data_dir);
    let staged = home.join(name);

    if !staged.join("package.json").is_file() {
        return Err("Nothing is staged to install. Fetch it again.".to_string());
    }

    let origin = super::origin_of(&home, name)
        .ok_or_else(|| "Nothing recorded what was staged. Fetch it again.".to_string())?;

    let node = crate::host::node_exe().ok_or(crate::host::NO_NODE)?;
    npm_install(&node, &staged)?;

    let installed = crate::extension_install::install_into(
        esbuild,
        &super::extensions_home(data_dir),
        &staged,
        &origin,
    )?;

    // Only once the build succeeded. Deleting earlier would take the source
    // away from a build that still needs it, and leaving it costs 45 MB per
    // extension for something nothing reads again.
    discard(data_dir);

    Ok(Done {
        revision: origin.revision.clone(),
        installed,
    })
}

/// Removes an installed extension and its commands.
///
/// A store you can install from and not remove from is not one anybody should
/// trust. Both halves matter: the directory holds the bundles and the index is
/// what the launcher searches, and leaving either behind leaves an extension
/// that is half gone.
pub fn uninstall(data_dir: &Path, extension: &str) -> Result<bool, String> {
    let extension =
        safe_name(extension).ok_or_else(|| format!("{extension} is not an extension name"))?;

    let home = super::extensions_home(data_dir);
    let directory = home.join(extension);
    let had = directory.is_dir();

    if had {
        std::fs::remove_dir_all(&directory)
            .map_err(|err| format!("could not remove {}: {err}", directory.display()))?;
    }

    let index = super::index_file(&home);
    let kept = crate::extension_install::without_extension(
        crate::registry::load_index(&index),
        extension,
    );

    let written = serde_json::to_string_pretty(&kept)
        .map_err(|err| format!("could not write the extension index: {err}"))?;
    std::fs::write(&index, format!("{written}\n"))
        .map_err(|err| format!("could not write {}: {err}", index.display()))?;

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
        assert_eq!(commands[1].title, "b", "an untitled command is named by its name");
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
            assert!(args.contains(&"--omit=dev".to_string()));
        }
    }

    #[test]
    fn a_lock_file_decides_whether_the_install_is_reproducible() {
        assert_eq!(npm_args(true)[0], "ci");
        assert_eq!(npm_args(false)[0], "install");
    }
}
