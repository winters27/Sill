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
//! There is no background refresh. The catalogue is fetched when the store is
//! opened and the copy on disk is stale, and at no other time.
//!
//! It is held while the store is in use and for [`IDLE_TIMEOUT`] afterwards,
//! then dropped. Dropping it the instant the view closed was the first version
//! and it was wrong in the way people actually use a launcher: leaving the
//! store and coming back ten seconds later paid to read and parse a megabyte
//! and a half of JSON again, which is precisely what "it loads everything from
//! scratch every time" feels like. Measured, that parse is **45 ms**.
//!
//! The timer only exists while something is held, so a machine that never
//! opens the store never runs it, and a launcher left alone overnight is
//! holding nothing. That is the same bargain
//! [`crate::host::HOST_IDLE_TIMEOUT`] already makes with the Node process.
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
    /**
    Not from Raycast's index: this one is only here because it is installed.

    Two extensions have this. One built from a folder on this machine, which
    Raycast has never heard of and never will. And one installed from the store
    that has since been withdrawn from it.

    **Both were invisible.** Browsing ran over the catalogue and nothing else,
    so an extension the index does not carry did not appear under Installed,
    could not be found by typing its name, and could not be removed from the
    one screen whose job is removing extensions. It was still in the launcher
    the whole time.

    Defaulted rather than required, so a catalogue cached by an earlier build
    still reads back: everything in that file came from the index, which is
    exactly what `false` means.
    */
    #[serde(default)]
    pub native: bool,
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
        // It is installed and Sill runs it, which is stronger evidence than
        // anything a manifest could have declared. Reading the field would
        // hide half of these behind the compatibility switch and label the
        // rest as unrunnable, on this machine, where they run.
        if self.native {
            return None;
        }

        if !self.declares_windows() {
            return Some("Does not say it runs on Windows".to_string());
        }
        if self.nothing_runnable() {
            return Some("Only has menu bar commands, which Sill has nowhere to put".to_string());
        }
        None
    }

    /// Where a person can read this exact revision for themselves.
    ///
    /// Empty when there is nowhere to send them. An extension built from a
    /// folder on this machine has no folder in Raycast's repository, and an
    /// address assembled from the parts anyway is a link to a 404 presented as
    /// the source of something they have installed.
    pub fn source_url(&self) -> String {
        if self.folder.is_empty() {
            return String::new();
        }

        format!(
            "https://github.com/{}/tree/{}/{}",
            source::REPO,
            self.revision,
            self.folder
        )
    }

    /// A listing for something that is installed here and nowhere in the index.
    ///
    /// Built from what the install itself recorded, which is all there is:
    /// there is no catalogue entry to read a description, an author, a
    /// category or a download count from, and inventing any of them would be
    /// putting made-up facts on a row beside real ones.
    pub fn of_installed(
        name: &str,
        title: &str,
        revision: &str,
        commands: Vec<ListedCommand>,
    ) -> Self {
        Self {
            name: name.to_string(),
            // No folder in a repository this was never in. `source_url` reads
            // that as "nowhere to link to" rather than assembling an address.
            folder: String::new(),
            title: title.to_string(),
            description: String::new(),
            author: String::new(),
            categories: Vec::new(),
            // Not "it does not run on Windows". It is running on this one.
            platforms: vec!["Windows".to_string()],
            revision: revision.to_string(),
            downloads: 0,
            icon: String::new(),
            commands,
            native: true,
        }
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
    /// The capability ids somebody agreed to when installing this.
    ///
    /// **What was shown, not what a later scan would find.** The screen that
    /// asked is the only thing entitled to decide what was granted, so the
    /// answer is written down at the moment it is given rather than derived
    /// again afterwards from source that could have been rebuilt since.
    ///
    /// It is also the record: this is what somebody said yes to, readable in
    /// settings long after they have forgotten.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Which Sill extension API the copy that installed this promised.
    ///
    /// Zero for everything installed before the number existed, which is the
    /// honest reading: nobody wrote it down. It is here rather than derived
    /// because it is a fact about the build that produced these bundles, and a
    /// later Sill that changes what an extension may rely on can then tell an
    /// old install apart from one of its own instead of assuming.
    #[serde(default)]
    pub api: u32,
    pub installed_at: i64,
}

impl Origin {
    pub fn folder(path: &std::path::Path, at: i64) -> Self {
        Self {
            source: "folder".to_string(),
            revision: String::new(),
            path: path.to_string_lossy().into_owned(),
            listing: String::new(),
            // A folder install shows no screen and so grants nothing. It is
            // asked on the card the first time it reaches for something, which
            // is the path that already existed.
            capabilities: Vec::new(),
            api: crate::extension_install::SILL_API_VERSION,
            installed_at: at,
        }
    }

    pub fn store(
        listing: &str,
        folder: &str,
        revision: &str,
        capabilities: Vec<String>,
        at: i64,
    ) -> Self {
        Self {
            source: "store".to_string(),
            revision: revision.to_string(),
            path: folder.to_string(),
            listing: listing.to_string(),
            capabilities,
            api: crate::extension_install::SILL_API_VERSION,
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
            // An install builds into `.<name>.installing` beside its
            // destination, and for the moment that exists it holds a complete
            // origin. Nothing dot-prefixed is an installed extension: the
            // names come from `safe_name`, which allows no leading dot.
            if directory.starts_with('.') {
                return None;
            }
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

/// What one name is installed as, whichever of its two names it is.
///
/// **An extension has two names and nothing makes them agree.** The catalogue
/// is keyed by a store slug, and the directory an extension installs into is
/// named by its own `package.json`. [`Origin::listing`] exists to record the
/// join, and until this function existed nothing consumed it: removing an
/// extension from the store view handed the slug straight to a function that
/// takes a directory name. Where the two differ that removes nothing, reports
/// "was not installed", and leaves the bundles, the index entry and every
/// permission behind. Where they differ *and something else is installed under
/// the slug*, it removes the wrong extension.
///
/// The directory is tried first, because that is what the settings panel and
/// the index already speak and an exact hit is not a guess. The slug is the
/// fallback, and it is looked up by reading the origins rather than by
/// assuming, which is what makes this a fact rather than a coincidence.
///
/// `None` means nothing installed answers to that name, which is a real answer:
/// removing something already gone is the end state somebody asked for.
pub fn installed_as(home: &std::path::Path, name: &str) -> Option<String> {
    if home.join(name).is_dir() {
        return Some(name.to_string());
    }

    let entries = std::fs::read_dir(home).ok()?;

    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        // An install builds into `.<name>.installing` beside its destination
        // and that directory holds a complete origin while it exists, so it
        // answers to the slug too. Removing it would take out a build in
        // progress. Same reasoning as `pins`.
        .filter(|directory| !directory.starts_with('.'))
        .find(|directory| origin_of(home, directory).is_some_and(|origin| origin.listing == name))
}

/// Writes one, making the directory if it is not there yet.
pub fn write_origin(
    home: &std::path::Path,
    extension: &str,
    origin: &Origin,
) -> Result<(), String> {
    write_origin_into(&home.join(extension), origin)
}

/// The same, into a directory that is not named after the extension.
///
/// An install builds into `.<name>.installing` and renames it into place, so
/// the origin has to be written before the directory has its final name. It is
/// written before the swap on purpose: what lands is then complete, and an
/// extension in the index always has something saying where it came from.
pub fn write_origin_into(dir: &std::path::Path, origin: &Origin) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("could not make {}: {err}", dir.display()))?;

    let text = serde_json::to_string_pretty(origin)
        .map_err(|err| format!("could not describe the install: {err}"))?;

    std::fs::write(dir.join(ORIGIN_FILE), format!("{text}\n"))
        .map_err(|err| format!("could not record where this came from: {err}"))
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
    /// Empty when there is nowhere to send somebody to read the source.
    pub source_url: String,
    /// Here because it is installed rather than because the index carries it.
    ///
    /// The window needs this to stop saying "0 installs" about an extension
    /// nobody could have installed from a store it is not in.
    pub native: bool,
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
    native: &[Listing],
    installed: impl Fn(&str) -> Option<Origin>,
    query: &Query,
    fetched_at: i64,
) -> Browse {
    // Lowered once for the whole browse rather than per listing, which is the
    // same reason the launcher prepares its needle once: doing it per candidate
    // is three allocations multiplied by the size of the catalogue.
    let lowered = query.text.trim().to_lowercase();
    let needle: Vec<char> = lowered.chars().collect();

    let mut categories: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for listing in listings {
        for category in &listing.categories {
            *categories.entry(category.as_str()).or_default() += 1;
        }
    }

    let mut updates = 0;
    let mut hidden = 0;
    let mut scored: Vec<(
        crate::registry::MatchClass,
        u64,
        &Listing,
        Option<Installed>,
    )> = Vec::new();

    /*
     * The catalogue, and then whatever is installed that it does not carry.
     *
     * Chained rather than handled separately, so an extension built from a
     * folder is searched, ranked, narrowed by the Installed tab and drawn by
     * exactly the same code as everything else. A second pass for these would
     * be a second store, and the two would agree about matching until the day
     * one of them learned something.
     *
     * They are not counted into the category list above, because they carry no
     * categories: a folder install has no catalogue entry to take them from,
     * and putting them under a category nobody assigned would be an invention.
     */
    for listing in listings.iter().chain(native) {
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
            match best_match(&needle, &lowered, listing) {
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
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.cmp(&a.1))
            .then(a.2.name.cmp(&b.2.name))
    });

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
            native: listing.native,
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

/// Whether `haystack` contains `lowered`, ignoring case, without allocating.
///
/// `match_name` builds two character vectors every time it is called, which is
/// the right trade for a title and the wrong one for a paragraph run over two
/// thousand listings on every keystroke. This allocates nothing.
///
/// `lowered` is already lowercase, so only the haystack needs folding, and
/// only its ASCII needs it: a description is prose and the queries people type
/// into a store are words.
fn contains_fold(haystack: &str, lowered: &str) -> bool {
    let (hay, needle) = (haystack.as_bytes(), lowered.as_bytes());

    needle.len() <= hay.len()
        && hay
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// The best class this listing matches the query on.
///
/// **What it is called first, and only then what it says about itself.** The
/// title because it is what people type, the slug because `google-translate`
/// is how somebody who has seen the repository would look for it, and the
/// author because people search for a maker. Those three get the real matcher,
/// the one that understands word starts and initials.
///
/// The description is different in two ways, and both were measured rather
/// than assumed.
///
/// **It is only consulted when nothing it is called matched.** Measured on the
/// real catalogue of 2,183 listings: matching every description on every
/// keystroke cost **63 ms for the query `"e"`** against a 60 ms budget.
///
/// **And it is a plain substring test rather than a fuzzy one.** Falling back
/// to the fuzzy matcher for the misses was worse than the problem: a phrase
/// that matches few names makes almost every listing reach its description,
/// and that measured **99 ms**. It was also the wrong search. A short query is
/// a subsequence of nearly any sentence, so fuzzy matching prose invents
/// matches, and `match_name` judges the text it is handed, so a description
/// that happened to equal the query came back as `ExactTitle` and outranked a
/// listing whose actual **name** contained the word.
///
/// So a description hit is [`Elsewhere`](crate::registry::MatchClass::Elsewhere),
/// which is what it is: found, somewhere that is not the name.
fn best_match(
    needle: &[char],
    lowered: &str,
    listing: &Listing,
) -> Option<crate::registry::MatchClass> {
    let named = [
        listing.title.as_str(),
        listing.name.as_str(),
        listing.author.as_str(),
    ]
    .into_iter()
    .filter_map(|text| crate::registry::match_name(needle, text).map(|(class, _)| class))
    .min();

    if named.is_some() {
        return named;
    }

    contains_fold(&listing.description, lowered).then_some(crate::registry::MatchClass::Elsewhere)
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
/// How long the catalogue stays held after the store is closed.
///
/// The same bargain [`crate::host::HOST_IDLE_TIMEOUT`] makes with the Node
/// process, and for the same reason: dropping it the instant the view closes
/// is correct at rest and wrong in the minute somebody spends opening the
/// store, glancing at something else, and opening it again. Reading a
/// megabyte and a half of JSON back off disk for that is work nobody asked
/// for, and it is exactly what "loads from scratch every time" feels like.
///
/// Five minutes, then it goes. The timer only exists while something is held,
/// so a machine that never opens the store never runs it.
pub const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// How often the timer looks.
pub const IDLE_CHECK: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Default)]
pub struct StoreState {
    /// **An `Arc`, and that is the whole point of this type.**
    ///
    /// The window asks for a screen on every keystroke and every one of those
    /// reaches for this. Holding the catalogue by value meant `held()` deep
    /// copied 2,183 listings and roughly fifty thousand strings **per
    /// letter typed**, which is the single largest thing the store was doing
    /// and none of it was work anybody wanted. Sharing a pointer makes it a
    /// refcount bump. There is a budget that fails if it goes back.
    inner: std::sync::Mutex<Option<std::sync::Arc<catalog::Catalog>>>,
    /// When it was last reached for, which is what the timer measures.
    last_used: std::sync::Mutex<Option<std::time::Instant>>,
}

impl StoreState {
    /// What is held, if anything.
    ///
    /// Touches the clock, so a store somebody is using stays warm and a store
    /// nobody has opened for five minutes does not.
    pub fn held(&self) -> Option<std::sync::Arc<catalog::Catalog>> {
        let held = self.inner.lock().ok()?.clone();
        if held.is_some() {
            self.touch();
        }
        held
    }

    pub fn hold(&self, catalog: std::sync::Arc<catalog::Catalog>) {
        if let Ok(mut held) = self.inner.lock() {
            *held = Some(catalog);
        }
        self.touch();
    }

    fn touch(&self) {
        if let Ok(mut at) = self.last_used.lock() {
            *at = Some(std::time::Instant::now());
        }
    }

    /// How long since anything reached for it, if anything is held.
    pub fn idle_for(&self) -> Option<std::time::Duration> {
        let at = (*self.last_used.lock().ok()?)?;
        self.inner.lock().ok()?.as_ref()?;
        Some(at.elapsed())
    }

    /// Lets go of it. Returns whether there was anything to let go of.
    pub fn forget(&self) -> bool {
        let dropped = self
            .inner
            .lock()
            .ok()
            .map(|mut held| held.take().is_some())
            .unwrap_or(false);

        if dropped {
            if let Ok(mut at) = self.last_used.lock() {
                *at = None;
            }
        }

        dropped
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
            native: false,
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
            &[],
            nothing,
            &Query {
                hide_blocked: true,
                ..Default::default()
            },
            0,
        );
        assert!(out.rows.is_empty());
        assert_eq!(out.hidden, 1, "and the switch can say how many");

        let shown = browse(&[quiet], &[], nothing, &Query::default(), 0);
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

    /// An extension Raycast's index does not carry is still on this machine.
    ///
    /// Built from a folder, or installed from the store and withdrawn from it
    /// since. Browsing ran over the catalogue and nothing else, so it was
    /// **absent from the Installed tab of the screen whose job is managing
    /// installed extensions**, unfindable by name, and unremovable there, while
    /// running perfectly well in the launcher.
    #[test]
    fn something_installed_here_appears_even_though_the_index_has_never_heard_of_it() {
        let mine = Listing::of_installed(
            "my-notes",
            "My Notes",
            "",
            vec![ListedCommand {
                name: "open".to_string(),
                title: "Open Notes".to_string(),
                description: String::new(),
                mode: "view".to_string(),
            }],
        );

        let here = |name: &str| {
            (name == "my-notes").then(|| Origin::folder(std::path::Path::new(r"C:\notes"), 0))
        };

        let out = browse(
            &[listing("other", "Other", 5)],
            std::slice::from_ref(&mine),
            here,
            &Query {
                installed_only: true,
                ..Default::default()
            },
            0,
        );

        assert_eq!(
            out.rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["my-notes"],
            "an installed extension the catalogue does not carry was left out of Installed"
        );
        assert!(out.rows[0].native, "the window has no way to tell it apart");
        assert!(
            out.rows[0].installed.is_some(),
            "so nothing there would offer to remove it"
        );
    }

    /// It is not held back by the compatibility switch either.
    ///
    /// A folder install declares whatever its author declared, which for
    /// something written before Raycast shipped for Windows is nothing. Reading
    /// that field would hide an extension that is installed and running, on
    /// this machine, behind a switch about whether it runs on this machine.
    #[test]
    fn one_that_is_already_running_here_is_not_hidden_as_incompatible() {
        let mine = Listing::of_installed("my-notes", "My Notes", "", Vec::new());

        assert!(mine.blocked().is_none());
        assert_eq!(
            mine.source_url(),
            "",
            "an address assembled for a repository it was never in is a link to a 404"
        );

        let out = browse(
            &[],
            std::slice::from_ref(&mine),
            |_| None,
            &Query {
                hide_blocked: true,
                ..Default::default()
            },
            0,
        );

        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.hidden, 0);
    }

    #[test]
    fn an_empty_query_orders_by_how_many_people_have_it() {
        let listings = vec![
            listing("few", "Few", 10),
            listing("many", "Many", 9_000),
            listing("some", "Some", 500),
        ];

        let out = browse(&listings, &[], nothing, &Query::default(), 0);
        let order: Vec<&str> = out.rows.iter().map(|r| r.name.as_str()).collect();

        assert_eq!(order, ["many", "some", "few"]);
        assert_eq!(out.total, 3);
        assert_eq!(out.matched, 3);
    }

    #[test]
    fn categories_are_counted_from_the_catalogue_rather_than_written_down() {
        let mut listings = vec![listing("a", "A", 1), listing("b", "B", 1)];
        listings[1].categories = vec!["Media".to_string(), "Productivity".to_string()];

        let out = browse(&listings, &[], nothing, &Query::default(), 0);

        assert_eq!(
            out.categories,
            vec![
                Category {
                    name: "Media".to_string(),
                    count: 1
                },
                Category {
                    name: "Productivity".to_string(),
                    count: 2
                },
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
            &[],
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
            &[],
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
            (name == "here")
                .then(|| Origin::store("here", "extensions/here", "old-sha", Vec::new(), 0))
        };

        let out = browse(&listings, &[], pinned, &Query::default(), 0);
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

        let out = browse(&listings, &[], pinned, &Query::default(), 0);

        assert!(!out.rows[0].installed.as_ref().unwrap().outdated);
        assert_eq!(out.updates, 0);
    }

    #[test]
    fn updates_only_leaves_everything_that_is_current() {
        let listings = vec![listing("old", "Old", 1), listing("new", "New", 1)];
        let pinned = |name: &str| match name {
            "old" => Some(Origin::store(
                "old",
                "extensions/old",
                "behind",
                Vec::new(),
                0,
            )),
            "new" => Some(Origin::store(
                "new",
                "extensions/new",
                "aaaa",
                Vec::new(),
                0,
            )),
            _ => None,
        };

        let out = browse(
            &listings,
            &[],
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
                &[],
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
            &[],
            nothing,
            &Query {
                text: "nothing like it".to_string(),
                ..Default::default()
            },
            0,
        );
        assert!(out.rows.is_empty());
    }

    /// A word in the description finds it, and never outranks a name.
    #[test]
    fn the_description_is_a_fallback_rather_than_a_field_that_can_win() {
        let mut named = listing("thing", "Thing", 1);
        named.description = "Nothing relevant here".to_string();

        let mut described = listing("other", "Other", 9_000);
        described.description = "Translates between languages".to_string();

        let out = browse(
            &[named, described],
            &[],
            nothing,
            &Query {
                text: "translates".to_string(),
                ..Default::default()
            },
            0,
        );

        assert_eq!(out.rows.len(), 1, "the description is searched");
        assert_eq!(out.rows[0].name, "other");
    }

    /// The ordering bug the clamp exists to stop.
    ///
    /// A description that happens to equal the query used to come back as an
    /// exact match and outrank an extension whose actual name contained the
    /// word, even with a thousandth of the downloads.
    #[test]
    fn a_description_that_equals_the_query_still_loses_to_a_name() {
        let mut by_name = listing("timer", "Timer", 10);
        by_name.description = "Nothing".to_string();

        let mut by_words = listing("other", "Other", 900_000);
        by_words.description = "timer".to_string();

        let out = browse(
            &[by_words, by_name],
            &[],
            nothing,
            &Query {
                text: "timer".to_string(),
                ..Default::default()
            },
            0,
        );

        assert_eq!(
            out.rows.first().map(|r| r.name.as_str()),
            Some("timer"),
            "a name beats a paragraph, whatever the download counts say"
        );
    }

    #[test]
    fn the_description_is_matched_by_substring_and_not_by_subsequence() {
        assert!(contains_fold("Translates between LANGUAGES", "languages"));
        assert!(contains_fold("Exactly", "exact"));

        // A short query is a subsequence of nearly any sentence, which is what
        // made fuzzy matching prose invent matches.
        assert!(!contains_fold("Translates between languages", "tbl"));
        assert!(!contains_fold("short", "a much longer needle"));
    }

    #[test]
    fn the_answer_is_capped_and_still_reports_how_many_matched() {
        let listings: Vec<Listing> = (0..SHOWN + 40)
            .map(|n| listing(&format!("ext{n}"), &format!("Ext {n}"), n as u64))
            .collect();

        let out = browse(&listings, &[], nothing, &Query::default(), 0);

        assert_eq!(out.rows.len(), SHOWN);
        assert_eq!(out.matched, SHOWN + 40);
    }

    /// The store row and the directory are two names for one extension.
    ///
    /// The catalogue calls it `translate`; its `package.json` calls it
    /// `google-translate`, and that is the directory. Removing it from the
    /// store view handed the slug to a function that takes a directory name,
    /// so it removed nothing and said "was not installed" while the bundles,
    /// the index entry and every permission stayed exactly where they were.
    #[test]
    fn a_store_slug_resolves_to_the_directory_the_manifest_named() {
        let dir = tempfile::tempdir().expect("temp dir");
        let home = dir.path();

        let origin = Origin::store("translate", "extensions/google-translate", "sha", vec![], 0);
        write_origin(home, "google-translate", &origin).expect("writes");

        assert_eq!(
            installed_as(home, "translate").as_deref(),
            Some("google-translate"),
            "the slug found nothing, so removing it would have removed nothing",
        );
    }

    /// The name the settings panel and the index already speak.
    #[test]
    fn a_directory_name_answers_for_itself() {
        let dir = tempfile::tempdir().expect("temp dir");
        let home = dir.path();

        write_origin(home, "uuid-generator", &Origin::folder(home, 0)).expect("writes");

        assert_eq!(
            installed_as(home, "uuid-generator").as_deref(),
            Some("uuid-generator"),
        );
    }

    /// An exact directory is never given up for somebody else's slug.
    ///
    /// Both of these answer to `translate`: one is a directory with that name,
    /// the other recorded it as the store row it came from. Resolving to the
    /// second would remove an extension the person never pointed at, which is
    /// worse than removing nothing.
    #[test]
    fn the_directory_wins_over_another_extensions_slug() {
        let dir = tempfile::tempdir().expect("temp dir");
        let home = dir.path();

        write_origin(home, "translate", &Origin::folder(home, 0)).expect("writes");
        write_origin(
            home,
            "google-translate",
            &Origin::store("translate", "extensions/google-translate", "sha", vec![], 0),
        )
        .expect("writes");

        assert_eq!(
            installed_as(home, "translate").as_deref(),
            Some("translate")
        );
    }

    /// A build in progress is not an installed extension.
    #[test]
    fn a_half_built_install_is_not_resolved_to() {
        let dir = tempfile::tempdir().expect("temp dir");
        let home = dir.path();

        write_origin(
            home,
            ".google-translate.installing",
            &Origin::store("translate", "extensions/google-translate", "sha", vec![], 0),
        )
        .expect("writes");

        assert_eq!(installed_as(home, "translate"), None);
    }

    #[test]
    fn nothing_installed_under_that_name_is_an_answer_rather_than_a_guess() {
        let dir = tempfile::tempdir().expect("temp dir");

        assert_eq!(installed_as(dir.path(), "never-installed"), None);
        // The directory does not exist at all, which is a machine with no
        // extensions on it.
        assert_eq!(installed_as(&dir.path().join("gone"), "anything"), None);
    }

    #[test]
    fn an_origin_round_trips_through_its_file_shape() {
        let origin = Origin::store(
            "demo",
            "extensions/demo",
            "sha",
            vec!["clipboard".to_string()],
            1_700_000_000,
        );
        let text = serde_json::to_string(&origin).expect("serialises");
        let back: Origin = serde_json::from_str(&text).expect("parses");

        assert_eq!(back, origin);
    }
}
