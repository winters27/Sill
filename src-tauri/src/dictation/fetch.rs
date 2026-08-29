//! Downloading and verifying a large file.
//!
//! Both things dictation installs are big: the smallest model is 74 MB and
//! the default is 465 MB. So the download streams to disk rather than into
//! memory, reports progress as it goes, and is checked against a published
//! digest before anything is moved into place. A truncated model does not
//! fail loudly, it loads and transcribes gibberish.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use crate::dictation::error::{DictationError, Result};

/// No whole-request timeout: a 465 MB model on a slow connection legitimately
/// takes minutes, and a ceiling here would cancel it at the worst moment.
/// The connect timeout is what bounds an unreachable host.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Longest gap allowed between bytes arriving. This is the useful timeout for
/// a long transfer: it catches a stalled connection without punishing a slow
/// but working one.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .expect("a download client with timeouts must build")
}

/// Streams `url` into `destination`, calling `progress(downloaded, total)`.
///
/// `total` is 0 when the server sends no content length. Callers that know
/// the published size should prefer it over rendering an unknown total as a
/// full progress bar.
pub async fn download_to(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<()> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| DictationError::Network(format!("Could not reach {url}: {e}")))?;

    if !response.status().is_success() {
        return Err(DictationError::Network(format!(
            "{url} answered {}",
            response.status()
        )));
    }

    let total = response.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(destination)?;
    let mut downloaded = 0u64;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|e| DictationError::Network(format!("Transfer interrupted: {e}")))?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        progress(downloaded, total);
    }

    file.flush()?;
    Ok(())
}

/// Hex sha256 of a file, read in blocks so a 1.5 GB model never lands in RAM.
pub fn digest_of(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Checks `path` against `expected`, deleting it when it does not match.
///
/// The file is removed on failure rather than left behind: a bad download
/// that stays on disk is indistinguishable from a good one to anything that
/// only checks whether the path exists, and every "is it installed" check
/// does exactly that.
pub fn verify(path: &Path, expected: &str) -> Result<()> {
    let expected = expected.strip_prefix("sha256:").unwrap_or(expected);
    let actual = digest_of(path)?;

    if actual.eq_ignore_ascii_case(expected) {
        return Ok(());
    }

    let _ = std::fs::remove_file(path);
    Err(DictationError::Validation(format!(
        "The download did not match its published checksum, so it was discarded. \
         Expected {expected}, got {actual}"
    )))
}

/// Moves `staged` onto `destination` through a same-directory rename.
///
/// The staging file has to be a sibling of the destination, or the rename
/// crosses a filesystem boundary and stops being atomic.
pub fn commit(staged: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(staged, destination) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(staged);
            Err(DictationError::Io(err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_file(contents: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("payload.bin");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(contents).expect("write");
        (dir, path)
    }

    /// sha256 of the empty string, the one digest that is easy to look up.
    const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn the_digest_matches_a_known_value() {
        let (_dir, path) = temp_file(b"");
        assert_eq!(digest_of(&path).expect("digest"), EMPTY_SHA256);
    }

    #[test]
    fn a_matching_digest_passes_with_or_without_the_prefix() {
        let (_dir, path) = temp_file(b"");
        verify(&path, EMPTY_SHA256).expect("bare digest");
        verify(&path, &format!("sha256:{EMPTY_SHA256}")).expect("prefixed digest");
        assert!(path.is_file(), "a good file must survive verification");
    }

    #[test]
    fn a_mismatch_deletes_the_file() {
        // The whole point: a bad download left on disk looks installed to
        // every "does the path exist" check in the module.
        let (_dir, path) = temp_file(b"not empty");
        let err = verify(&path, EMPTY_SHA256).expect_err("must reject");

        assert!(!path.exists(), "a bad download must not be left behind");
        assert!(
            err.to_string().contains("checksum"),
            "the message should say why, got {err}"
        );
    }

    #[test]
    fn committing_moves_the_file_and_creates_the_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let staged = dir.path().join(".model.partial");
        std::fs::write(&staged, b"payload").expect("write");

        let destination = dir.path().join("models").join("model.bin");
        commit(&staged, &destination).expect("commit");

        assert!(!staged.exists(), "the staging file is consumed");
        assert_eq!(std::fs::read(&destination).expect("read"), b"payload");
    }
}
