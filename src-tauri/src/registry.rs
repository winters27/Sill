//! What the launcher can run, and how it decides what to show first.
//!
//! Two concerns that only make sense together: the set of installed commands,
//! and the ranking that turns a half-typed query into the one the user meant.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One runnable command, as written by `scripts/build-extension.mjs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    /// `<extension>:<command>`, stable across rebuilds and used as the
    /// frecency key.
    pub id: String,
    pub extension: String,
    pub extension_title: String,
    pub command: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub description: String,
    pub mode: String,
    pub entrypoint: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// A file to pull an icon from, when it differs from the launch target.
    ///
    /// An Apps folder entry launches by AppUserModelID but may still have a
    /// real executable behind it, and a packaged app has neither.
    #[serde(default)]
    pub icon: Option<String>,
    /// The settings panel this belongs to, for anything Sill owns.
    ///
    /// Carries the panel's drawn icon out to the launcher, so "Dictate" and
    /// "Dictation Vocabulary" arrive under the same mark they wear in
    /// settings. Set here rather than mapped in the frontend, because the
    /// answer is a fact about the command and would otherwise be maintained
    /// in two places that drift.
    #[serde(default)]
    pub panel: Option<String>,
}

/// A command plus why it placed where it did.
///
/// Internal to ranking. What crosses the IPC boundary is [`SearchResult`],
/// which is a good deal smaller.
#[derive(Debug, Clone)]
pub struct RankedCommand {
    pub command: CommandRecord,
    pub score: i64,
    /// Indices into `title` that matched, so the UI can highlight them.
    pub matched: Vec<usize>,
}

/// One result, as the window receives it.
///
/// Separate from [`CommandRecord`] because the two are answering different
/// questions. The record is what ranking needs, and it carries the fields that
/// make matching work: `description`, `keywords`, and the score itself. **None
/// of those are ever read by the frontend**, and flattening the whole record
/// onto the wire meant sending every one of them for every result.
///
/// This is rule 9's domain/DTO split, applied at the one place it pays for
/// itself: this type is serialised on every keystroke.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub extension: String,
    pub extension_title: String,
    pub command: String,
    pub title: String,
    pub subtitle: String,
    pub mode: String,
    pub entrypoint: String,
    pub icon: Option<String>,
    pub panel: Option<String>,
    /// Indices into `title` that matched, so the UI can highlight them.
    pub matched: Vec<usize>,
}

impl From<RankedCommand> for SearchResult {
    fn from(ranked: RankedCommand) -> Self {
        let RankedCommand {
            command,
            matched,
            // Ranking is finished by the time this conversion happens, and the
            // order results arrive in is the only thing the UI needs to know
            // about it.
            score: _,
        } = ranked;

        Self {
            id: command.id,
            extension: command.extension,
            extension_title: command.extension_title,
            command: command.command,
            title: command.title,
            subtitle: command.subtitle,
            mode: command.mode,
            entrypoint: command.entrypoint,
            icon: command.icon,
            panel: command.panel,
            matched,
        }
    }
}

/// Turns a discovered application into a registry entry.
///
/// An app is just another thing you can launch, so it is expressed as a
/// `CommandRecord` with `mode: "app"` rather than as a parallel type. Search,
/// ranking and frecency then apply to apps and commands identically, with no
/// second code path to keep in step.
pub fn app_record(name: &str, path: &str, icon: Option<String>, kind: &str) -> CommandRecord {
    app_entry(name, path, icon, "app", kind)
}

/// A bare executable found on `%PATH%`.
///
/// Kept distinct from an application because there are an order of magnitude
/// more of them and they are mostly command-line tools. Same launch path, but
/// ranked below real applications so a CLI utility never displaces the app
/// someone was reaching for.
pub fn executable_record(name: &str, path: &str, kind: &str) -> CommandRecord {
    app_entry(name, path, Some(path.to_string()), "exe", kind)
}

fn app_entry(
    name: &str,
    path: &str,
    icon: Option<String>,
    mode: &str,
    kind: &str,
) -> CommandRecord {
    CommandRecord {
        id: format!("app:{path}"),
        extension: "app".to_string(),
        extension_title: kind.to_string(),
        command: name.to_string(),
        title: name.to_string(),
        subtitle: String::new(),
        description: String::new(),
        mode: mode.to_string(),
        entrypoint: path.to_string(),
        keywords: Vec::new(),
        icon,
        // A Windows application, not one of Sill's panels.
        panel: None,
    }
}

/// Reads the previously scanned index.
///
/// Discovery costs a PowerShell round trip and a few thousand filesystem
/// calls, which is a second or so of the launcher being half-populated on
/// every start. The cache makes the full list available immediately; the scan
/// still runs behind it and replaces this with fresh results.
pub fn load_cache(path: &Path) -> Vec<CommandRecord> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the index for the next start.
pub fn save_cache(path: &Path, commands: &[CommandRecord]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string(commands).unwrap_or_else(|_| "[]".into());
    std::fs::write(path, text)
}

/// Where the scanned index is cached, given the app's data directory.
pub fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("index-cache.json")
}

/// Sill's own commands.
///
/// These are the things the launcher can do to itself. They belong in the root
/// list rather than behind a shortcut: a setting nobody can find is a setting
/// nobody has.
pub fn builtins() -> Vec<CommandRecord> {
    vec![
        builtin(
            "settings",
            "general",
            "Sill Settings",
            "Hotkey, appearance, sources and file search",
            &["preferences", "options", "configure", "hotkey"],
        ),
        builtin(
            "reload",
            "advanced",
            "Reload Sill Index",
            "Rescan applications, settings and shortcuts",
            &["refresh", "rescan", "reindex"],
        ),
        builtin(
            "snippets",
            "snippets",
            "Snippets",
            "Saved text, expanded by keyword or pasted from here",
            &["snippet", "template", "expand", "abbreviation", "text"],
        ),
        builtin(
            "quicklinks",
            "quicklinks",
            "Quicklinks",
            "Saved links that take what you type",
            &["link", "url", "bookmark", "search", "open", "web"],
        ),
        builtin(
            "clipboard",
            "clipboard",
            "Clipboard History",
            "Everything you have copied, searchable",
            &["paste", "copy", "history", "recent", "pasteboard"],
        ),
        builtin(
            "dictate",
            "dictation",
            "Dictate",
            "Start a dictation without the hotkey",
            &["voice", "speech", "talk", "transcribe", "microphone", "whisper"],
        ),
        builtin(
            "dictation-history",
            "dictation",
            "Dictation History",
            "Every transcript, newest first",
            &["voice", "speech", "transcripts", "past", "log"],
        ),
        builtin(
            "last-transcription",
            "dictation",
            "Get Last Transcription",
            "Copy the most recent transcript",
            &["voice", "speech", "again", "repeat", "copy"],
        ),
        builtin(
            "vocabulary",
            "dictation",
            "Dictation Vocabulary",
            "Words and names dictation should always get right",
            &["voice", "speech", "terms", "jargon", "names"],
        ),
    ]
}

fn builtin(id: &str, panel: &str, title: &str, subtitle: &str, keywords: &[&str]) -> CommandRecord {
    CommandRecord {
        id: format!("sill:{id}"),
        extension: "sill".to_string(),
        extension_title: "Sill".to_string(),
        command: id.to_string(),
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        description: String::new(),
        mode: "builtin".to_string(),
        entrypoint: id.to_string(),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        icon: None,
        panel: Some(panel.to_string()),
    }
}

/// A quicklink, shaped as a command so the ranker treats it like anything
/// else.
///
/// `needs_argument` rides along in the mode rather than as another field: the
/// launcher has to know before it opens anything whether to ask for a query
/// or go straight there, and that is the only thing it needs to know.
pub fn quicklink_record(
    id: &str,
    name: &str,
    keyword: &str,
    link: &str,
    needs_argument: bool,
) -> CommandRecord {
    CommandRecord {
        id: format!("quicklink:{id}"),
        extension: "quicklinks".to_string(),
        extension_title: "Quicklink".to_string(),
        command: id.to_string(),
        title: name.to_string(),
        // The target is the useful subtitle: two links called "Search" are
        // told apart by where they go, not by their names.
        subtitle: link.to_string(),
        description: String::new(),
        mode: if needs_argument {
            "quicklink-arg".to_string()
        } else {
            "quicklink".to_string()
        },
        entrypoint: id.to_string(),
        keywords: if keyword.is_empty() {
            Vec::new()
        } else {
            vec![keyword.to_string()]
        },
        icon: None,
        panel: Some("quicklinks".to_string()),
    }
}

/// A snippet, shaped as a command so the ranker treats it like anything else.
pub fn snippet_record(id: &str, name: &str, keyword: &str, preview: &str) -> CommandRecord {
    CommandRecord {
        id: format!("snippet:{id}"),
        extension: "snippets".to_string(),
        extension_title: "Snippets".to_string(),
        command: id.to_string(),
        title: name.to_string(),
        // The keyword is worth showing: it is how the snippet is used when
        // the launcher is not open, and there is nowhere else to learn it.
        subtitle: if keyword.is_empty() {
            preview.to_string()
        } else {
            keyword.to_string()
        },
        description: String::new(),
        mode: "snippet".to_string(),
        entrypoint: id.to_string(),
        // Searchable by what is in it, not only by what it is called.
        keywords: vec![keyword.to_string(), preview.to_string()],
        icon: None,
        panel: None,
    }
}

/// A calculator answer, shaped as a result so the list needs no special case.
///
/// Scored far above anything the ranker can produce, because when a query is
/// a sum the answer is the only thing being asked for.
pub fn answer_record(text: &str, input: &str) -> RankedCommand {
    RankedCommand {
        command: CommandRecord {
            id: "sill:answer".to_string(),
            extension: "sill".to_string(),
            extension_title: "Calculator".to_string(),
            command: "answer".to_string(),
            title: text.to_string(),
            subtitle: input.to_string(),
            description: String::new(),
            mode: "answer".to_string(),
            // What Enter copies.
            entrypoint: text.to_string(),
            keywords: Vec::new(),
            icon: None,
            panel: None,
        },
        score: i64::MAX,
        matched: Vec::new(),
    }
}

pub fn load_index(path: &Path) -> Vec<CommandRecord> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

// ---------------------------------------------------------------- matching

/// Subsequence match with positional scoring, in the spirit of fzf.
///
/// Returns the score and the matched indices, or `None` if the query is not a
/// subsequence of the haystack at all.
///
/// The weighting is what makes a launcher feel like it reads your mind:
/// consecutive characters and word starts are worth far more than scattered
/// hits, so "vh" finds "View History" ahead of anything that merely contains a
/// v and an h.
fn fuzzy(query: &str, haystack: &str) -> Option<(i64, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }

    let hay: Vec<char> = haystack.chars().collect();
    let hay_lower: Vec<char> = haystack.to_lowercase().chars().collect();
    let needle: Vec<char> = query.to_lowercase().chars().collect();

    // Guard: lowercasing can change length for some scripts, and the indices
    // below are handed to the UI to slice the original string.
    if hay_lower.len() != hay.len() {
        return simple_subsequence(&needle, &hay);
    }

    let mut score = 0i64;
    let mut matched = Vec::with_capacity(needle.len());
    let mut hay_i = 0usize;
    let mut previous_match: Option<usize> = None;

    for &want in &needle {
        let found = (hay_i..hay_lower.len()).find(|&i| hay_lower[i] == want)?;

        let mut points = 1i64;

        // Start of the string, or of a word.
        let boundary = found == 0
            || matches!(hay[found - 1], ' ' | '-' | '_' | '.' | '/' | ':')
            || (hay[found].is_uppercase() && hay[found - 1].is_lowercase());
        if boundary {
            points += 8;
        }

        // Directly after the previous match.
        if previous_match == Some(found.saturating_sub(1)) && found > 0 {
            points += 6;
        }

        // Earlier is better, mildly.
        points += ((32 - found.min(32)) / 8) as i64;

        score += points;
        matched.push(found);
        previous_match = Some(found);
        hay_i = found + 1;
    }

    // A query covering most of the target beats one that covers a little.
    let coverage = (needle.len() * 10 / hay.len().max(1)) as i64;
    score += coverage;

    Some((score, matched))
}

/// Fallback for strings whose length changes when lowercased.
fn simple_subsequence(needle: &[char], hay: &[char]) -> Option<(i64, Vec<usize>)> {
    let mut matched = Vec::new();
    let mut hay_i = 0;

    for &want in needle {
        let found = (hay_i..hay.len())
            .find(|&i| hay[i].to_lowercase().next() == Some(want))?;
        matched.push(found);
        hay_i = found + 1;
    }

    Some((matched.len() as i64, matched))
}

// --------------------------------------------------------------- frecency

/// How often and how recently each command was launched.
///
/// Frequency alone entrenches whatever was popular last month; recency alone
/// forgets a daily habit after one detour. The combination is what makes the
/// top result usually correct without the user typing anything.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Frecency {
    /// id -> (launch count, last launch as unix seconds)
    #[serde(default)]
    entries: HashMap<String, (u32, i64)>,
}

/// Recency buckets, in seconds, and what each is worth.
const RECENCY_TIERS: [(i64, i64); 5] = [
    (60 * 60, 100),          // within the hour
    (60 * 60 * 24, 70),      // today
    (60 * 60 * 24 * 7, 40),  // this week
    (60 * 60 * 24 * 30, 20), // this month
    (i64::MAX, 5),           // ever
];

impl Frecency {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(path, text)
    }

    /// How many distinct entries have ever been launched.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn record(&mut self, id: &str, now: i64) {
        let entry = self.entries.entry(id.to_string()).or_insert((0, now));
        entry.0 = entry.0.saturating_add(1);
        entry.1 = now;
    }

    /// Higher is more likely to be what the user wants.
    ///
    /// Recency sets the magnitude and frequency multiplies within it, rather
    /// than the two being added. Addition lets a stale habit outrank something
    /// used minutes ago, which is exactly backwards for a launcher: what you
    /// did recently is the best predictor of what you are about to do.
    ///
    /// A command used constantly but not for a month lands near the bottom,
    /// while one used constantly *and* recently dominates, which is the
    /// behaviour that makes the root list usually right before you type.
    pub fn score(&self, id: &str, now: i64) -> i64 {
        let Some(&(count, last)) = self.entries.get(id) else {
            return 0;
        };

        let age = (now - last).max(0);
        let recency = RECENCY_TIERS
            .iter()
            .find(|(window, _)| age <= *window)
            .map(|(_, points)| *points)
            .unwrap_or(5);

        // Capped so a command launched hundreds of times cannot permanently
        // outrank everything regardless of the query. The +10 keeps a
        // once-used command at its full recency value rather than a tenth.
        let frequency = 10 + count.min(20) as i64;

        recency * frequency / 10
    }
}

// ----------------------------------------------------------------- search

/// How many results a search may return.
///
/// History worth keeping, because both mistakes are easy to repeat. It was
/// first 50, which silently capped the root list at 50 of about 1,400 indexed
/// entries: every source added past that point was invisible, and the symptom
/// looked exactly like the indexing not working at all. It was then raised to
/// 2,000, above the corpus, which is not a limit at all and meant **every
/// search serialised the entire index across the IPC boundary**, measured at
/// 534 KB for an empty query against a list that draws about fifteen rows.
///
/// 120 is the number that is a limit without being a wall. Ranking still
/// considers every entry; this is only how many survive to be sent. Anything
/// past the first hundred was never going to be found by scrolling, only by
/// typing more, and typing more re-runs the search over the whole corpus.
pub const SEARCH_LIMIT: usize = 120;

/// Ranks commands for a query.
///
/// An empty query is the root list, ordered purely by frecency, which is what
/// a user sees every time they summon without typing.
pub fn search(
    commands: &[CommandRecord],
    query: &str,
    frecency: &Frecency,
    now: i64,
    limit: usize,
) -> Vec<RankedCommand> {
    search_excluding(commands, query, frecency, now, limit, &[])
}

/// The corpus is borrowed, never collected.
///
/// This runs on **every keystroke** against an index of well over a thousand
/// entries, so taking an iterator rather than a slice is what lets a caller
/// chain two sources together without deep-copying either. An earlier version
/// cloned the whole index per character typed, which is thousands of string
/// allocations for one keypress.

/// The same search, with the user's exclusion terms applied.
///
/// Mirrors `files::search` and `files::search_with`: the plain name is the
/// common case, the long one carries the settings.
pub fn search_excluding<'a>(
    commands: impl IntoIterator<Item = &'a CommandRecord>,
    query: &str,
    frecency: &Frecency,
    now: i64,
    limit: usize,
    excluded: &[String],
) -> Vec<RankedCommand> {
    let query = query.trim();

    // Scored by reference, cloned afterwards.
    //
    // The previous version cloned a whole `CommandRecord`, keywords vector and
    // all, for every candidate that matched, and then threw all but the first
    // `limit` of them away. On an empty query that is fourteen hundred deep
    // clones to produce a hundred results. Borrowing until the truncation is
    // done means only the survivors are ever copied.
    let mut scored: Vec<(i64, &CommandRecord, Vec<usize>)> = Vec::new();

    // Filtered here rather than at scan time so removing a term brings its
    // entries straight back, with no reindex.
    let excluded: Vec<String> = excluded
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    for command in commands {
        if !excluded.is_empty() && is_excluded(command, &excluded) {
            continue;
        }

        let (mut score, matched) = if query.is_empty() {
            (0, Vec::new())
        } else {
            // The title is what the user sees, so a hit there ranks above one
            // in the extension name or a keyword.
            let title = fuzzy(query, &command.title).map(|(s, m)| (s * 3, m));
            // Only title matches are highlighted, so the other sources drop
            // their indices rather than pointing into the wrong string.
            let extension = fuzzy(query, &command.extension_title).map(|(s, _)| (s * 2, Vec::new()));
            let keyword = command
                .keywords
                .iter()
                .filter_map(|k| fuzzy(query, k))
                .map(|(s, _)| (s, Vec::new()))
                .max_by_key(|(s, _)| *s);

            match [title, extension, keyword]
                .into_iter()
                .flatten()
                .max_by_key(|(s, _)| *s)
            {
                Some(best) => best,
                None => continue,
            }
        };

        score += frecency.score(&command.id, now);

        // A bare PATH executable is usually not what someone means. There are
        // roughly a thousand of them against a couple of hundred real
        // applications, so without this a CLI tool wins on any short query.
        // A penalty rather than exclusion: they are still reachable by name.
        if command.mode == "exe" {
            score -= 12;
        }

        scored.push((score, command, matched));
    }

    scored.sort_by(|(a_score, a_command, _), (b_score, b_command, _)| {
        b_score
            .cmp(a_score)
            // Stable and predictable when scores tie, rather than arbitrary.
            .then_with(|| a_command.title.cmp(&b_command.title))
    });
    scored.truncate(limit);

    scored
        .into_iter()
        .map(|(score, command, matched)| RankedCommand {
            command: command.clone(),
            score,
            matched,
        })
        .collect()
}

/// Whether an entry matches any exclusion term, by title or by path.
///
/// Path as well as title, because the useful exclusions are usually a folder:
/// hiding one vendor's whole Start Menu directory is a single term, where
/// hiding it by name is a dozen.
fn is_excluded(command: &CommandRecord, excluded: &[String]) -> bool {
    let title = command.title.to_lowercase();
    let target = command.entrypoint.to_lowercase();
    excluded
        .iter()
        .any(|term| title.contains(term.as_str()) || target.contains(term.as_str()))
}

/// Where the frecency file lives, given the app's data directory.
pub fn frecency_path(data_dir: &Path) -> PathBuf {
    data_dir.join("frecency.json")
}
