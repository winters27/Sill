//! Getting one extension's source onto the machine, at a named commit.
//!
//! ## Why not a git clone
//!
//! `.github/workflows/verify.yml` does exactly that for the two extensions the
//! view gate builds: `--filter=blob:none --sparse --depth 1` is 25 MB and four
//! seconds. It works, and it needs git.
//!
//! Sill already requires Node to run an extension at all and npm to install
//! one's dependencies, and npm arrives with Node. Git would be a third program
//! somebody has to have, wanted by exactly one feature, when the same bytes are
//! reachable over HTTP that the catalogue is already being fetched over. So
//! this is plain requests, and the store needs nothing installed that running
//! an extension did not already need.
//!
//! ## What it costs
//!
//! One call to list the extension's directory, one more per subdirectory it
//! has, and then the files themselves from `raw.githubusercontent.com`.
//! Measured on `uuid-generator`: **three API calls, 19 files, 158 KB, 2.1
//! seconds.**
//!
//! Only the first kind is rate limited. GitHub allows sixty an hour to an
//! unauthenticated address, so three per install is about twenty installs an
//! hour, and a token in settings raises it to five thousand. `raw` is a CDN
//! and is not counted, which is why the file bytes go through it and why the
//! token is never sent there: it is not needed for a public repository and a
//! credential should not travel further than the thing that asks for it.
//!
//! ## What is checked rather than trusted
//!
//! Every path that arrives from the API is checked to be inside the directory
//! being fetched before anything is written, the same guard `zip::enclosed_name`
//! performs for the speech engine archive. A response is somebody else's data
//! whatever it usually contains.

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The repository. MIT, and the extensions in it are MIT.
pub const REPO: &str = "raycast/extensions";

/// Sent on every request. GitHub refuses an API request without one.
pub const USER_AGENT: &str = concat!("Sill/", env!("CARGO_PKG_VERSION"));

/// Directories inside an extension that are never fetched.
///
/// `metadata` is the store's own screenshots, which are megabytes of PNG that
/// nothing here displays. `node_modules` should never be committed and is
/// listed because when it has been, it is enormous.
///
/// An exception list, so a directory somebody adds next year is fetched rather
/// than silently missing from the build.
const SKIP_DIRECTORIES: &[&str] = &["metadata", "node_modules"];

/// The most an extension may weigh before this refuses to fetch it.
///
/// Not a judgement about extensions, a bound on a download driven by somebody
/// else's numbers. The largest in the catalogue is a few megabytes; sixty-four
/// is far above anything real and far below anything that fills a disk.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// And the most files, for the same reason.
const MAX_FILES: usize = 3_000;

/// One file to fetch, by its path in the repository.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    /// Full repository path, for example `extensions/linear/src/index.tsx`.
    pub path: String,
    pub bytes: u64,
}

/// What a fetch produced.
#[derive(Debug, Clone, Default)]
pub struct Fetched {
    pub files: usize,
    pub bytes: u64,
}

// ------------------------------------------------------------------- shapes

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    path: String,
    sha: String,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct Tree {
    #[serde(default)]
    truncated: bool,
    tree: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    #[serde(rename = "type")]
    kind: String,
    path: String,
    #[serde(default)]
    size: u64,
}

// --------------------------------------------------------------------- pure

/// Where a repository path lands under the directory being filled.
///
/// `None` for anything that is not inside `folder`, which is the whole point:
/// the path comes from a response, it is joined onto a directory on this
/// machine, and a `..` in it would write outside that directory. Backslashes
/// are refused too, because on Windows they separate as well and a name
/// containing one would climb just as effectively.
pub fn under(folder: &str, path: &str) -> Option<PathBuf> {
    let prefix = format!("{}/", folder.trim_end_matches('/'));
    let relative = path.strip_prefix(&prefix)?;

    if relative.is_empty() {
        return None;
    }

    let mut out = PathBuf::new();

    for part in relative.split('/') {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\\')
            || part.contains(':')
        {
            return None;
        }
        out.push(part);
    }

    Some(out)
}

/// Whether a directory inside an extension is one this skips.
pub fn skipped(name: &str) -> bool {
    SKIP_DIRECTORIES.contains(&name)
}

/// The files in one directory listing, and the subdirectories still to read.
///
/// Split out of the request so the awkward answers are values in a test: a
/// listing containing a symlink, a submodule, or a path that is not under the
/// directory that was asked for.
pub fn read_listing(folder: &str, body: &str) -> Result<(Vec<Wanted>, Vec<String>), String> {
    let entries: Vec<Entry> = serde_json::from_str(body)
        .map_err(|err| format!("could not read the extension's file list: {err}"))?;

    let mut files = Vec::new();
    let mut directories = Vec::new();

    for entry in entries {
        if under(folder, &entry.path).is_none() {
            continue;
        }

        match entry.kind.as_str() {
            "file" => files.push(Wanted {
                path: entry.path,
                bytes: entry.size,
            }),
            "dir" if !skipped(&entry.name) => directories.push(entry.sha),
            // A symlink or a submodule. Neither is something to write into a
            // build directory, and neither has ever appeared here.
            _ => {}
        }
    }

    Ok((files, directories))
}

/// The files in one recursive tree response.
pub fn read_tree(folder: &str, parent: &str, body: &str) -> Result<Vec<Wanted>, String> {
    let tree: Tree = serde_json::from_str(body)
        .map_err(|err| format!("could not read the extension's file list: {err}"))?;

    if tree.truncated {
        return Err(format!(
            "{parent} has more files than one request can list, so this extension \
             cannot be installed from the store"
        ));
    }

    Ok(tree
        .tree
        .into_iter()
        .filter(|node| node.kind == "blob")
        .filter_map(|node| {
            let path = format!("{parent}/{}", node.path);
            // A blob inside a skipped directory arrives here too, because the
            // recursive tree returns everything below its root.
            let inside = under(folder, &path)?;
            inside
                .components()
                .all(|part| !skipped(&part.as_os_str().to_string_lossy()))
                .then_some(Wanted {
                    path,
                    bytes: node.size,
                })
        })
        .collect())
}

/// Where the bytes of one file are.
pub fn raw_url(revision: &str, path: &str) -> String {
    format!("https://raw.githubusercontent.com/{REPO}/{revision}/{path}")
}

/// Whether a fetch is within the bounds, said as a sentence when it is not.
pub fn within_bounds(files: &[Wanted]) -> Result<u64, String> {
    if files.len() > MAX_FILES {
        return Err(format!(
            "that extension has {} files, which is more than the store will fetch",
            files.len()
        ));
    }

    let bytes: u64 = files.iter().map(|file| file.bytes).sum();

    if bytes > MAX_BYTES {
        return Err(format!(
            "that extension is {} MB, which is more than the store will fetch",
            bytes / (1024 * 1024)
        ));
    }

    Ok(bytes)
}

/// What to say when GitHub has stopped answering because of how much has been
/// asked.
///
/// Names the setting that fixes it, because "403" about a request nobody made
/// on purpose is not something a person can act on. The reset time is carried
/// through as the header gives it rather than formatted here, since Rust has
/// no calendar in this crate and the window does.
pub fn rate_limited(remaining: Option<&str>) -> Option<String> {
    if remaining != Some("0") {
        return None;
    }

    Some(
        "GitHub is not answering any more requests from this machine for now. \
         It allows sixty an hour without a token, and an extension install uses \
         about three. Adding a GitHub token in Settings under Extensions raises \
         that to five thousand."
            .to_string(),
    )
}

// ----------------------------------------------------------------- fetching

/// One request to the GitHub API, with the token when there is one.
async fn api(client: &reqwest::Client, url: &str, token: Option<&str>) -> Result<String, String> {
    let mut request = client
        .get(url)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");

    // Only ever to the API. The file bytes come from a CDN that needs no
    // credential for a public repository, and a token should not travel
    // further than the thing that asks for it.
    if let Some(token) = token.filter(|it| !it.trim().is_empty()) {
        request = request.bearer_auth(token.trim());
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("could not reach GitHub: {err}"))?;

    let status = response.status();

    if !status.is_success() {
        let remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|it| it.to_str().ok())
            .map(str::to_string);

        if let Some(said) = rate_limited(remaining.as_deref()) {
            return Err(said);
        }

        return Err(format!("GitHub answered {status} for {url}"));
    }

    response
        .text()
        .await
        .map_err(|err| format!("GitHub's answer did not finish arriving: {err}"))
}

/// Every file in one extension's directory at one commit.
pub async fn list(
    client: &reqwest::Client,
    folder: &str,
    revision: &str,
    token: Option<&str>,
) -> Result<Vec<Wanted>, String> {
    let listing = api(
        client,
        &format!("https://api.github.com/repos/{REPO}/contents/{folder}?ref={revision}"),
        token,
    )
    .await?;

    let (mut files, directories) = read_listing(folder, &listing)?;

    if files.is_empty() && directories.is_empty() {
        return Err(format!("{folder} is empty at {revision}"));
    }

    // One recursive request per subdirectory, which for a normal extension is
    // `src` and `assets`. Asking the API for the whole repository tree instead
    // would come back truncated: this repository has a quarter of a million
    // files in it.
    for sha in directories {
        let body = api(
            client,
            &format!("https://api.github.com/repos/{REPO}/git/trees/{sha}?recursive=1"),
            token,
        )
        .await?;

        // The tree response gives paths relative to its own root, so the root
        // has to be put back. It is found from the listing rather than assumed.
        let parent = parent_of(folder, &listing, &sha)?;
        files.extend(read_tree(folder, &parent, &body)?);
    }

    Ok(files)
}

/// The repository path of the directory a tree sha belongs to.
fn parent_of(folder: &str, listing: &str, sha: &str) -> Result<String, String> {
    let entries: Vec<Entry> = serde_json::from_str(listing)
        .map_err(|err| format!("could not read the extension's file list: {err}"))?;

    entries
        .into_iter()
        .find(|entry| entry.sha == sha && entry.kind == "dir")
        .map(|entry| entry.path)
        .ok_or_else(|| format!("a directory in {folder} could not be placed"))
}

/// How many files are fetched at once.
///
/// Eight. Sequential took long enough on a sixty-file extension to look
/// stalled, and everything at once is a burst of sixty requests at a CDN for
/// no gain over eight.
const AT_ONCE: usize = 8;

/// Downloads every file into `destination`, keeping the layout.
pub async fn download(
    client: &reqwest::Client,
    folder: &str,
    revision: &str,
    files: &[Wanted],
    destination: &Path,
    report: crate::extension_install::Report<'_>,
) -> Result<Fetched, String> {
    use futures_util::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let bytes = within_bounds(files)?;

    // Every directory first, so the concurrent writes below never race to
    // create the same parent.
    for file in files {
        let Some(relative) = under(folder, &file.path) else {
            return Err(format!("{} is not inside {folder}", file.path));
        };
        if let Some(parent) = destination.join(&relative).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not make {}: {err}", parent.display()))?;
        }
    }

    // Owned before any future is made, rather than mapped over the borrowed
    // slice. A closure handed a reference and returning an async block that
    // captures it is not general over lifetimes, and the compiler reports that
    // where the command is registered rather than here, which is a long way
    // from the cause.
    let jobs: Vec<(String, String, PathBuf)> = files
        .iter()
        .map(|file| {
            (
                raw_url(revision, &file.path),
                file.path.clone(),
                destination.join(under(folder, &file.path).unwrap_or_default()),
            )
        })
        .collect();

    /*
     * Counted as they land rather than as they are started.
     *
     * These run several at a time, so "started" would reach the total while
     * the last few were still arriving and the bar would sit full through the
     * slowest part of the wait. Shared because the futures are concurrent and
     * each finishes on its own.
     */
    let done = AtomicUsize::new(0);
    let total = files.len();
    report(crate::extension_install::Progress::Fetching { done: 0, total });

    let results: Vec<Result<(), String>> = futures_util::stream::iter(jobs)
        .map(|(url, path, out)| {
            let done = &done;
            async move {
            let response = client
                .get(&url)
                .header(reqwest::header::USER_AGENT, USER_AGENT)
                .send()
                .await
                .map_err(|err| format!("could not fetch {path}: {err}"))?;

            if !response.status().is_success() {
                return Err(format!("{path} answered {}", response.status()));
            }

            let body = response
                .bytes()
                .await
                .map_err(|err| format!("{path} did not finish arriving: {err}"))?;

            std::fs::write(&out, &body)
                .map_err(|err| format!("could not write {}: {err}", out.display()))?;

            report(crate::extension_install::Progress::Fetching {
                done: done.fetch_add(1, Ordering::Relaxed) + 1,
                total,
            });
            Ok(())
            }
        })
        .buffer_unordered(AT_ONCE)
        .collect()
        .await;

    for result in results {
        result?;
    }

    Ok(Fetched {
        files: files.len(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FOLDER: &str = "extensions/demo";

    /// The check that stops a response writing outside the directory it is
    /// filling.
    #[test]
    fn a_path_that_climbs_out_is_refused() {
        assert_eq!(
            under(FOLDER, "extensions/demo/src/index.tsx"),
            Some(PathBuf::from("src").join("index.tsx"))
        );

        for bad in [
            "extensions/demo/../../../etc/passwd",
            "extensions/demo/./x",
            "extensions/other/x",
            "extensions/demo",
            "extensions/demo/",
            r"extensions/demo/a\b",
            "extensions/demo/C:evil",
        ] {
            assert_eq!(under(FOLDER, bad), None, "{bad} was accepted");
        }
    }

    /// A prefix match is not a directory match.
    #[test]
    fn a_sibling_whose_name_starts_the_same_is_not_inside() {
        assert_eq!(under("extensions/demo", "extensions/demo-two/x.ts"), None);
    }

    #[test]
    fn a_listing_separates_files_from_the_directories_still_to_read() {
        let body = r#"[
            {"type":"file","name":"package.json","path":"extensions/demo/package.json","sha":"a","size":10},
            {"type":"dir","name":"src","path":"extensions/demo/src","sha":"b","size":0},
            {"type":"dir","name":"metadata","path":"extensions/demo/metadata","sha":"c","size":0},
            {"type":"symlink","name":"link","path":"extensions/demo/link","sha":"d","size":0}
        ]"#;

        let (files, directories) = read_listing(FOLDER, body).expect("parses");

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "extensions/demo/package.json");
        assert_eq!(
            directories,
            vec!["b".to_string()],
            "metadata is skipped, and so is the symlink"
        );
    }

    #[test]
    fn a_tree_yields_every_blob_under_its_root() {
        let body = r#"{"truncated":false,"tree":[
            {"type":"blob","path":"index.tsx","size":100},
            {"type":"tree","path":"lib","size":0},
            {"type":"blob","path":"lib/helper.ts","size":50}
        ]}"#;

        let files = read_tree(FOLDER, "extensions/demo/src", body).expect("parses");
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();

        assert_eq!(
            paths,
            [
                "extensions/demo/src/index.tsx",
                "extensions/demo/src/lib/helper.ts"
            ]
        );
    }

    /// A committed `node_modules` inside `src` would otherwise be fetched file
    /// by file, because the skip happens on the directory listing above it.
    #[test]
    fn a_skipped_directory_nested_deeper_is_still_skipped() {
        let body = r#"{"truncated":false,"tree":[
            {"type":"blob","path":"index.tsx","size":1},
            {"type":"blob","path":"node_modules/left-pad/index.js","size":1}
        ]}"#;

        let files = read_tree(FOLDER, "extensions/demo/src", body).expect("parses");

        assert_eq!(files.len(), 1, "only the source file");
    }

    /// A truncated tree is a wrong answer, not a small one: files would be
    /// missing and the build would fail on an import nobody could explain.
    #[test]
    fn a_truncated_tree_refuses_rather_than_installing_half_an_extension() {
        let body = r#"{"truncated":true,"tree":[]}"#;
        assert!(read_tree(FOLDER, "extensions/demo/src", body).is_err());
    }

    #[test]
    fn a_download_is_bounded_by_both_count_and_weight() {
        let big = vec![Wanted {
            path: "extensions/demo/x".to_string(),
            bytes: MAX_BYTES + 1,
        }];
        assert!(within_bounds(&big).is_err());

        let many: Vec<Wanted> = (0..MAX_FILES + 1)
            .map(|n| Wanted {
                path: format!("extensions/demo/{n}"),
                bytes: 1,
            })
            .collect();
        assert!(within_bounds(&many).is_err());

        let fine = vec![Wanted {
            path: "extensions/demo/x".to_string(),
            bytes: 10,
        }];
        assert_eq!(within_bounds(&fine), Ok(10));
    }

    #[test]
    fn the_rate_limit_is_only_reported_when_it_is_the_reason() {
        assert!(rate_limited(Some("0")).is_some());
        assert!(rate_limited(Some("41")).is_none());
        assert!(rate_limited(None).is_none());
    }

    #[test]
    fn a_file_is_fetched_from_the_commit_rather_than_from_a_branch() {
        assert_eq!(
            raw_url("abc123", "extensions/demo/src/index.tsx"),
            "https://raw.githubusercontent.com/raycast/extensions/abc123/extensions/demo/src/index.tsx"
        );
    }
}
