//! Whether anything installed from the store is behind, asked cheaply enough
//! to ask on a summon.
//!
//! ## Why this is not just a catalogue refresh
//!
//! The store already knows how to answer this. [`super::browse`] counts
//! outdated extensions on every keystroke and has since it was written. It
//! answers by reading the whole catalogue, and the whole catalogue is **seven
//! requests and 3.8 MB gzipped**, measured, which is a fine price for opening a
//! shelf of three thousand extensions and an absurd one for learning whether
//! the four you have are current.
//!
//! So this asks about the four. Raycast publishes a listing per extension at
//! `/extensions/{author}/{name}` carrying the same `commit_sha` the index
//! does, and one of those is **16.8 KB gzipped**, measured. Under
//! [`ASK_EACH_UP_TO`] installs that is cheaper on bytes, on time and on every
//! axis that matters; past it the catalogue wins and this asks for that
//! instead.
//!
//! ## What was tried and does not work
//!
//! **Conditional requests.** The listing endpoint sends `ETag` and
//! `Cache-Control: must-revalidate`, which promises that a second ask costs a
//! header and a 304. It does not: `If-None-Match` with that exact tag, with and
//! without its `W/` prefix, is answered **200 with the whole body**. The
//! politeness of an `ETag` is there and the saving is not, so nothing here
//! pretends otherwise and the throttle does the work instead.
//!
//! **Asking GitHub.** The source already comes from `raycast/extensions`, and
//! the newest commit touching a folder is two kilobytes to fetch. It is also
//! the wrong answer. The store publishes a commit; the repository has commits
//! the store has not published. Installing the newest one would break the thing
//! [`super`] is built on, which is that what lands on this machine is the
//! revision the catalogue named.
//!
//! ## Why there is no timer
//!
//! There is no thread, no task and no interval. The check runs when the
//! launcher is summoned, which is a moment somebody created, and refuses to run
//! again for [`ASK_AGAIN_AFTER`]. A machine with nothing installed from the
//! store never opens a socket for this at all, and a machine sitting idle never
//! reaches the code.
//!
//! The answer is written to disk, so the first summon after a restart draws the
//! row from a file rather than from the network. That is the difference between
//! a launcher that knows something and one that has to go and find out before
//! it can say anything.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Origin;

/// How long an answer is good for.
///
/// Six hours, matching [`super::catalog::FRESH_FOR`] because it is the same
/// question about the same data: an extension store does not change enough in
/// an afternoon to be worth asking twice.
///
/// The number that makes this affordable is not this one, it is the absence of
/// a timer. Nothing wakes up to consume it. Somebody who lives in the launcher
/// spends four checks a day and somebody who never opens it spends none.
pub const ASK_AGAIN_AFTER: i64 = 6 * 60 * 60;

/// Past this many store installs, ask the catalogue instead.
///
/// Measured, per extension: 16.8 KB gzipped and roughly 200 ms, taken one at a
/// time because [`super::catalog::fetch`] takes its seven that way on purpose
/// and firing a batch at somebody else's index reads as a scrape.
///
/// So the comparison at n installs is n requests and 17n KB against seven
/// requests and 3.8 MB. Bytes do not break even until about 226, but **time
/// breaks even around 30**, and time is what somebody notices. Twenty five
/// leaves room under that and is still far more extensions than anybody has.
pub const ASK_EACH_UP_TO: usize = 25;

/// The shape of the file on disk. Bumped when [`Behind`] changes.
const FORMAT: u32 = 1;

/// One installed extension with something newer published.
///
/// Everything needed to go and fetch it is here, which is the point: applying
/// an update must not have to open the catalogue to find out where the source
/// lives. That would put 3.8 MB behind a keypress whose whole promise is that
/// it is one keypress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Behind {
    /// The directory it is installed in, which is what the index and the worker
    /// call it. Not always the store's name for it.
    pub extension: String,
    /// The store's name for it, which is what the endpoint is keyed by.
    pub listing: String,
    pub author: String,
    pub folder: String,
    pub title: String,
    /// The revision installed here.
    pub from: String,
    /// The revision the store now publishes.
    pub to: String,
}

/// What is behind, and when that was last established.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Known {
    #[serde(default)]
    pub format: u32,
    /// Seconds since the epoch, or zero for never.
    #[serde(default)]
    pub checked_at: i64,
    #[serde(default)]
    pub behind: Vec<Behind>,
}

impl Known {
    /// Whether an answer is recent enough not to ask again.
    ///
    /// Written the way [`super::catalog::is_fresh`] is, and for the reasons
    /// written there: a file from an older Sill is stale whatever its timestamp
    /// says, and a clock that has gone backwards must read as stale rather than
    /// as fresh for the next six hours.
    pub fn is_fresh(&self, now: i64) -> bool {
        self.format == FORMAT && now >= self.checked_at && now - self.checked_at < ASK_AGAIN_AFTER
    }
}

/// Where the answer is kept between runs.
pub fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("store").join("updates.json")
}

/// Reads the last answer, or an empty one.
///
/// A file that cannot be read is not an error here. The worst it costs is one
/// check that would have been skipped.
pub fn load(data_dir: &Path) -> Known {
    std::fs::read_to_string(cache_path(data_dir))
        .ok()
        .and_then(|text| serde_json::from_str::<Known>(&text).ok())
        .filter(|known| known.format == FORMAT)
        .unwrap_or_default()
}

/// Writes it, making the directory if it is not there.
pub fn save(data_dir: &Path, known: &Known) -> Result<(), String> {
    let path = cache_path(data_dir);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("could not make {}: {err}", parent.display()))?;
    }

    let text = serde_json::to_string_pretty(known)
        .map_err(|err| format!("could not write what is out of date: {err}"))?;

    std::fs::write(&path, format!("{text}\n"))
        .map_err(|err| format!("could not write {}: {err}", path.display()))
}

/// Where to ask about one extension, and under what name.
///
/// **The author is the whole difficulty.** The endpoint is keyed by handle and
/// slug, and until [`Origin::author`] existed only the slug was recorded, so an
/// extension installed by an older Sill cannot be addressed from its origin
/// alone. The catalogue knows, when there is one on disk, so it is the fallback
/// rather than the first choice: a handle read out of a catalogue fetched today
/// is a guess about what it was when this was installed, and the origin is a
/// record of it.
pub fn addressed(
    directory: &str,
    origin: &Origin,
    from_catalog: impl Fn(&str) -> Option<(String, String)>,
) -> Option<(String, String, String)> {
    let listing = if origin.listing.is_empty() {
        directory.to_string()
    } else {
        origin.listing.clone()
    };

    if !origin.author.is_empty() {
        return Some((listing, origin.author.clone(), origin.path.clone()));
    }

    let (author, folder) = from_catalog(&listing)?;
    Some((listing, author, folder))
}

/// One extension this can ask the store about.
#[derive(Debug, Clone, PartialEq)]
pub struct Askable {
    /// The directory it is in, which is the name the index and worker use.
    pub directory: String,
    /// The store's name for it, which the endpoint is keyed by.
    pub listing: String,
    pub author: String,
    pub folder: String,
    pub origin: Origin,
}

/// Everything installed from the store, with what is needed to ask about it.
///
/// Takes [`super::installed`] rather than [`super::pins`], because it needs the
/// directory and `pins` is keyed by slug. Folder installs are skipped, and so
/// is anything whose author cannot be resolved. Both are the same kind of
/// answer: there is no question that can be asked, so no request is made,
/// rather than a request made in hope.
pub fn askable(
    installed: &[(String, Origin)],
    from_catalog: impl Fn(&str) -> Option<(String, String)>,
) -> Vec<Askable> {
    installed
        .iter()
        .filter(|(_, origin)| origin.source == "store" && !origin.revision.is_empty())
        .filter_map(|(directory, origin)| {
            let (listing, author, folder) = addressed(directory, origin, &from_catalog)?;
            Some(Askable {
                directory: directory.clone(),
                listing,
                author,
                folder,
                origin: origin.clone(),
            })
        })
        .collect()
}

/// What one extension's published revision comparison produced.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    /// The store publishes this, and it is the same one installed.
    Current,
    /// The store publishes something else.
    Newer(String),
    /// The store has no listing under that name any more.
    ///
    /// A withdrawn extension, or a handle that has changed since it was
    /// installed. Either way there is nothing to update to, and saying so is
    /// better than reporting a failed check every six hours forever.
    Unlisted,
}

/// Compares one published revision against what is installed.
///
/// Split out from the fetch so the decision is testable without a socket.
pub fn compare(origin: &Origin, published: Option<&str>) -> Asked {
    let Some(published) = published else {
        return Asked::Unlisted;
    };

    if published.is_empty() {
        return Asked::Unlisted;
    }

    if origin.outdated_against(published) {
        Asked::Newer(published.to_string())
    } else {
        Asked::Current
    }
}

/// The capabilities a new version reaches that were never agreed to.
///
/// **This is what makes one press honest.** An update is the same install path
/// at a newer commit, and the capability screen exists because a version can
/// gain the ability to run programs that somebody would otherwise have waved
/// through. Comparing what the new source reaches against what
/// [`Origin::capabilities`] recorded separates the two cases: a bug fix that
/// asks for nothing new has nothing to show anybody, and a version that grew
/// its reach has exactly one thing worth showing, which is the part that grew.
///
/// Order is the new version's, so the list reads the way the screen does.
///
/// An origin written before capabilities were recorded has an empty list, so
/// **everything reads as new**. That is the honest answer rather than an
/// awkward one: nobody agreed to anything, because nothing asked.
pub fn newly_asks(granted: &[String], reaches: &[String]) -> Vec<String> {
    reaches
        .iter()
        .filter(|id| !granted.iter().any(|had| had == *id))
        .cloned()
        .collect()
}

// ----------------------------------------------------------------- fetching

/// The listing for one extension.
const LISTING: &str = "https://backend.raycast.com/api/v1/extensions";

/// The one field of it this reads.
#[derive(Deserialize)]
struct RawListing {
    #[serde(default)]
    commit_sha: Option<String>,
}

/// Asks the store what it publishes for one extension.
///
/// A 404 is [`Asked::Unlisted`] rather than an error, because it is an answer:
/// the store does not have this under that name. Anything else is an error,
/// because a 500 or a dropped connection is a question that was not answered
/// and treating it as "nothing to update" would hide a real update behind a bad
/// afternoon on somebody's server.
pub async fn published(
    client: &reqwest::Client,
    author: &str,
    listing: &str,
) -> Result<Option<String>, String> {
    let response = client
        .get(format!("{LISTING}/{author}/{listing}"))
        .header(reqwest::header::USER_AGENT, super::source::USER_AGENT)
        .send()
        .await
        .map_err(|err| format!("could not reach the extension store: {err}"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }

    if !response.status().is_success() {
        return Err(format!(
            "the extension store answered {} when asked about {listing}",
            response.status()
        ));
    }

    let raw: RawListing = response
        .json()
        .await
        .map_err(|err| format!("the store's answer about {listing} could not be read: {err}"))?;

    Ok(raw.commit_sha.filter(|it| !it.is_empty()))
}

// ------------------------------------------------------------------ holding

/// What the launcher is doing about updates, if anything.
///
/// A tagged enum rather than a set of flags, for the reason
/// [`crate::update::Progress`] is one: the states are exclusive, and a struct
/// of booleans is how a surface ends up claiming to be applying something it
/// has already applied. `serde` writes it as a discriminated union so the
/// TypeScript side cannot forget a case.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Doing {
    /// Nothing. The row, if there is one, is only reporting.
    #[default]
    Nothing,
    /// Asking the store. Not shown: a check that finds nothing must leave no
    /// trace, and one that finds something is drawn as what it found.
    Checking,
    /// Applying, with the one being applied now.
    Applying {
        title: String,
        done: usize,
        total: usize,
    },
}

/// Everything a window needs to draw the row, in one answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Standing {
    pub behind: Vec<Behind>,
    pub doing: Doing,
    /// Titles of extensions an update could not be applied to on its own,
    /// because the new version reaches something nobody agreed to.
    ///
    /// Kept apart from `behind` rather than flagged inside it, because they are
    /// two different sentences: one is "this can be done now" and the other is
    /// "this needs you to look at something".
    pub asking: Vec<String>,
    /// What failed, and why.
    ///
    /// **The reason travels.** It used to be a list of titles, with the reason
    /// going to the log and nowhere else, so the launcher said "Hacker News
    /// could not be updated" and the one sentence that said what to do about it
    /// was in a file nobody was looking at. A failure somebody cannot act on is
    /// barely worth reporting.
    pub failed: Vec<Failure>,
    pub checked_at: i64,
}

/// One update that did not work, and the reason it gives.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub title: String,
    /// The error as the install path worded it. Already a sentence somebody can
    /// act on: the npm one names the Node it looked beside and what to install.
    pub why: String,
}

/// The one place the answer lives.
///
/// A managed service rather than a `static`, which is what rule 2 refuses, and
/// the same shape [`crate::update::Updates`] next door already settled on.
///
/// At rest this is a `Mutex` around a small struct and nothing else. There is
/// no timer, no task and no thread: see the module header for why the summon is
/// the substitute for an interval.
#[derive(Default)]
pub struct ExtensionUpdates {
    inner: std::sync::Mutex<Held>,
}

#[derive(Default)]
struct Held {
    known: Known,
    /// Whether the file on disk has been read yet.
    ///
    /// Read once, lazily, rather than at startup. A machine that never summons
    /// the launcher never opens this file, and the first summon pays a read of
    /// a few hundred bytes to draw a row it would otherwise need the network
    /// for.
    loaded: bool,
    doing: Doing,
    asking: Vec<String>,
    failed: Vec<Failure>,
}

impl ExtensionUpdates {
    /// The current standing, reading the file on the first call.
    pub fn read(&self, data_dir: &Path) -> Standing {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Self::ensure_loaded(&mut held, data_dir);

        Standing {
            behind: held.known.behind.clone(),
            doing: held.doing.clone(),
            asking: held.asking.clone(),
            failed: held.failed.clone(),
            checked_at: held.known.checked_at,
        }
    }

    fn ensure_loaded(held: &mut Held, data_dir: &Path) {
        if !held.loaded {
            held.known = load(data_dir);
            held.loaded = true;
        }
    }

    /// Whether asking the store now is worth the requests.
    ///
    /// False while something is already in flight, because a second check would
    /// replace the answer the first is still building, and false inside the
    /// throttle unless somebody asked for it by name.
    pub fn worth_asking(&self, data_dir: &Path, now: i64, force: bool) -> bool {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Self::ensure_loaded(&mut held, data_dir);

        if !matches!(held.doing, Doing::Nothing) {
            return false;
        }

        force || !held.known.is_fresh(now)
    }

    /// Says what is happening, and whether that is news worth an event.
    pub fn set_doing(&self, doing: Doing) -> bool {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if held.doing == doing {
            return false;
        }
        held.doing = doing;
        true
    }

    /// Records an answer and writes it down, saying whether anything changed.
    ///
    /// The timestamp moves whether or not the list did, because the point of it
    /// is when the question was last asked. The event is only worth sending
    /// when the list changed, which is the rule that stops a check finding
    /// nothing from waking two windows every six hours.
    pub fn record(&self, data_dir: &Path, behind: Vec<Behind>, now: i64) -> bool {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());

        let changed = held.known.behind != behind;
        held.known = Known {
            format: FORMAT,
            checked_at: now,
            behind,
        };
        held.loaded = true;

        if let Err(err) = save(data_dir, &held.known) {
            crate::say!("could not write what is out of date: {err}");
        }

        changed
    }

    /// Takes one extension out of the list, because it is now current.
    pub fn applied(&self, data_dir: &Path, extension: &str) {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        held.known.behind.retain(|it| it.extension != extension);

        if let Err(err) = save(data_dir, &held.known) {
            crate::say!("could not write what is out of date: {err}");
        }
    }

    /// Clears what the last batch reported, so a new one starts from nothing.
    pub fn start_batch(&self) {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        held.asking.clear();
        held.failed.clear();
    }

    /// Records that one needs somebody to look at it.
    pub fn needs_a_look(&self, title: &str) {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !held.asking.iter().any(|it| it == title) {
            held.asking.push(title.to_string());
        }
    }

    /// Records that one did not work, and what it said.
    pub fn did_not_work(&self, title: &str, why: &str) {
        let mut held = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !held.failed.iter().any(|it| it.title == title) {
            held.failed.push(Failure {
                title: title.to_string(),
                why: why.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(revision: &str) -> Origin {
        Origin::store("demo", "extensions/demo", "someone", revision, vec![], 0)
    }

    #[test]
    fn a_published_revision_that_differs_is_something_newer() {
        assert_eq!(
            compare(&origin("old"), Some("new")),
            Asked::Newer("new".to_string())
        );
    }

    #[test]
    fn the_same_revision_is_current() {
        assert_eq!(compare(&origin("same"), Some("same")), Asked::Current);
    }

    /// A withdrawn extension must not read as one with an update.
    #[test]
    fn nothing_published_under_that_name_is_unlisted_rather_than_newer() {
        assert_eq!(compare(&origin("old"), None), Asked::Unlisted);
        assert_eq!(compare(&origin("old"), Some("")), Asked::Unlisted);
    }

    /// The guard that keeps an update badge off somebody's own working copy.
    #[test]
    fn a_folder_install_is_never_behind() {
        let folder = Origin::folder(Path::new("C:/work/demo"), 0);
        assert_eq!(compare(&folder, Some("anything")), Asked::Current);
    }

    #[test]
    fn a_version_that_reaches_nothing_new_asks_for_nothing() {
        let granted = vec!["clipboard".to_string(), "network".to_string()];
        let reaches = vec!["clipboard".to_string()];
        assert!(newly_asks(&granted, &reaches).is_empty());
    }

    #[test]
    fn a_version_that_gained_one_names_only_the_one() {
        let granted = vec!["clipboard".to_string()];
        let reaches = vec!["clipboard".to_string(), "processes".to_string()];
        assert_eq!(newly_asks(&granted, &reaches), vec!["processes".to_string()]);
    }

    /// Installed before capabilities were written down: nobody agreed to
    /// anything, so nothing may be applied without asking.
    #[test]
    fn an_origin_from_before_capabilities_treats_everything_as_new() {
        let reaches = vec!["clipboard".to_string(), "network".to_string()];
        assert_eq!(newly_asks(&[], &reaches), reaches);
    }

    #[test]
    fn the_recorded_author_is_used_rather_than_the_catalogue() {
        let mut it = origin("sha");
        it.author = "recorded".to_string();

        let addressed = addressed(&it.listing.clone(), &it, |_| {
            Some(("guessed".to_string(), "extensions/guessed".to_string()))
        });

        assert_eq!(addressed.unwrap().1, "recorded");
    }

    /// The fallback that keeps extensions installed before the field existed
    /// from being silently unanswerable.
    #[test]
    fn an_origin_with_no_author_falls_back_to_the_catalogue() {
        let mut it = origin("sha");
        it.author = String::new();

        let addressed = addressed("demo", &it, |name| {
            (name == "demo").then(|| ("found".to_string(), "extensions/demo".to_string()))
        });

        assert_eq!(addressed.unwrap().1, "found");
    }

    /// Neither source knows, so no request is made at all.
    #[test]
    fn an_extension_no_author_can_be_found_for_is_not_asked_about() {
        let mut it = origin("sha");
        it.author = String::new();

        assert!(addressed("demo", &it, |_| None).is_none());
    }

    #[test]
    fn a_folder_install_is_never_asked_about() {
        let installed = vec![(
            "mine".to_string(),
            Origin::folder(Path::new("C:/work/mine"), 0),
        )];

        assert!(askable(&installed, |_| Some((
            "someone".to_string(),
            "extensions/mine".to_string()
        )))
        .is_empty());
    }

    /// The slug and the directory are different strings for some extensions,
    /// and the two names are used for different things. Losing the directory
    /// would update the index under a name nothing else answers to.
    #[test]
    fn both_names_survive_when_the_slug_and_the_directory_differ() {
        let mut it = origin("sha");
        it.listing = "translate".to_string();

        let asked = askable(
            &[("google-translate".to_string(), it)],
            |_| None,
        );

        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].directory, "google-translate");
        assert_eq!(asked[0].listing, "translate");
    }

    #[test]
    fn an_answer_from_an_older_sill_is_stale_however_new_it_is() {
        let known = Known {
            format: FORMAT - 1,
            checked_at: 1_000,
            behind: Vec::new(),
        };
        assert!(!known.is_fresh(1_001));
    }

    #[test]
    fn a_clock_that_went_backwards_reads_as_stale() {
        let known = Known {
            format: FORMAT,
            checked_at: 10_000,
            behind: Vec::new(),
        };
        assert!(!known.is_fresh(9_000));
    }

    #[test]
    fn an_answer_inside_the_window_is_fresh_and_outside_it_is_not() {
        let known = Known {
            format: FORMAT,
            checked_at: 1_000,
            behind: Vec::new(),
        };
        assert!(known.is_fresh(1_000 + ASK_AGAIN_AFTER - 1));
        assert!(!known.is_fresh(1_000 + ASK_AGAIN_AFTER));
    }
}
