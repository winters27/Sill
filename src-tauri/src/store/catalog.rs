//! Getting the list of extensions, and not getting it more often than that.
//!
//! ## Where it comes from
//!
//! Raycast's public store index. It is the only thing that aggregates this:
//! the repository holds one `package.json` per extension and no summary of
//! itself, so the alternative is three thousand requests to build a list of
//! titles. It is read the way a browser reads the public store page, and
//! nothing here signs in, sends anything, or asks for a private field.
//!
//! **It supplies metadata only.** No code is fetched from it, ever. What it
//! contributes to an install is the commit hash, and [`super::source`] fetches
//! the source itself from the MIT repository at that hash. If this index
//! changes shape or goes away, browsing stops working and installing from a
//! folder does not, which is the right way round.
//!
//! ## Why the whole thing at once
//!
//! The index pages, and the page parameter is the only one it honours: `search`,
//! `category` and `sort` were each tried and each returned the unfiltered first
//! page. So filtering and ranking happen here, over the whole catalogue, which
//! is where the constitution wants them anyway.
//!
//! Seven requests, 19 MB uncompressed and 3.5 MB with `Accept-Encoding: gzip`,
//! in about six seconds. That is a real cost and it is paid once: the reduced
//! form is written to disk and read back until it goes stale.
//!
//! ## What is dropped
//!
//! Almost everything. A single listing carries the author's biography, every
//! past contributor with theirs, the full description of every AI tool, and a
//! dozen prompt examples. Reducing at the point of parsing is what turns 19 MB
//! into 2 MB, and it happens here rather than in the window because the window
//! should never see the 19 MB at all.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ListedCommand, Listing};

/// The public index every Raycast client reads.
const INDEX: &str = "https://backend.raycast.com/api/v1/store_listings";

/// How many listings one request asks for.
///
/// Measured: 500 comes back in under a second and 3 MB, and the seven requests
/// that covers the catalogue are faster than the sixty-five that `per_page=50`
/// would need.
const PAGE: usize = 500;

/// A stop, so a paging endpoint that never runs out cannot spin forever.
///
/// Twenty pages is ten thousand extensions against the three thousand that
/// exist, so it is a runaway guard rather than a limit anybody reaches.
const MAX_PAGES: usize = 20;

/// How long a fetched catalogue is treated as current.
///
/// Six hours. The index's own `Cache-Control` says three minutes, which is
/// right for a web page somebody is refreshing and wrong for a launcher: this
/// costs three and a half megabytes and six seconds, and an extension store
/// does not change enough in an afternoon to be worth that on every open.
/// Anybody who wants it sooner has a refresh in the store itself.
pub const FRESH_FOR: i64 = 6 * 60 * 60;

/// The shape of the file on disk.
///
/// Bumped when [`Listing`] changes. An older file is treated as stale rather
/// than as an error, so an upgrade refetches quietly instead of failing to
/// open the store.
///
/// 2 added the icon. Without the bump a cache written by the previous build
/// deserialises perfectly and every row draws a lettered tile, which is the
/// quietest kind of wrong: nothing fails, the store just looks unfinished
/// until the six hours are up.
const FORMAT: u32 = 2;

/// Statuses that are not offered.
///
/// An exception list rather than a list of the ones that are, which is the
/// rule this codebase arrived at the hard way: a status the index invents next
/// year shows up rather than silently emptying the store.
const RETIRED: &[&str] = &["deprecated"];

/// The platform Sill is.
///
/// Raycast ships for macOS and for Windows and its extensions say which they
/// support, so the index is two stores in one file. **An extension that names
/// its platforms and does not name this one is dropped at the point of
/// parsing**, before it reaches disk or memory: it is not a thing Sill can
/// offer and carrying it would mean holding a second product's catalogue for
/// no purpose.
///
/// Measured against the index on 2026-09-01: 3,234 listings, of which 886 name
/// Windows, 1,048 name macOS and not Windows, and **1,300 name nothing at
/// all**. That third group is the reason this drops rather than filters on the
/// stronger test. The field arrived with Raycast's Windows build, so an
/// extension that never declared one predates the question instead of
/// answering it, and treating silence as refusal would throw away two fifths
/// of the store, most of it ordinary JavaScript that works. Silence is kept,
/// marked, and hidden by default where the person looking can see the count
/// and change their mind.
const PLATFORM: &str = "Windows";

/// The catalogue, reduced, with when it was fetched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub format: u32,
    /// Seconds since the epoch.
    pub fetched_at: i64,
    pub listings: Vec<Listing>,
}

/// Whether a catalogue is still worth reading instead of refetching.
///
/// Its own function because both halves are worth being sure about with no
/// machine involved: a file written by an older Sill is stale whatever its
/// timestamp says, and a clock that has gone backwards must read as stale
/// rather than as fresh for the next six hours.
pub fn is_fresh(catalog: &Catalog, now: i64) -> bool {
    catalog.format == FORMAT && now >= catalog.fetched_at && now - catalog.fetched_at < FRESH_FOR
}

/// Where the reduced catalogue is kept.
pub fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("catalog.json")
}

// ------------------------------------------------------------------ parsing

/// The fields of a store listing this reads. Everything else is ignored.
#[derive(Deserialize)]
struct Raw {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    relative_path: Option<String>,
    commit_sha: Option<String>,
    download_count: Option<u64>,
    status: Option<String>,
    categories: Option<Vec<String>>,
    platforms: Option<Vec<String>>,
    author: Option<RawAuthor>,
    commands: Option<Vec<RawCommand>>,
    icons: Option<RawIcons>,
}

/// The two icons a listing can carry, either of which can be absent.
#[derive(Deserialize)]
struct RawIcons {
    light: Option<String>,
    dark: Option<String>,
}

#[derive(Deserialize)]
struct RawAuthor {
    handle: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct RawCommand {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
    mode: Option<String>,
}

#[derive(Deserialize)]
struct Page {
    data: Vec<Raw>,
}

/// The repository path an extension's source is at, if it is a sane one.
///
/// This string is pasted into a URL and then into a filesystem path, so it is
/// checked rather than trusted. An index that answered `extensions/../../x`
/// would otherwise send the fetcher outside the directory it means to read and
/// the writer outside the directory it means to fill. Nothing suggests the
/// index would; the check costs one function and removes the question.
pub fn folder_of(relative_path: &str) -> Option<String> {
    let trimmed = relative_path.trim().trim_end_matches('/');

    if trimmed.is_empty() || !trimmed.starts_with("extensions/") {
        return None;
    }

    let clean = trimmed.split('/').all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && !part.contains('\\')
            && !part.contains(':')
    });

    clean.then(|| trimmed.to_string())
}

/// One raw listing, reduced, or nothing when it is not usable.
///
/// A listing with no commit, no folder or no commands cannot be installed, and
/// offering a row that fails the moment it is pressed is worse than not
/// offering it.
fn reduce(raw: Raw) -> Option<Listing> {
    if raw
        .status
        .as_deref()
        .is_some_and(|status| RETIRED.contains(&status))
    {
        return None;
    }

    let platforms = raw.platforms.unwrap_or_default();

    // Names its platforms and this is not one of them. Another product's
    // extension, dropped here rather than carried and filtered later.
    if !platforms.is_empty() && !platforms.iter().any(|it| it == PLATFORM) {
        return None;
    }

    let name = raw.name.filter(|it| !it.is_empty())?;
    let folder = folder_of(&raw.relative_path?)?;
    let revision = raw.commit_sha.filter(|it| !it.is_empty())?;

    let commands: Vec<ListedCommand> = raw
        .commands
        .unwrap_or_default()
        .into_iter()
        .filter_map(|command| {
            let name = command.name?;
            Some(ListedCommand {
                title: command.title.unwrap_or_else(|| name.clone()),
                name,
                description: command.description.unwrap_or_default(),
                mode: command.mode.unwrap_or_default(),
            })
        })
        .collect();

    if commands.is_empty() {
        return None;
    }

    let author = raw
        .author
        .and_then(|it| it.handle.or(it.name))
        .unwrap_or_default();

    // The dark variant first, falling back to the light one. Every theme Sill
    // ships is dark, and `icons.dark` is the artwork an author drew for a dark
    // background. It is null on nearly every listing, which is why the
    // fallback is the common case rather than the exception.
    //
    // Empty is an ordinary answer: plenty of listings carry neither, and the
    // window draws a lettered tile for those, the way the launcher already
    // does for an application whose icon the shell cannot produce.
    let icon = raw
        .icons
        .and_then(|it| it.dark.or(it.light))
        .filter(|url| !url.is_empty())
        .unwrap_or_default();

    Some(Listing {
        title: raw.title.clone().unwrap_or_else(|| name.clone()),
        name,
        folder,
        description: raw.description.unwrap_or_default(),
        author,
        categories: raw.categories.unwrap_or_default(),
        platforms,
        revision,
        downloads: raw.download_count.unwrap_or(0),
        icon,
        commands,
        // Everything here came out of the index, which is what this means.
        native: false,
    })
}

/// Every usable listing in one page of the index.
///
/// Separated from fetching so the awkward listings are values in a test rather
/// than something the network has to be persuaded to return.
pub fn listings_in(body: &str) -> Result<Vec<Listing>, String> {
    let page: Page = serde_json::from_str(body)
        .map_err(|err| format!("the store index was unreadable: {err}"))?;

    Ok(page.data.into_iter().filter_map(reduce).collect())
}

// ----------------------------------------------------------------- fetching

/// Fetches the whole catalogue.
///
/// Sequential rather than parallel on purpose: seven requests against somebody
/// else's index, taken one at a time, is a polite shape and it is already fast
/// enough. Firing them together would save four seconds and read as a scrape.
pub async fn fetch(client: &reqwest::Client) -> Result<Catalog, String> {
    let mut listings = Vec::new();

    for page in 1..=MAX_PAGES {
        let url = format!("{INDEX}?per_page={PAGE}&page={page}");

        let response = client
            .get(&url)
            .header(reqwest::header::USER_AGENT, super::source::USER_AGENT)
            .send()
            .await
            .map_err(|err| format!("could not reach the extension store: {err}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "the extension store answered {} when asked for page {page}",
                response.status()
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|err| format!("the store index did not finish arriving: {err}"))?;

        let batch = listings_in(&body)?;

        // An empty page is the end. The index has no total and no next link,
        // so running out is the only signal there is.
        if batch.is_empty() {
            break;
        }

        listings.extend(batch);
    }

    if listings.is_empty() {
        return Err("the extension store returned nothing at all".to_string());
    }

    Ok(Catalog {
        format: FORMAT,
        fetched_at: crate::state::now_seconds(),
        listings,
    })
}

/// Reads the catalogue off disk, if there is a readable one.
pub fn read_cache(path: &Path) -> Option<Catalog> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes it, ignoring a failure.
///
/// A cache that cannot be written is slower, not broken: the next open fetches
/// again. Failing the browse over it would turn a full disk into "the
/// extension store is unavailable", which names the wrong thing.
pub fn write_cache(path: &Path, catalog: &Catalog) {
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }

    if let Ok(text) = serde_json::to_string(catalog) {
        let _ = std::fs::write(path, text);
    }
}

/// The catalogue, from disk when it is current and from the network when it is
/// not.
///
/// `refresh` is the store's own refresh action and skips the disk copy
/// entirely. Nothing else here ever fetches: there is no timer, no warm-up and
/// no revalidation behind the user's back.
pub async fn load(data_dir: &Path, refresh: bool) -> Result<Catalog, String> {
    let path = cache_path(data_dir);

    if !refresh {
        if let Some(cached) = read_cache(&path) {
            if is_fresh(&cached, crate::state::now_seconds()) {
                return Ok(cached);
            }
        }
    }

    let client = crate::dictation::fetch::client();

    match fetch(&client).await {
        Ok(fresh) => {
            write_cache(&path, &fresh);
            Ok(fresh)
        }
        // A stale copy beats no store at all. Somebody on a train should be
        // able to look at what they already had, and the browse reports when
        // it was fetched so a stale list never pretends to be current.
        Err(err) => match read_cache(&path) {
            Some(stale) if stale.format == FORMAT => Ok(stale),
            _ => Err(err),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = r#"{
        "data": [{
            "name": "translate",
            "title": "Google Translate",
            "description": "Translate things",
            "relative_path": "extensions/google-translate/",
            "commit_sha": "abc123",
            "download_count": 42,
            "status": "active",
            "categories": ["Web", "Productivity"],
            "platforms": ["macOS", "Windows"],
            "author": { "handle": "someone", "name": "Some One" },
            "icons": { "light": "https://files.raycast.com/light-one", "dark": null },
            "commands": [
                { "name": "translate", "title": "Translate", "mode": "view", "description": "d" }
            ]
        }]
    }"#;

    #[test]
    fn a_listing_reduces_to_what_browsing_and_installing_need() {
        let listings = listings_in(ONE).expect("parses");
        assert_eq!(listings.len(), 1);

        let one = &listings[0];
        assert_eq!(one.name, "translate");
        assert_eq!(
            one.folder, "extensions/google-translate",
            "no trailing slash"
        );
        assert_eq!(one.author, "someone", "the handle, which is what is shown");
        assert_eq!(one.revision, "abc123");
        assert_eq!(one.commands.len(), 1);
    }

    /// The index is somebody else's and its nulls are ordinary.
    #[test]
    fn a_listing_missing_every_optional_field_still_parses() {
        let listings = listings_in(
            r#"{"data":[{
                "name": "bare",
                "relative_path": "extensions/bare",
                "commit_sha": "sha",
                "commands": [{ "name": "go" }]
            }]}"#,
        )
        .expect("parses");

        let one = &listings[0];
        assert_eq!(one.title, "bare", "the slug stands in for a missing title");
        assert_eq!(one.commands[0].title, "go");
        assert_eq!(
            one.commands[0].mode, "",
            "and an unstated mode is unrunnable"
        );
        assert!(!one.commands[0].runnable());
        assert!(one.categories.is_empty());
        assert!(one.platforms.is_empty());
    }

    #[test]
    fn a_listing_that_could_not_be_installed_is_not_offered() {
        for body in [
            // Nothing to fetch.
            r#"{"data":[{"name":"x","commit_sha":"s","commands":[{"name":"c"}]}]}"#,
            // Nothing to pin.
            r#"{"data":[{"name":"x","relative_path":"extensions/x","commands":[{"name":"c"}]}]}"#,
            // Nothing to run.
            r#"{"data":[{"name":"x","relative_path":"extensions/x","commit_sha":"s","commands":[]}]}"#,
        ] {
            assert!(listings_in(body).expect("parses").is_empty(), "{body}");
        }
    }

    #[test]
    fn a_retired_listing_is_left_out() {
        let listings = listings_in(
            r#"{"data":[{
                "name":"old","relative_path":"extensions/old","commit_sha":"s",
                "status":"deprecated","commands":[{"name":"c","mode":"view"}]
            }]}"#,
        )
        .expect("parses");

        assert!(listings.is_empty());
    }

    /// The icon survives the reduce.
    ///
    /// It was dropped by the first version of this file, which is a failure
    /// with no symptom worth the name: nothing errors, every row draws its
    /// lettered fallback, and a store of eighty grey tiles reads as unfinished
    /// rather than as a field somebody forgot to carry.
    #[test]
    fn the_icon_is_carried_and_the_dark_one_wins() {
        let listings = listings_in(ONE).expect("parses");
        assert_eq!(listings[0].icon, "https://files.raycast.com/light-one");

        let both = listings_in(
            r#"{"data":[{
                "name":"x","relative_path":"extensions/x","commit_sha":"s",
                "commands":[{"name":"c","mode":"view"}],
                "icons":{"light":"L","dark":"D"}
            }]}"#,
        )
        .expect("parses");

        assert_eq!(
            both[0].icon, "D",
            "every theme Sill ships is dark, so the dark artwork is the right one"
        );
    }

    #[test]
    fn a_listing_with_no_icon_carries_an_empty_one_rather_than_failing() {
        for icons in [
            r#""icons":{"light":null,"dark":null}"#,
            r#""icons":null"#,
            r#""x":1"#,
        ] {
            let listings = listings_in(&format!(
                r#"{{"data":[{{
                    "name":"x","relative_path":"extensions/x","commit_sha":"s",
                    "commands":[{{"name":"c","mode":"view"}}], {icons}
                }}]}}"#
            ))
            .expect("parses");

            assert_eq!(listings.len(), 1, "{icons}");
            assert!(listings[0].icon.is_empty(), "{icons}");
        }
    }

    /// The index is two products' stores in one file.
    #[test]
    fn an_extension_that_names_only_the_other_platform_never_reaches_the_catalogue() {
        let listings = listings_in(
            r#"{"data":[{
                "name":"mac-only","relative_path":"extensions/mac-only","commit_sha":"s",
                "platforms":["macOS"],"commands":[{"name":"c","mode":"view"}]
            }]}"#,
        )
        .expect("parses");

        assert!(listings.is_empty(), "it is not a thing Sill can offer");
    }

    /// The 1,300 that predate the field are kept, and the store marks them.
    #[test]
    fn an_extension_that_names_no_platform_is_kept() {
        let listings = listings_in(
            r#"{"data":[{
                "name":"quiet","relative_path":"extensions/quiet","commit_sha":"s",
                "commands":[{"name":"c","mode":"view"}]
            }]}"#,
        )
        .expect("parses");

        assert_eq!(listings.len(), 1);
        assert!(listings[0].platforms.is_empty());
        assert!(!listings[0].declares_windows());
    }

    /// A status nobody here has heard of is shown rather than hidden, which is
    /// the direction this codebase settled on.
    #[test]
    fn an_unfamiliar_status_is_still_offered() {
        let listings = listings_in(
            r#"{"data":[{
                "name":"new","relative_path":"extensions/new","commit_sha":"s",
                "status":"something-invented-later","commands":[{"name":"c","mode":"view"}]
            }]}"#,
        )
        .expect("parses");

        assert_eq!(listings.len(), 1);
    }

    /// This string becomes a URL and then a path on disk.
    #[test]
    fn a_repository_path_that_climbs_out_is_refused() {
        assert_eq!(
            folder_of("extensions/linear/"),
            Some("extensions/linear".to_string())
        );
        assert_eq!(
            folder_of("extensions/a/b"),
            Some("extensions/a/b".to_string()),
            "a nested path is fine, it is the climbing that is not"
        );

        for bad in [
            "extensions/../../etc",
            "extensions/./x",
            "/etc/passwd",
            "../extensions/x",
            "extensions/x\\y",
            "extensions/C:x",
            "",
            "   ",
        ] {
            assert_eq!(folder_of(bad), None, "{bad} was accepted");
        }
    }

    #[test]
    fn a_file_from_an_older_sill_is_stale_however_new_it_is() {
        let catalog = Catalog {
            format: FORMAT - 1,
            fetched_at: 1_000,
            listings: Vec::new(),
        };

        assert!(!is_fresh(&catalog, 1_001));
    }

    #[test]
    fn a_clock_that_went_backwards_reads_as_stale_rather_than_fresh_for_hours() {
        let catalog = Catalog {
            format: FORMAT,
            fetched_at: 10_000,
            listings: Vec::new(),
        };

        assert!(!is_fresh(&catalog, 9_000));
        assert!(is_fresh(&catalog, 10_000 + FRESH_FOR - 1));
        assert!(!is_fresh(&catalog, 10_000 + FRESH_FOR));
    }

    #[test]
    fn a_catalog_round_trips_through_its_file_shape() {
        let catalog = Catalog {
            format: FORMAT,
            fetched_at: 5,
            listings: listings_in(ONE).expect("parses"),
        };

        let text = serde_json::to_string(&catalog).expect("serialises");
        let back: Catalog = serde_json::from_str(&text).expect("parses");

        assert_eq!(back, catalog);
    }
}
