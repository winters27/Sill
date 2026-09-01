//! Finding an extension without already knowing where it is.
//!
//! [`crate::extension_install`] can build an extension out of a folder, which
//! covers somebody writing one and somebody who has cloned one. This is the
//! other half: a catalogue to look through, and a way to get the folder onto
//! the machine in the first place.
//!
//! ## Two sources, and why they are two
//!
//! **The catalogue comes from Raycast's own store index.** Nothing else
//! aggregates it. The alternative is 3,234 requests for 3,234 `package.json`
//! files, because the repository has no summary of itself, and a browse
//! surface built on folder names alone is a list of slugs.
//!
//! **The code comes from `github.com/raycast/extensions`**, which is MIT, at
//! the exact commit the catalogue names. Sill never downloads a built bundle
//! from anybody: what lands on the machine is source somebody can read, at a
//! revision that is written down, and it is transpiled here by the same
//! esbuild call a folder install uses.
//!
//! That split is the whole trust story. The index is a convenience and can be
//! wrong or go away, in which case the store says so and installing from a
//! folder still works. The code is fetched from the repository that publishes
//! it under a licence that allows this, pinned, and auditable afterwards.
//!
//! ## Nothing happens unless somebody asks
//!
//! There is no timer here and no background refresh. The catalogue is fetched
//! when the store is opened and the copy on disk is stale, and at no other
//! time. It is held in memory only while the store is open and dropped when it
//! closes, for the reason [`crate::meter`] forgets its previous reading: a
//! browse surface that is not on screen has no business holding two megabytes
//! of somebody else's product listings.
//!
//! ## Raycast ships for two platforms, and so does its store
//!
//! Extensions declare which they support, so the index is two stores in one
//! file. [`catalog`] drops anything that names macOS and not Windows before it
//! reaches disk, and what is left is the 886 that name Windows plus the 1,300
//! that name nothing because they predate the field. The store shows the first
//! group and holds the second behind a switch that says how many there are.
//!
//! ## What is a list here, and what is not
//!
//! Categories, platforms and command modes are **read out of the catalogue**,
//! never written down. A category Raycast adds tomorrow appears tomorrow. The
//! only judgement written into this file is the compatibility filter, and it
//! is written as the exceptions: an extension is offered unless something
//! specific says otherwise, which is the shape this codebase settled on after
//! five separate hand-written membership lists each silently drew nothing for
//! something added later.

pub mod capability;
pub mod catalog;
pub mod install;
pub mod source;

use serde::{Deserialize, Serialize};

use crate::exthost::CommandMode;

/// One extension, as the catalogue describes it.
///
/// A long way short of what the index actually returns. The listing for a
/// single extension carries its author's biography, every past contributor,
/// the full text of every AI tool description and a set of prompt examples,
/// which is why the whole index is 19 MB on the wire and 2 MB once this is
/// done with it. Everything not needed to browse, decide and install is
/// dropped at the point of parsing rather than carried around.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    /// What the store calls it, which is also its id here.
    pub name: String,
    /// Where its source is in the repository, without a trailing slash.
    ///
    /// **Not derivable from `name`**, which is the trap: the extension the
    /// store calls `translate` lives in `extensions/google-translate`, and
    /// `visual-studio-code` lives in
    /// `extensions/visual-studio-code-recent-projects`. Fetching by name would
    /// 404 on those and on every other one that has ever been renamed.
    pub folder: String,
    pub title: String,
    pub description: String,
    /// The author's handle, which is what the store shows.
    pub author: String,
    pub categories: Vec<String>,
    /// The platforms it says it runs on.
    ///
    /// Empty means it does not say. That is not the same as "macOS only": the
    /// field arrived when Raycast shipped a Windows build, so an extension
    /// that never declared one predates the question rather than answering it.
    pub platforms: Vec<String>,
    /// The commit the catalogue publishes, which is what gets installed.
    pub revision: String,
    pub downloads: u64,
    /// Where the extension's icon is, or empty when it has none.
    ///
    /// A URL on Raycast's asset host rather than a file, because the artwork
    /// is theirs and already the right size. It is the one thing the store
    /// draws that is fetched per row rather than per catalogue, which is why
    /// the window loads it lazily: browsing must not mean a request for every
    /// one of eighty rows the moment they exist.
    #[serde(default)]
    pub icon: String,
    pub commands: Vec<ListedCommand>,
}

/// One command an extension contributes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListedCommand {
    pub name: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// `view`, `no-view` or `menu-bar`, as the manifest wrote it.
    pub mode: String,
}

impl ListedCommand {
    /// Whether Sill can actually run this one.
    ///
    /// Asked of [`CommandMode`] rather than answered here, so the store and
    /// the thing that loads a command agree by construction. A mode Raycast
    /// invents next year is unrunnable here until that type learns it, and it
    /// says so in the store instead of installing and then doing nothing.
    pub fn runnable(&self) -> bool {
        CommandMode::from_manifest(&self.mode).is_some()
    }
}

impl Listing {
    /// Whether it says it runs on Windows.
    ///
    /// An extension that names macOS and not Windows never gets this far:
    /// [`catalog`] drops it at the point of parsing, because it belongs to the
    /// other half of a store Raycast ships for two platforms. What reaches
    /// here is what names Windows and what names nothing, and this is what
    /// tells those two apart.
    pub fn declares_windows(&self) -> bool {
        self.platforms.iter().any(|p| p == "Windows")
    }

    /// Whether every command it has is one Sill cannot run.
    ///
    /// True for the twenty-two extensions that are menu bar items and nothing
    /// else. Installing one puts nothing in the list, which reads as the
    /// install having failed.
    pub fn nothing_runnable(&self) -> bool {
        !self.commands.is_empty() && !self.commands.iter().any(ListedCommand::runnable)
    }

    /// Why this is not offered by default, or nothing when it is.
    ///
    /// One sentence, because it is shown on the row. Two reasons, and they are
    /// different strengths of the same idea: the second is a certainty and the
    /// first is a silence. Both are hidden by the one switch, and the row says
    /// which it is rather than making them look alike.
    pub fn blocked(&self) -> Option<String> {
        if !self.declares_windows() {
            return Some("Does not say it runs on Windows".to_string());
        }
        if self.nothing_runnable() {
            return Some("Only has menu bar commands, which Sill has nowhere to put".to_string());
        }
        None
    }

    /// Where a person can read this exact revision for themselves.
    pub fn source_url(&self) -> String {
        format!(
            "https://github.com/{}/tree/{}/{}",
            source::REPO,
            self.revision,
            self.folder
        )
    }
}

// ------------------------------------------------------------------- origin

/// Where an installed extension came from, written beside its bundle.
///
/// Its own small file per extension rather than a second index, because a
/// second index is a list that has to agree with the first one and this
/// codebase has been bitten by that five times. This one cannot disagree: it
/// lives inside the directory it describes and goes when that goes.
///
/// The revision is the point. It is what makes "out of date" a comparison
/// rather than a guess, and it is the same bargain [`crate::tts::piper`]
/// makes with its model repository: pin what was fetched, so what is on the
/// machine tomorrow is what was fetched today.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    /// `store` or `folder`.
    pub source: String,
    /// The commit installed, for a store install. Empty for a folder.
    #[serde(default)]
    pub revision: String,
    /// The repository path for a store install, the local path for a folder.
    #[serde(default)]
    pub path: String,
    /// What the store calls it, which is not always what its manifest does.
    ///
    /// The directory an extension installs into is named by its manifest, and
    /// the catalogue is keyed by its store slug. Those agree today on every
    /// listing checked, and nothing makes them agree. Recording the slug is
    /// what lets the join be a fact rather than a coincidence, and it is what
    /// stops an update badge silently never appearing for the one extension
    /// where they differ. Empty for a folder install, which has no slug.
    #[serde(default)]
    pub listing: String,
    pub installed_at: i64,
}

impl Origin {
    pub fn folder(path: &std::path::Path, at: i64) -> Self {
        Self {
            source: "folder".to_string(),
            revision: String::new(),
            path: path.to_string_lossy().into_owned(),
            listing: String::new(),
            installed_at: at,
        }
    }

    pub fn store(listing: &str, folder: &str, revision: &str, at: i64) -> Self {
        Self {
            source: "store".to_string(),
            revision: revision.to_string(),
            path: folder.to_string(),
            listing: listing.to_string(),
            installed_at: at,
        }
    }

    /// Whether the catalogue now publishes something newer.
    ///
    /// A folder install is never out of date, because nothing here knows what
    /// its folder says now, and claiming otherwise would put an update badge
    /// on somebody's own working copy.
    pub fn outdated_against(&self, published: &str) -> bool {
        self.source == "store" && !self.revision.is_empty() && self.revision != published
    }
}

/// The file inside an installed extension that records where it came from.
pub const ORIGIN_FILE: &str = "origin.json";

/// Where Sill keeps installed extensions.
///
/// One function rather than four spellings of `data_dir/extensions`. The
/// installer writes there, the loader reads there, the store removes from
/// there and the pins live there, and a directory name assembled separately in
/// each of them is the shape that eventually disagrees.
pub fn extensions_home(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("extensions")
}

/// The index inside it, which is what the launcher searches.
pub fn index_file(home: &std::path::Path) -> std::path::PathBuf {
    home.join("index.json")
}

/// Reads the origin of one installed extension, if it has one.
///
/// Absent is an ordinary answer rather than an error: everything installed
/// before this existed has no origin file, and the honest thing to show for
/// one is that nobody knows, not a fabricated folder path.
pub fn origin_of(home: &std::path::Path, extension: &str) -> Option<Origin> {
    let text = std::fs::read_to_string(home.join(extension).join(ORIGIN_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every installed extension's origin, keyed by the name the catalogue uses.
///
/// Read once per browse rather than once per row, because the alternative is
/// three thousand directory probes on every keystroke.
///
/// Keyed by the recorded slug where there is one and by the directory name
/// otherwise. A folder install has no slug and is keyed by its directory,
/// which is how somebody's own working copy of an extension still lines up
/// with the store's row for it.
pub fn pins(home: &std::path::Path) -> std::collections::HashMap<String, Origin> {
    let Ok(entries) = std::fs::read_dir(home) else {
        return std::collections::HashMap::new();
    };

    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let directory = entry.file_name().to_string_lossy().into_owned();
            let origin = origin_of(home, &directory)?;
            let key = if origin.listing.is_empty() {
                directory
            } else {
                origin.listing.clone()
            };
            Some((key, origin))
        })
        .collect()
}

/// Writes one, making the directory if it is not there yet.
pub fn write_origin(
    home: &std::path::Path,
    extension: &str,
    origin: &Origin,
) -> Result<(), String> {
    let dir = home.join(extension);
    std::fs::create_dir_all(&dir).map_err(|err| format!("could not make {}: {err}", dir.display()))?;

    let text = serde_json::to_string_pretty(origin)
        .map_err(|err| format!("could not describe the install: {err}"))?;

    std::fs::write(dir.join(ORIGIN_FILE), format!("{text}\n"))
        .map_err(|err| format!("could not record where {extension} came from: {err}"))
}

// ------------------------------------------------------------------- browse

/// How many rows one browse answers with.
///
/// The launcher caps its own results at 120 and this is a heavier row: a
/// title, a description, an author and a command list each. Eighty is more
/// than anybody scrolls through and small enough that the payload stays under
/// a hundred kilobytes, which matters because this crosses the IPC boundary
/// on every keystroke.
pub const SHOWN: usize = 80;

/// A category as the sidebar offers it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub name: String,
    pub count: usize,
}

/// What is known about an extension that is already here.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub revision: String,
    pub source: String,
    pub outdated: bool,
}

/// One extension, ready to draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub name: String,
    pub folder: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub categories: Vec<String>,
    pub platforms: Vec<String>,
    pub downloads: u64,
    pub revision: String,
    pub icon: String,
    pub commands: Vec<RowCommand>,
    pub installed: Option<Installed>,
    /// Why it will not work here, when it will not.
    pub blocked: Option<String>,
    pub source_url: String,
}

/// A command on a row, with the one thing the listing does not say.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowCommand {
    pub name: String,
    pub title: String,
    pub description: String,
    pub mode: String,
    pub runnable: bool,
}

/// Everything one browse produces.
///
/// One call and one answer, because the alternative is the window asking for
/// rows, then for the category list, then for whether each row is installed,
/// which is the chatter rule 18 exists to stop.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Browse {
    pub rows: Vec<Row>,
    /// Every category the catalogue uses, with how many carry it.
    pub categories: Vec<Category>,
    /// How many matched before the cap.
    pub matched: usize,
    /// How many the catalogue holds at all.
    pub total: usize,
    /// How many the compatibility filter is holding back, so the switch can
    /// say what turning it off would show.
    pub hidden: usize,
    /// Installed extensions the catalogue publishes a newer revision for.
    pub updates: usize,
    /// When the catalogue was fetched, in seconds.
    pub fetched_at: i64,
}

/// What the window is asking for.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    #[serde(default)]
    pub text: String,
    /// A category name straight out of the catalogue, or nothing for all.
    #[serde(default)]
    pub category: Option<String>,
    /// Only what is already here.
    #[serde(default)]
    pub installed_only: bool,
    /// Only what has something newer published.
    #[serde(default)]
    pub updates_only: bool,
    /// Leave out anything [`Listing::blocked`] has an answer for.
    #[serde(default)]
    pub hide_blocked: bool,
}

/// Filters, ranks and caps the catalogue against one query.
///
/// Pure, and takes the pins rather than reading them, so the interesting cases
/// are values in a test instead of directories somebody has to create. This is
/// where the constitution's "search computation lives in Rust" lands for the
/// store: the window sends a query and gets rows it can draw, and never sees
/// the two megabytes this ran over.
pub fn browse(
    listings: &[Listing],
    installed: impl Fn(&str) -> Option<Origin>,
    query: &Query,
    fetched_at: i64,
) -> Browse {
    let needle: Vec<char> = query.text.trim().to_lowercase().chars().collect();

    let mut categories: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for listing in listings {
        for category in &listing.categories {
            *categories.entry(category.as_str()).or_default() += 1;
        }
    }

    let mut updates = 0;
    let mut hidden = 0;
    let mut scored: Vec<(crate::registry::MatchClass, u64, &Listing, Option<Installed>)> =
        Vec::new();

    for listing in listings {
        let origin = installed(&listing.name);
        let state = origin.as_ref().map(|origin| Installed {
            revision: origin.revision.clone(),
            source: origin.source.clone(),
            outdated: origin.outdated_against(&listing.revision),
        });

        if state.as_ref().is_some_and(|it| it.outdated) {
            updates += 1;
        }

        let blocked = listing.blocked();

        // Counted before the other filters, so the switch reports what it is
        // holding back rather than what is left after a category narrowed the
        // list to nine things.
        if blocked.is_some() {
            hidden += 1;
            if query.hide_blocked {
                continue;
            }
        }

        if query.installed_only && state.is_none() {
            continue;
        }
        if query.updates_only && !state.as_ref().is_some_and(|it| it.outdated) {
            continue;
        }
        if let Some(wanted) = &query.category {
            if !listing.categories.iter().any(|it| it == wanted) {
                continue;
            }
        }

        let class = if needle.is_empty() {
            // Nothing typed, so nothing matched. Every row shares a class and
            // the download count does the ordering, which is what a store
            // showing no query should do.
            crate::registry::MatchClass::TitleWord
        } else {
            match best_match(&needle, listing) {
                Some(class) => class,
                None => continue,
            }
        };

        scored.push((class, listing.downloads, listing, state));
    }

    let matched = scored.len();

    // Class first, then how many people have it. Reusing the launcher's own
    // classifier rather than scoring here is rule 22: two answers to "does
    // this text match" drift the first time either learns something.
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.name.cmp(&b.2.name)));

    let rows = scored
        .into_iter()
        .take(SHOWN)
        .map(|(_, _, listing, state)| Row {
            name: listing.name.clone(),
            folder: listing.folder.clone(),
            title: listing.title.clone(),
            description: listing.description.clone(),
            author: listing.author.clone(),
            categories: listing.categories.clone(),
            platforms: listing.platforms.clone(),
            downloads: listing.downloads,
            revision: listing.revision.clone(),
            icon: listing.icon.clone(),
            commands: listing
                .commands
                .iter()
                .map(|command| RowCommand {
                    name: command.name.clone(),
                    title: command.title.clone(),
                    description: command.description.clone(),
                    mode: command.mode.clone(),
                    runnable: command.runnable(),
                })
                .collect(),
            blocked: listing.blocked(),
            source_url: listing.source_url(),
            installed: state,
        })
        .collect();

    Browse {
        rows,
        categories: categories
            .into_iter()
            .map(|(name, count)| Category {
                name: name.to_string(),
                count,
            })
            .collect(),
        matched,
        total: listings.len(),
        hidden,
        updates,
        fetched_at,
    }
}

/// The best class this listing matches the query on, over every field worth
/// searching.
///
/// The title first because it is what people type, then the slug, because
/// `google-translate` is how somebody who has seen the repository would look
/// for it, then the author, then the description. The description is included
/// and is deliberately last: it is the only way to find an extension whose
/// name says nothing, and it is also the field most likely to contain a word
/// by accident.
fn best_match(needle: &[char], listing: &Listing) -> Option<crate::registry::MatchClass> {
    [
        listing.title.as_str(),
        listing.name.as_str(),
        listing.author.as_str(),
        listing.description.as_str(),
    ]
    .into_iter()
    .filter_map(|text| crate::registry::match_name(needle, text).map(|(class, _)| class))
    .min()
}

// -------------------------------------------------------------------- state

/// The catalogue, while somebody is looking at it.
///
/// Held rather than re-read because a keystroke has to filter three thousand
/// listings and reading two megabytes off disk per keystroke is not a
/// keystroke budget. Dropped the moment the store closes, which is the same
/// bargain [`crate::meter::Meter::forget`] makes and for the same reason:
/// there is no version of "at rest, do almost nothing" where a launcher
/// nobody is using holds a product catalogue.
#[derive(Default)]
pub struct StoreState {
    inner: std::sync::Mutex<Option<catalog::Catalog>>,
}

impl StoreState {
    /// What is held, if anything.
    pub fn held(&self) -> Option<catalog::Catalog> {
        self.inner.lock().ok().and_then(|held| held.clone())
    }

    pub fn hold(&self, catalog: catalog::Catalog) {
        if let Ok(mut held) = self.inner.lock() {
            *held = Some(catalog);
        }
    }

    /// Lets go of it, which is what closing the store does.
    pub fn forget(&self) {
        if let Ok(mut held) = self.inner.lock() {
            *held = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(name: &str, title: &str, downloads: u64) -> Listing {
        Listing {
            name: name.to_string(),
            folder: format!("extensions/{name}"),
            title: title.to_string(),
            description: String::new(),
            author: "someone".to_string(),
            categories: vec!["Productivity".to_string()],
            platforms: vec!["macOS".to_string(), "Windows".to_string()],
            revision: "aaaa".to_string(),
            icon: String::new(),
            downloads,
            commands: vec![ListedCommand {
                name: "run".to_string(),
                title: "Run".to_string(),
                description: String::new(),
                mode: "view".to_string(),
            }],
        }
    }

    fn nothing(_: &str) -> Option<Origin> {
        None
    }

    /// The trap that would 404 every renamed extension.
    #[test]
    fn the_folder_is_carried_rather_than_guessed_from_the_name() {
        let mut renamed = listing("translate", "Google Translate", 1);
        renamed.folder = "extensions/google-translate".to_string();

        assert_eq!(
            renamed.source_url(),
            "https://github.com/raycast/extensions/tree/aaaa/extensions/google-translate",
            "the repository path is a fact about the listing, not a spelling of its name"
        );
    }

    /// The default view is the extensions that say Windows. The ones that say
    /// nothing are still there, marked, behind the switch.
    #[test]
    fn saying_nothing_about_platforms_is_held_back_but_not_thrown_away() {
        let mut quiet = listing("old", "Old", 1);
        quiet.platforms = Vec::new();

        assert!(!quiet.declares_windows());
        assert_eq!(
            quiet.blocked().as_deref(),
            Some("Does not say it runs on Windows")
        );

        let out = browse(
            &[quiet.clone()],
            nothing,
            &Query {
                hide_blocked: true,
                ..Default::default()
            },
            0,
        );
        assert!(out.rows.is_empty());
        assert_eq!(out.hidden, 1, "and the switch can say how many");

        let shown = browse(&[quiet], nothing, &Query::default(), 0);
        assert_eq!(shown.rows.len(), 1, "turning it off brings them back");
    }

    /// A mode Sill cannot run is a fact about `CommandMode`, asked rather than
    /// listed here.
    #[test]
    fn a_menu_bar_only_extension_says_so_before_it_is_installed() {
        let mut bar = listing("bar", "Bar", 1);
        bar.commands[0].mode = "menu-bar".to_string();

        assert!(!bar.commands[0].runnable());
        assert!(bar.nothing_runnable());
        assert!(bar.blocked().is_some());

        // One unrunnable command among runnable ones is not a blocker: the
        // rest of the extension still works.
        bar.commands.push(ListedCommand {
            name: "other".to_string(),
            title: "Other".to_string(),
            description: String::new(),
            mode: "no-view".to_string(),
        });
        assert!(!bar.nothing_runnable());
        assert!(bar.blocked().is_none());
    }

    #[test]
    fn an_empty_query_orders_by_how_many_people_have_it() {
        let listings = vec![
            listing("few", "Few", 10),
            listing("many", "Many", 9_000),
            listing("some", "Some", 500),
        ];

        let out = browse(&listings, nothing, &Query::default(), 0);
        let order: Vec<&str> = out.rows.iter().map(|r| r.name.as_str()).collect();

        assert_eq!(order, ["many", "some", "few"]);
        assert_eq!(out.total, 3);
        assert_eq!(out.matched, 3);
    }

    #[test]
    fn categories_are_counted_from_the_catalogue_rather_than_written_down() {
        let mut listings = vec![listing("a", "A", 1), listing("b", "B", 1)];
        listings[1].categories = vec!["Media".to_string(), "Productivity".to_string()];

        let out = browse(&listings, nothing, &Query::default(), 0);

        assert_eq!(
            out.categories,
            vec![
                Category { name: "Media".to_string(), count: 1 },
                Category { name: "Productivity".to_string(), count: 2 },
            ],
            "a category nobody here has ever heard of still appears, because it \
             is read out of the data"
        );
    }

    #[test]
    fn a_category_narrows_the_rows_and_leaves_the_category_list_alone() {
        let mut listings = vec![listing("a", "A", 1), listing("b", "B", 1)];
        listings[1].categories = vec!["Media".to_string()];

        let out = browse(
            &listings,
            nothing,
            &Query {
                category: Some("Media".to_string()),
                ..Default::default()
            },
            0,
        );

        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0].name, "b");
        assert_eq!(out.categories.len(), 2, "the sidebar still offers both");
    }

    /// What is held back is counted before anything else narrows the list, so
    /// the switch offering to show them can say how many there are.
    #[test]
    fn hidden_counts_what_the_filter_holds_back_not_what_survived() {
        let mut listings = vec![listing("ok", "Ok", 1), listing("quiet", "Quiet", 1)];
        listings[1].platforms = Vec::new();

        let out = browse(
            &listings,
            nothing,
            &Query {
                hide_blocked: true,
                category: Some("nothing at all".to_string()),
                ..Default::default()
            },
            0,
        );

        assert_eq!(out.rows.len(), 0, "the category matched nothing");
        assert_eq!(out.hidden, 1, "and one was held back regardless");
    }

    #[test]
    fn an_installed_extension_says_so_and_says_when_it_is_behind() {
        let listings = vec![listing("here", "Here", 1)];
        let pinned = |name: &str| {
            (name == "here").then(|| Origin::store("here", "extensions/here", "old-sha", 0))
        };

        let out = browse(&listings, pinned, &Query::default(), 0);
        let installed = out.rows[0].installed.as_ref().expect("it is installed");

        assert_eq!(installed.revision, "old-sha");
        assert!(installed.outdated, "the catalogue publishes aaaa");
        assert_eq!(out.updates, 1);
    }

    /// Somebody's own working copy must not grow an update badge.
    #[test]
    fn a_folder_install_is_never_out_of_date() {
        let listings = vec![listing("here", "Here", 1)];
        let pinned = |_: &str| Some(Origin::folder(std::path::Path::new("C:/mine"), 0));

        let out = browse(&listings, pinned, &Query::default(), 0);

        assert!(!out.rows[0].installed.as_ref().unwrap().outdated);
        assert_eq!(out.updates, 0);
    }

    #[test]
    fn updates_only_leaves_everything_that_is_current() {
        let listings = vec![listing("old", "Old", 1), listing("new", "New", 1)];
        let pinned = |name: &str| match name {
            "old" => Some(Origin::store("old", "extensions/old", "behind", 0)),
            "new" => Some(Origin::store("new", "extensions/new", "aaaa", 0)),
            _ => None,
        };

        let out = browse(
            &listings,
            pinned,
            &Query {
                updates_only: true,
                ..Default::default()
            },
            0,
        );

        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0].name, "old");
    }

    #[test]
    fn typing_reaches_the_title_the_slug_and_the_author() {
        let mut listings = vec![listing("google-translate", "Google Translate", 1)];
        listings[0].author = "peculiarhandle".to_string();

        for query in ["translate", "google-tr", "peculiar"] {
            let out = browse(
                &listings,
                nothing,
                &Query {
                    text: query.to_string(),
                    ..Default::default()
                },
                0,
            );
            assert_eq!(out.rows.len(), 1, "{query} found nothing");
        }

        let out = browse(
            &listings,
            nothing,
            &Query {
                text: "nothing like it".to_string(),
                ..Default::default()
            },
            0,
        );
        assert!(out.rows.is_empty());
    }

    #[test]
    fn the_answer_is_capped_and_still_reports_how_many_matched() {
        let listings: Vec<Listing> = (0..SHOWN + 40)
            .map(|n| listing(&format!("ext{n}"), &format!("Ext {n}"), n as u64))
            .collect();

        let out = browse(&listings, nothing, &Query::default(), 0);

        assert_eq!(out.rows.len(), SHOWN);
        assert_eq!(out.matched, SHOWN + 40);
    }

    #[test]
    fn an_origin_round_trips_through_its_file_shape() {
        let origin = Origin::store("demo", "extensions/demo", "sha", 1_700_000_000);
        let text = serde_json::to_string(&origin).expect("serialises");
        let back: Origin = serde_json::from_str(&text).expect("parses");

        assert_eq!(back, origin);
    }
}
