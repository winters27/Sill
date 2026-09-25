//! Which whisper.cpp build upstream currently calls its release.
//!
//! The engine follows upstream on its own cadence rather than waiting for a
//! Sill release to carry a new pin, so this is what finds the newest build.
//!
//! ## The tags are a trap
//!
//! whisper.cpp publishes two kinds of release, and the binaries are on the
//! wrong one for asking naively:
//!
//! - `vX.Y.Z` is the version, and carries **no files at all**. It is also what
//!   `releases/latest` returns.
//! - `bNNNN` is the build tag, and carries `whisper-bin-x64.zip`. Most of them
//!   are nightlies with no version.
//!
//! A version's release notes name its build (`**Nightly build:** [b5130](...)`)
//! and both tags point at the same commit. So this reads the version, takes the
//! build its notes name, and **checks the two commits agree** before believing
//! it. The `prerelease` flags are not consulted, because upstream sets them
//! inconsistently: `b5130` is marked prerelease and `b4938` was not.
//!
//! Anything that does not line up is an error and nothing is offered. Guessing
//! at a build here would mean downloading and running a program on the
//! strength of the guess.

use serde_json::Value;

use crate::dictation::engine::Build;

const REPO: &str = "ggml-org/whisper.cpp";

/// The one archive Sill installs.
const ASSET: &str = "whisper-bin-x64.zip";

/// Where a published file for this repository is served from. A download URL
/// anywhere else is refused, whatever the API says.
const DOWNLOADS: &str = "https://github.com/ggml-org/whisper.cpp/releases/download/";

fn api_url(path: &str) -> String {
    format!("https://api.github.com/repos/{REPO}/{path}")
}

/// The newest release build, ready to install.
///
/// Five requests of a few kilobytes each, made only when somebody is looking at
/// the dictation settings and at most every few hours after that.
pub async fn latest(client: &reqwest::Client, token: Option<&str>) -> Result<Build, String> {
    let release = get(client, &api_url("releases/latest"), token).await?;

    let tag = release
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or("GitHub's latest whisper.cpp release has no tag")?;
    let version = version_of(tag)
        .ok_or_else(|| format!("whisper.cpp's latest release is tagged {tag}, not a version"))?;
    let build = release
        .get("body")
        .and_then(Value::as_str)
        .and_then(build_named_in)
        .ok_or_else(|| format!("whisper.cpp {tag}'s release notes do not name its build"))?;

    let tagged = commit_of(client, tag, token).await?;
    let built = commit_of(client, &build, token).await?;
    if tagged != built {
        return Err(format!(
            "whisper.cpp {tag} and {build} are different commits, so {build} is not that release"
        ));
    }

    let published = get(client, &api_url(&format!("releases/tags/{build}")), token).await?;
    build_from(&version, &build, &published)
}

/// The commit a tag points at, following an annotated tag to its target.
///
/// `v` tags are annotated and `b` tags are lightweight, so both shapes turn up
/// in one check.
async fn commit_of(client: &reqwest::Client, tag: &str, token: Option<&str>) -> Result<String, String> {
    let reference = get(client, &api_url(&format!("git/ref/tags/{tag}")), token).await?;
    let (kind, sha) = object_of(&reference).ok_or_else(|| format!("tag {tag} points at nothing"))?;

    if kind == "commit" {
        return Ok(sha);
    }

    let annotated = get(client, &api_url(&format!("git/tags/{sha}")), token).await?;
    match object_of(&annotated) {
        Some((kind, sha)) if kind == "commit" => Ok(sha),
        _ => Err(format!("tag {tag} does not lead to a commit")),
    }
}

async fn get(client: &reqwest::Client, url: &str, token: Option<&str>) -> Result<Value, String> {
    let text = crate::store::source::api(client, url, token).await?;
    serde_json::from_str(&text).map_err(|err| format!("GitHub's answer for {url} was not JSON: {err}"))
}

/// `(type, sha)` out of a git ref or tag object.
fn object_of(value: &Value) -> Option<(String, String)> {
    let object = value.get("object")?;
    Some((
        object.get("type")?.as_str()?.to_string(),
        object.get("sha")?.as_str()?.to_string(),
    ))
}

/// `v1.9.4` as `1.9.4`. Anything that is not three numbers is not a version.
pub fn version_of(tag: &str) -> Option<String> {
    let core = tag.strip_prefix('v')?;
    let parts: Vec<&str> = core.split('.').collect();
    (parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
        .then(|| core.to_string())
}

/// The build tag a version's release notes link to.
///
/// Looks only at the line that says it is the build, so a changelog that
/// happens to link some other release is not mistaken for it.
pub fn build_named_in(body: &str) -> Option<String> {
    body.lines()
        .filter(|line| line.to_ascii_lowercase().contains("nightly build"))
        .find_map(|line| {
            let (_, rest) = line.split_once("/releases/tag/")?;
            let tag: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            let digits = tag.strip_prefix('b')?;
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some(tag)
        })
}

/// The installable build out of a build tag's release.
pub fn build_from(version: &str, build: &str, release: &Value) -> Result<Build, String> {
    let asset = release
        .get("assets")
        .and_then(Value::as_array)
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| asset.get("name").and_then(Value::as_str) == Some(ASSET))
        })
        .ok_or_else(|| format!("whisper.cpp {build} has no {ASSET}"))?;

    let url = asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{ASSET} in {build} has no download address"))?;
    if !url.starts_with(DOWNLOADS) {
        return Err(format!("{ASSET} in {build} is served from {url}, which is not whisper.cpp's"));
    }

    // Without a published digest there is nothing to check the bytes against,
    // and nothing unchecked is ever run.
    let sha256 = asset
        .get("digest")
        .and_then(Value::as_str)
        .filter(|digest| is_sha256(digest))
        .ok_or_else(|| format!("GitHub publishes no sha256 for {ASSET} in {build}"))?;

    let bytes = asset.get("size").and_then(Value::as_u64).unwrap_or(0);

    Ok(Build {
        version: format!("{version}+{build}"),
        url: url.to_string(),
        sha256: sha256.to_string(),
        bytes,
    })
}

fn is_sha256(digest: &str) -> bool {
    digest
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The start of `v1.9.4`'s release notes, as published.
    const NOTES: &str = "## Overview\n\nNew version has been released.\n\n\
        **Nightly build:** [b5130](https://github.com/ggml-org/whisper.cpp/releases/tag/b5130)\n\
        **More info:** [dist : releases and versioning of ggml-org projects](https://github.com/ggml-org/ggml/discussions/1579)\n\n\
        ## Changelog since v1.9.3\n";

    fn release(assets: Value) -> Value {
        json!({ "tag_name": "b5130", "assets": assets })
    }

    fn zip_asset() -> Value {
        json!({
            "name": "whisper-bin-x64.zip",
            "size": 8_573_270,
            "digest": "sha256:f9ec6c52a2e949b62ab51fa21d0d497958f9e41c3010c157c4e42932d5316f3c",
            "browser_download_url": "https://github.com/ggml-org/whisper.cpp/releases/download/b5130/whisper-bin-x64.zip"
        })
    }

    #[test]
    fn a_version_tag_is_read_as_a_version() {
        assert_eq!(version_of("v1.9.4").as_deref(), Some("1.9.4"));
        assert_eq!(version_of("v1.10.0").as_deref(), Some("1.10.0"));
    }

    #[test]
    fn a_build_tag_is_not_a_version() {
        // `releases/latest` could one day answer with a build tag. It has to
        // be refused rather than read as version 5130.
        for tag in ["b5130", "1.9.4", "v1.9", "v1.9.4.1", "v1.9.x", "v1..4", ""] {
            assert_eq!(version_of(tag), None, "{tag:?}");
        }
    }

    #[test]
    fn the_notes_name_the_build() {
        assert_eq!(build_named_in(NOTES).as_deref(), Some("b5130"));
    }

    #[test]
    fn only_the_build_line_is_read() {
        // A changelog line linking another release must not be taken for it.
        let notes = "See [b4938](https://github.com/ggml-org/whisper.cpp/releases/tag/b4938)\n\
                     **Nightly build:** [b5130](https://github.com/ggml-org/whisper.cpp/releases/tag/b5130)";
        assert_eq!(build_named_in(notes).as_deref(), Some("b5130"));
    }

    #[test]
    fn notes_without_a_build_name_nothing() {
        assert_eq!(build_named_in("## Overview\n\nNew version has been released."), None);
        assert_eq!(
            build_named_in("**Nightly build:** [v1.9.4](https://github.com/x/y/releases/tag/v1.9.4)"),
            None,
            "a version tag in the build line is not a build"
        );
    }

    #[test]
    fn the_archive_becomes_an_installable_build() {
        let build = build_from("1.9.4", "b5130", &release(json!([zip_asset()]))).unwrap();

        assert_eq!(build.version, "1.9.4+b5130");
        assert_eq!(build.bytes, 8_573_270);
        assert!(build.url.ends_with("/b5130/whisper-bin-x64.zip"));
        assert!(build.sha256.starts_with("sha256:f9ec6c52"));
        assert!(
            crate::dictation::engine::parse_version(&build.version).is_some(),
            "the version has to be one the installer can order"
        );
    }

    #[test]
    fn the_other_windows_archives_are_not_taken() {
        // The BLAS and CUDA builds are named alike and are not what the
        // include set was measured against.
        let blas = json!({
            "name": "whisper-blas-bin-x64.zip",
            "size": 1,
            "digest": format!("sha256:{}", "a".repeat(64)),
            "browser_download_url": "https://github.com/ggml-org/whisper.cpp/releases/download/b5130/whisper-blas-bin-x64.zip"
        });
        assert!(build_from("1.9.4", "b5130", &release(json!([blas]))).is_err());
    }

    #[test]
    fn a_build_without_a_digest_is_refused() {
        let mut asset = zip_asset();
        asset.as_object_mut().unwrap().remove("digest");
        assert!(build_from("1.9.4", "b5130", &release(json!([asset]))).is_err());
    }

    #[test]
    fn a_malformed_digest_is_refused() {
        for digest in ["sha256:short", "md5:0123", "f9ec6c52a2e949b62ab51fa21d0d497958f9e41c3010c157c4e42932d5316f3c"] {
            let mut asset = zip_asset();
            asset["digest"] = json!(digest);
            assert!(
                build_from("1.9.4", "b5130", &release(json!([asset]))).is_err(),
                "{digest}"
            );
        }
    }

    #[test]
    fn a_download_served_from_elsewhere_is_refused() {
        let mut asset = zip_asset();
        asset["browser_download_url"] = json!("https://example.com/whisper-bin-x64.zip");
        assert!(build_from("1.9.4", "b5130", &release(json!([asset]))).is_err());
    }

    #[test]
    fn annotated_and_lightweight_tags_both_yield_an_object() {
        let lightweight = json!({ "object": { "type": "commit", "sha": "927cfce3" } });
        let annotated = json!({ "object": { "type": "tag", "sha": "7d75b149" } });

        assert_eq!(object_of(&lightweight), Some(("commit".into(), "927cfce3".into())));
        assert_eq!(object_of(&annotated), Some(("tag".into(), "7d75b149".into())));
        assert_eq!(object_of(&json!({})), None);
    }

    /// Asks the real GitHub, for when upstream changes shape.
    ///
    /// ```text
    /// cargo test --lib dictation::upstream::tests::latest_resolves -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "talks to GitHub"]
    async fn latest_resolves_against_github() {
        let build = latest(&crate::dictation::fetch::client(), None)
            .await
            .expect("upstream should resolve");
        println!("{build:?}");
        assert!(crate::dictation::engine::parse_version(&build.version).is_some());
    }
}
