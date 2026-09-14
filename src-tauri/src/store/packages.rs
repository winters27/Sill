//! Putting an extension's dependencies where esbuild can resolve them,
//! without a package manager.
//!
//! ## Why this is not npm's job
//!
//! npm is run for one reason: esbuild bundles what it is pointed at, so an
//! import it cannot resolve is a build failure rather than a warning. The tree
//! npm writes is read once, by esbuild, and deleted minutes later. Nothing here
//! runs a lifecycle script, links a binary, reads a peer dependency or asks npm
//! a question. See [`super::install`].
//!
//! Measured across the ten extensions the view gate installs: npm writes
//! **2,449 packages and 991.7 MiB**, and esbuild reaches **151 packages and
//! 37.7 MiB** of it. For `hacker-news` specifically, 238 installed and 17 used.
//! The other 96% is fetched, written and deleted without being opened.
//!
//! ## The lockfile is already the answer
//!
//! A `package-lock.json` at version 2 or 3 is not a description of a tree, it
//! **is** the tree: every key is the literal path the package belongs at,
//! nesting included, and every entry carries the tarball URL and its integrity
//! hash. So there is no resolver here, no semver, and no hoisting. There is a
//! download, a hash check, and an extract to the path the lock names.
//!
//! Proven before this was written: extracting the five entries `uuid-generator`
//! actually needs, to the paths the lock gives, produced bundles **byte for
//! byte identical** to npm's, from a tree 0.52% the size.
//!
//! ## Why it fetches lazily
//!
//! Walking the lock's whole closure up front would be correct and slower than
//! npm. It also drags in a problem that does not otherwise exist: a lockfile
//! lists packages npm deliberately never installs, gated by `os` and `cpu`.
//! `uuid-generator`'s lock names 26 `@esbuild/*` platform binaries, of which 3
//! are for Windows, and fetching the other 25 is **100 MiB**, more than npm's
//! entire installed tree for that extension. None of them is marked `dev`, so
//! the obvious filter misses them.
//!
//! Fetching only what esbuild asks for sidesteps that whole class of problem
//! rather than reimplementing npm's platform rules and keeping them correct:
//! **esbuild never imports a platform binary, so it is never asked for.**
//!
//! ## Flattening is the trap
//!
//! A flat directory of packages is the obvious shortcut and it **fails
//! silently**. `uuid-generator` built from one with no error and no warning,
//! and produced a different bundle: `typeid-js` declares `uuid: ^10.0.0` and
//! quietly received 11.1.0, because a flat tree has nowhere to put the second
//! copy. The lock's own key is the placement, and it is used verbatim.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// One package, as the lockfile places it.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Where it belongs, relative to the extension's root, exactly as the
    /// lockfile keys it: `node_modules/typeid-js/node_modules/uuid`.
    pub path: String,
    /// Its tarball.
    pub resolved: String,
    /// `<algorithm>-<base64>`, as the registry published it.
    pub integrity: String,
}

/// What a lockfile says, reduced to what installing needs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lock {
    by_path: BTreeMap<String, Placed>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLock {
    #[serde(default)]
    lockfile_version: u32,
    #[serde(default)]
    packages: BTreeMap<String, RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    #[serde(default)]
    resolved: Option<String>,
    #[serde(default)]
    integrity: Option<String>,
}

/// The lockfile versions whose keys are paths.
///
/// Version 1 keyed by name with nesting expressed as a `dependencies` tree, so
/// none of this applies to it. Reading one is not worth the code: it has not
/// been npm's default since npm 7, and an extension carrying one falls back to
/// npm, which is what this replaces rather than removes.
const KEYED_BY_PATH: u32 = 2;

impl Lock {
    /// Reads a `package-lock.json`, or answers `None` when it is not one this
    /// can use.
    ///
    /// `None` is a real answer rather than a failure: the caller installs with
    /// npm instead. An entry with no `resolved` or no `integrity` is dropped
    /// rather than guessed at, which means a lockfile that is mostly those
    /// resolves almost nothing and the build asks for npm. That is measured,
    /// not hypothetical: `pokedex` has 179 of 226 entries with neither.
    pub fn read(text: &str) -> Option<Self> {
        let raw: RawLock = serde_json::from_str(text).ok()?;

        if raw.lockfile_version < KEYED_BY_PATH {
            return None;
        }

        let by_path = raw
            .packages
            .into_iter()
            .filter(|(path, _)| !path.is_empty())
            .filter_map(|(path, entry)| {
                let resolved = entry.resolved.filter(|it| !it.is_empty())?;
                let integrity = entry.integrity.filter(|it| !it.is_empty())?;
                Some((
                    path.clone(),
                    Placed {
                        path,
                        resolved,
                        integrity,
                    },
                ))
            })
            .collect();

        Some(Self { by_path })
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    /// Where a bare import resolves to, by the rule Node actually uses.
    ///
    /// **Walking up is the whole of it.** From the directory holding the file
    /// that did the importing, try `<dir>/node_modules/<name>`, then the same
    /// in its parent, and so on to the root. The first one the lockfile places
    /// is the answer, which is how `typeid-js` gets `uuid` 10 while the
    /// extension beside it gets 11.
    ///
    /// `importer` is relative to the extension's root, which is what the
    /// metafile and esbuild's own messages both speak.
    pub fn resolve(&self, importer: &str, specifier: &str) -> Option<&Placed> {
        let name = package_of(specifier)?;
        let mut at = PathBuf::from(importer);

        // The importer is a file; its directory is where the walk starts.
        at.pop();

        loop {
            // Never look inside a `node_modules` for another one at the same
            // level, which is not a place Node looks either.
            let candidate = if at.as_os_str().is_empty() {
                format!("node_modules/{name}")
            } else {
                format!("{}/node_modules/{name}", slashed(&at))
            };

            if let Some(found) = self.by_path.get(&candidate) {
                return Some(found);
            }

            if !at.pop() {
                return None;
            }
        }
    }

    /// Every placement whose directory is this one or inside it.
    ///
    /// A package's own nested dependencies live under its path, so fetching one
    /// and then asking for these is how a round gathers what it just learned it
    /// will need next.
    pub fn nested_in(&self, path: &str) -> Vec<&Placed> {
        let inside = format!("{path}/node_modules/");
        self.by_path
            .values()
            .filter(|placed| placed.path.starts_with(&inside))
            .collect()
    }

    #[cfg(test)]
    fn with(entries: &[(&str, &str)]) -> Self {
        Self {
            by_path: entries
                .iter()
                .map(|(path, version)| {
                    (
                        path.to_string(),
                        Placed {
                            path: path.to_string(),
                            resolved: format!("https://registry.npmjs.org/x/-/x-{version}.tgz"),
                            integrity: "sha512-deadbeef".to_string(),
                        },
                    )
                })
                .collect(),
        }
    }
}

/// The package a specifier names, without its subpath.
///
/// `uuid/dist/esm/index.js` is the `uuid` package, and `@raycast/utils/x` is
/// `@raycast/utils`: a scope is part of the name rather than a directory above
/// it, which is the one case a naive split gets wrong.
///
/// Answers `None` for anything that is not a bare specifier. A relative import
/// is the extension's own file and a Node builtin is not on disk, and neither
/// is something to go and fetch.
pub fn package_of(specifier: &str) -> Option<&str> {
    if specifier.starts_with('.') || specifier.starts_with('/') {
        return None;
    }

    let specifier = specifier.strip_prefix("node:").map_or(specifier, |_| "");
    if specifier.is_empty() {
        return None;
    }

    let mut parts = specifier.split('/');
    let first = parts.next()?;

    if first.starts_with('@') {
        let second = parts.next()?;
        // `@scope` alone is not a package.
        if second.is_empty() {
            return None;
        }
        return Some(&specifier[..first.len() + 1 + second.len()]);
    }

    Some(first)
}

/// A path in the form a lockfile key takes, whatever the platform used.
fn slashed(path: &Path) -> String {
    path.components()
        .filter_map(|part| match part {
            Component::Normal(bit) => Some(bit.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

// ------------------------------------------------------- what esbuild wants

/// One import esbuild could not satisfy from what is on disk.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wanted {
    /// The file that did the importing, relative to the extension's root.
    pub importer: String,
    /// What it asked for, as written.
    pub specifier: String,
}

/// The imports esbuild refused, read out of what it said.
///
/// Its message is two lines that belong together:
///
/// ```text
/// X [ERROR] Could not resolve "uuid"
///
///     node_modules/typeid-js/dist/index.js:2:18:
/// ```
///
/// Both halves are needed and neither is enough alone: the name says what to
/// fetch and the importer says **which** copy of it, because a lockfile places
/// the same package at more than one path on purpose.
///
/// Parsed rather than asked for, because esbuild has no machine-readable form
/// of a failure: a build that fails writes no metafile at all.
pub fn refused(said: &str) -> Vec<Wanted> {
    let mut found = Vec::new();
    let mut waiting: Option<String> = None;

    for line in said.lines() {
        let line = line.trim();

        if let Some(name) = between_quotes(line, "Could not resolve ") {
            waiting = Some(name.to_string());
            continue;
        }

        let Some(specifier) = waiting.clone() else {
            continue;
        };

        if let Some(importer) = importer_in(line) {
            found.push(Wanted {
                importer,
                specifier,
            });
            waiting = None;
        }
    }

    found.sort();
    found.dedup();
    found
}

/// The text inside the first pair of quotes after a marker.
fn between_quotes<'a>(line: &'a str, after: &str) -> Option<&'a str> {
    let rest = line.split_once(after)?.1;
    let rest = rest.strip_prefix('"')?;
    rest.split_once('"').map(|(inside, _)| inside)
}

/// The file a `path:line:column:` line names.
///
/// esbuild writes the position after the path, and a Windows path carries a
/// colon of its own, so the split has to come from the right and has to be
/// two of them rather than one.
fn importer_in(line: &str) -> Option<String> {
    let line = line.strip_suffix(':')?;
    let (rest, column) = line.rsplit_once(':')?;
    let (path, row) = rest.rsplit_once(':')?;

    if path.is_empty()
        || row.is_empty()
        || column.is_empty()
        || !row.bytes().all(|b| b.is_ascii_digit())
        || !column.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    Some(path.replace('\\', "/"))
}

// ------------------------------------------------------------- the tarball

/// How large a single package may be before this refuses it.
///
/// A guard on somebody else's server rather than a budget. The largest package
/// in the ten extensions the gate installs is under 10 MB; `typescript`, which
/// is among the largest on the registry, is about 23 MB. Fifty is far past
/// anything an extension has a reason to pull and still small enough that a
/// redirect to something enormous is stopped rather than written to disk.
pub const LARGEST_PACKAGE: u64 = 50 * 1024 * 1024;

/// Whether a tarball is the one the lockfile named.
///
/// The integrity string is `<algorithm>-<base64 digest>`. Only SHA-512 and
/// SHA-256 are accepted: SHA-1 appears in older lockfiles and is not a hash to
/// accept code on, and an algorithm this does not know is refused rather than
/// waved through, which is the difference between a check and a decoration.
pub fn matches_integrity(integrity: &str, bytes: &[u8]) -> Result<(), String> {
    use base64::Engine as _;
    use sha2::Digest as _;

    let (algorithm, expected) = integrity
        .split_once('-')
        .ok_or_else(|| format!("{integrity} is not an integrity this understands"))?;

    let digest = match algorithm {
        "sha512" => sha2::Sha512::digest(bytes).to_vec(),
        "sha256" => sha2::Sha256::digest(bytes).to_vec(),
        other => {
            return Err(format!(
                "the lockfile vouches for this package with {other}, which Sill does not accept"
            ))
        }
    };

    let want = base64::engine::general_purpose::STANDARD
        .decode(expected)
        .map_err(|_| format!("{integrity} does not carry a digest this can read"))?;

    if digest == want {
        return Ok(());
    }

    Err("the package that arrived is not the one the lockfile vouches for".to_string())
}

/// Unpacks an npm tarball into `into`, dropping its leading directory.
///
/// Every npm tarball wraps its contents in a single `package/` directory, so
/// the first component of every path is stripped. Anything that then tries to
/// leave `into` is refused rather than skipped: a tarball that reaches outside
/// the directory it was given is not a package with a mistake in it.
///
/// Only ordinary files and directories are written. A link inside a package is
/// a thing esbuild has no use for and a symlink is another way out of the
/// destination, so both are passed over.
pub fn unpack(bytes: &[u8], into: &Path) -> Result<usize, String> {
    use std::io::Read as _;

    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let root = into
        .canonicalize()
        .unwrap_or_else(|_| into.to_path_buf());

    let mut written = 0;

    for entry in archive
        .entries()
        .map_err(|err| format!("the package could not be opened: {err}"))?
    {
        let mut entry = entry.map_err(|err| format!("the package is damaged: {err}"))?;

        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            continue;
        }

        let path = entry
            .path()
            .map_err(|err| format!("the package names a file this cannot read: {err}"))?
            .into_owned();

        let Some(inside) = without_first(&path) else {
            continue;
        };

        let at = root.join(&inside);
        if !at.starts_with(&root) {
            return Err(format!(
                "the package tried to write to {}, which is outside where it was asked to go",
                path.display()
            ));
        }

        if kind.is_dir() {
            std::fs::create_dir_all(&at)
                .map_err(|err| format!("could not make {}: {err}", at.display()))?;
            continue;
        }

        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not make {}: {err}", parent.display()))?;
        }

        let mut holding = Vec::new();
        entry
            .read_to_end(&mut holding)
            .map_err(|err| format!("could not read {} out of the package: {err}", path.display()))?;

        std::fs::write(&at, &holding)
            .map_err(|err| format!("could not write {}: {err}", at.display()))?;
        written += 1;
    }

    Ok(written)
}

/// A path with its first component removed, or `None` when nothing is left.
///
/// Components that navigate are dropped rather than kept, so `..` never
/// survives to be joined onto the destination. The check in [`unpack`] is the
/// guard that matters; this is what stops it having to fire for an ordinary
/// tarball that merely contains `./`.
fn without_first(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components().filter_map(|part| match part {
        Component::Normal(bit) => Some(bit),
        _ => None,
    });

    parts.next()?;

    let rest: PathBuf = parts.collect();
    (!rest.as_os_str().is_empty()).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------- the lock

    #[test]
    fn a_lockfile_keyed_by_path_is_read() {
        let lock = Lock::read(
            r#"{"lockfileVersion":3,"packages":{
                "": {"name":"demo"},
                "node_modules/uuid": {
                    "version":"11.1.0",
                    "resolved":"https://registry.npmjs.org/uuid/-/uuid-11.1.0.tgz",
                    "integrity":"sha512-aaa"
                }
            }}"#,
        )
        .expect("a lock");

        assert_eq!(lock.len(), 1);
        assert_eq!(
            lock.resolve("src/run.ts", "uuid").map(|it| it.path.as_str()),
            Some("node_modules/uuid")
        );
    }

    /// Version 1 keys by name rather than by path, so none of this applies and
    /// the caller falls back to npm.
    #[test]
    fn an_older_lockfile_is_not_one_this_can_use() {
        assert!(Lock::read(r#"{"lockfileVersion":1,"dependencies":{}}"#).is_none());
        assert!(Lock::read("not json").is_none());
    }

    /// Measured: `pokedex` has 179 of 226 entries with neither field. Dropping
    /// them is what makes the caller ask npm instead of installing a tree with
    /// holes in it.
    #[test]
    fn an_entry_that_vouches_for_nothing_is_dropped_rather_than_guessed_at() {
        let lock = Lock::read(
            r#"{"lockfileVersion":3,"packages":{
                "node_modules/a": {"version":"1.0.0"},
                "node_modules/b": {"version":"1.0.0","resolved":"https://x/b.tgz"},
                "node_modules/c": {"version":"1.0.0","integrity":"sha512-x"},
                "node_modules/d": {"version":"1.0.0","resolved":"https://x/d.tgz","integrity":"sha512-x"}
            }}"#,
        )
        .expect("a lock");

        assert_eq!(lock.len(), 1);
        assert!(lock.resolve("src/run.ts", "d").is_some());
        assert!(lock.resolve("src/run.ts", "a").is_none());
    }

    // ------------------------------------------------------- the resolution

    /// **The case flattening gets wrong.** `typeid-js` declares `uuid ^10` and
    /// the extension has 11; a flat tree gives both the same copy and the
    /// bundle changes without an error being raised anywhere.
    #[test]
    fn a_nested_copy_wins_for_the_package_that_nests_it() {
        let lock = Lock::with(&[
            ("node_modules/uuid", "11.1.0"),
            ("node_modules/typeid-js", "1.0.0"),
            ("node_modules/typeid-js/node_modules/uuid", "10.0.0"),
        ]);

        assert_eq!(
            lock.resolve("node_modules/typeid-js/dist/index.js", "uuid")
                .map(|it| it.path.as_str()),
            Some("node_modules/typeid-js/node_modules/uuid"),
        );

        assert_eq!(
            lock.resolve("src/run.ts", "uuid").map(|it| it.path.as_str()),
            Some("node_modules/uuid"),
        );
    }

    /// Walking up does not stop at the first `node_modules` it passes.
    #[test]
    fn a_package_without_its_own_copy_reaches_the_one_above() {
        let lock = Lock::with(&[("node_modules/uuid", "11.1.0"), ("node_modules/ulidx", "2.0.0")]);

        assert_eq!(
            lock.resolve("node_modules/ulidx/dist/index.js", "uuid")
                .map(|it| it.path.as_str()),
            Some("node_modules/uuid"),
        );
    }

    #[test]
    fn nothing_placed_anywhere_up_the_walk_is_not_resolved() {
        let lock = Lock::with(&[("node_modules/uuid", "11.1.0")]);
        assert!(lock.resolve("src/run.ts", "left-pad").is_none());
    }

    #[test]
    fn what_a_package_nests_is_found_by_its_path() {
        let lock = Lock::with(&[
            ("node_modules/typeid-js", "1.0.0"),
            ("node_modules/typeid-js/node_modules/uuid", "10.0.0"),
            ("node_modules/uuid", "11.1.0"),
        ]);

        let nested = lock.nested_in("node_modules/typeid-js");
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].path, "node_modules/typeid-js/node_modules/uuid");
    }

    // ------------------------------------------------------------ the names

    #[test]
    fn a_scope_is_part_of_the_name_rather_than_a_directory_above_it() {
        assert_eq!(package_of("@raycast/utils"), Some("@raycast/utils"));
        assert_eq!(package_of("@raycast/utils/dist/x.js"), Some("@raycast/utils"));
        assert_eq!(package_of("uuid"), Some("uuid"));
        assert_eq!(package_of("uuid/dist/esm/index.js"), Some("uuid"));
    }

    /// Neither the extension's own files nor a Node builtin is something to
    /// fetch.
    #[test]
    fn what_is_not_a_package_is_not_named_as_one() {
        for not in ["./helper", "../shared/x", "/abs/path", "node:fs", ""] {
            assert_eq!(package_of(not), None, "{not}");
        }
    }

    // --------------------------------------------------------- what esbuild

    #[test]
    fn an_import_esbuild_refused_names_the_package_and_who_asked() {
        let said = "\
X [ERROR] Could not resolve \"uuid\"

    node_modules/typeid-js/dist/index.js:2:18:
      2 | import { v4 } from \"uuid\"
        |                     ~~~~~~

1 error";

        assert_eq!(
            refused(said),
            vec![Wanted {
                importer: "node_modules/typeid-js/dist/index.js".to_string(),
                specifier: "uuid".to_string(),
            }]
        );
    }

    /// Which copy is wanted depends on who asked, so two refusals of the same
    /// name are two different answers rather than one.
    #[test]
    fn the_same_name_refused_twice_keeps_both_askers() {
        let said = "\
X [ERROR] Could not resolve \"uuid\"

    src/run.tsx:1:18:

X [ERROR] Could not resolve \"uuid\"

    node_modules/typeid-js/dist/index.js:2:18:
";

        let found = refused(said);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|it| it.specifier == "uuid"));
    }

    #[test]
    fn a_build_that_said_nothing_about_resolving_wants_nothing() {
        assert!(refused("").is_empty());
        assert!(refused("X [ERROR] Expected \";\" but found \"}\"\n\n  src/run.ts:4:2:\n").is_empty());
    }

    // ---------------------------------------------------------- the tarball

    /// Builds a gzipped tar the way the registry serves one, wrapping
    /// everything in the `package/` directory npm tarballs always carry.
    fn tarball(files: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write as _;

        let mut builder = tar::Builder::new(Vec::new());
        for (name, body) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, name, *body)
                .expect("appended");
        }
        let tar = builder.into_inner().expect("a tar");

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).expect("gzipped");
        gz.finish().expect("a gzip")
    }

    #[test]
    fn a_package_lands_without_the_directory_it_was_wrapped_in() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let bytes = tarball(&[
            ("package/package.json", br#"{"name":"uuid"}"# as &[u8]),
            ("package/dist/index.js", b"export const v4 = () => 1;"),
        ]);

        assert_eq!(unpack(&bytes, scratch.path()).expect("unpacked"), 2);

        assert!(scratch.path().join("package.json").is_file());
        assert!(scratch.path().join("dist/index.js").is_file());
        assert!(
            !scratch.path().join("package").exists(),
            "the wrapper directory is stripped rather than kept"
        );
    }

    /// A tar header written by hand, because the `tar` crate refuses to build
    /// one whose path contains `..`. A hostile archive is written by something
    /// that has no such scruples, so the guard has to be tested against one.
    fn forged(name: &str, body: &[u8]) -> Vec<u8> {
        use std::io::Write as _;

        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        // mode, uid, gid
        header[100..107].copy_from_slice(b"0000644");
        header[108..115].copy_from_slice(b"0000000");
        header[116..123].copy_from_slice(b"0000000");
        // size and mtime, octal, NUL terminated
        let size = format!("{:011o}\0", body.len());
        header[124..136].copy_from_slice(size.as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        // an ordinary file
        header[156] = b'0';
        // the checksum is computed with its own field read as spaces
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        let written = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(written.as_bytes());

        let mut tar = header.to_vec();
        tar.extend_from_slice(body);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
        // two empty blocks end an archive
        tar.extend_from_slice(&[0u8; 1024]);

        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).expect("gzipped");
        gz.finish().expect("a gzip")
    }

    /// **A tarball that reaches outside where it was told to go writes
    /// nothing there.** npm makes this check too; an extractor that forgets it
    /// is how an archive lands in somebody's home directory.
    #[test]
    fn a_package_that_tries_to_escape_writes_nothing_above_the_destination() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        // Nested deeply enough that an escape lands inside this test's own
        // temporary directory rather than in the shared one, which a previous
        // run could have left something in.
        let into = scratch.path().join("a/b/into");
        std::fs::create_dir_all(&into).expect("a directory");

        let bytes = forged("package/../../escaped.js", b"nope");
        let outcome = unpack(&bytes, &into);

        assert!(
            !scratch.path().join("a/escaped.js").exists(),
            "nothing may be written above the destination"
        );
        assert!(
            !scratch.path().join("a/b/escaped.js").exists(),
            "nor one level above it"
        );

        // Refusing outright and dropping the navigation are both safe. What is
        // not safe is a file above `into`, which the assertions above hold.
        if let Ok(written) = outcome {
            assert!(written <= 1);
        }
    }

    /// The same forging, with an absolute path rather than a climbing one.
    #[test]
    fn a_package_naming_an_absolute_path_writes_nothing_there() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        let into = scratch.path().join("into");
        std::fs::create_dir_all(&into).expect("a directory");

        let bytes = forged("/tmp/sill-escaped.js", b"nope");
        let _ = unpack(&bytes, &into);

        assert!(!Path::new("/tmp/sill-escaped.js").exists());
    }

    /// A link is another way out of the destination and esbuild has no use for
    /// one, so neither kind is written.
    #[test]
    fn links_inside_a_package_are_passed_over() {
        let scratch = tempfile::tempdir().expect("a temp directory");

        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_entry_type(tar::EntryType::Symlink);
        header
            .set_link_name("../../../../etc/passwd")
            .expect("a link name");
        header.set_cksum();
        builder
            .append_data(&mut header, "package/sneaky", std::io::empty())
            .expect("appended");

        let mut ordinary = tar::Header::new_gnu();
        ordinary.set_size(2);
        ordinary.set_mode(0o644);
        ordinary.set_cksum();
        builder
            .append_data(&mut ordinary, "package/real.js", b"ok" as &[u8])
            .expect("appended");

        let tar = builder.into_inner().expect("a tar");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).expect("gzipped");
        let bytes = gz.finish().expect("a gzip");

        assert_eq!(unpack(&bytes, scratch.path()).expect("unpacked"), 1);
        assert!(scratch.path().join("real.js").is_file());
        assert!(!scratch.path().join("sneaky").exists());
    }

    #[test]
    fn something_that_is_not_a_package_is_an_error_rather_than_an_empty_directory() {
        let scratch = tempfile::tempdir().expect("a temp directory");
        assert!(unpack(b"not gzip at all", scratch.path()).is_err());
    }

    // -------------------------------------------------------- the integrity

    #[test]
    fn a_package_the_lockfile_vouches_for_is_accepted() {
        use base64::Engine as _;
        use sha2::Digest as _;

        let bytes = b"the bytes that were published";
        let digest = sha2::Sha512::digest(bytes);
        let vouched = format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(digest)
        );

        assert!(matches_integrity(&vouched, bytes).is_ok());
        assert!(matches_integrity(&vouched, b"something else").is_err());
    }

    /// SHA-1 is in older lockfiles and is not a hash to accept code on. An
    /// algorithm this does not know is refused rather than waved through.
    #[test]
    fn a_hash_this_does_not_accept_is_refused_rather_than_ignored() {
        assert!(matches_integrity("sha1-abc", b"anything").is_err());
        assert!(matches_integrity("md5-abc", b"anything").is_err());
        assert!(matches_integrity("nonsense", b"anything").is_err());
    }
}
