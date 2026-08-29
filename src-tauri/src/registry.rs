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
    /// What `getPreferenceValues()` answers with, from the manifest.
    ///
    /// Only extension commands have any, so it is skipped when empty rather
    /// than writing `{}` beside every one of the thousand-odd applications in
    /// the index cache.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub preferences: serde_json::Value,
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
    pub title: String,
    pub subtitle: String,
    pub mode: String,
    pub entrypoint: String,
    /// Only when it differs from `entrypoint`.
    ///
    /// Most entries take their icon from the thing they launch, and the window
    /// already falls back to `entrypoint` when this is absent. Sending the
    /// same path twice per row was the single largest field in the payload,
    /// and paths are the longest strings a result carries.
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

        // `command.command` is not here on purpose. It is the manifest name a
        // command was declared under, which matching uses and nothing in the
        // window ever reads.
        let icon = command.icon.filter(|icon| *icon != command.entrypoint);

        Self {
            id: command.id,
            extension: command.extension,
            extension_title: command.extension_title,
            title: command.title,
            subtitle: command.subtitle,
            mode: command.mode,
            entrypoint: command.entrypoint,
            icon,
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
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
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
            &[
                "voice",
                "speech",
                "talk",
                "transcribe",
                "microphone",
                "whisper",
            ],
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
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
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
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
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
        // Only extension commands carry any.
        preferences: serde_json::Value::Null,
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
            // Only extension commands carry any.
            preferences: serde_json::Value::Null,
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
/// The same, against a needle prepared once for the whole search.
///
/// Preparing it per candidate is what this exists to avoid. Lowercasing the
/// query and collecting it into a vector is three allocations, and the old
/// shape paid them **four times for every entry in the index**: once for the
/// title, once for the extension name, once per keyword, and once more in the
/// classifier. Across sixteen hundred entries that is thousands of
/// allocations for a single keystroke.
fn fuzzy_with(needle: &[char], haystack: &str) -> Option<(i64, Vec<usize>)> {
    if needle.is_empty() {
        return Some((0, Vec::new()));
    }

    let hay: Vec<char> = haystack.chars().collect();
    let hay_lower: Vec<char> = haystack.to_lowercase().chars().collect();

    // Guard: lowercasing can change length for some scripts, and the indices
    // below are handed to the UI to slice the original string.
    if hay_lower.len() != hay.len() {
        return simple_subsequence(needle, &hay);
    }

    let mut score = 0i64;
    let mut matched = Vec::with_capacity(needle.len());
    let mut hay_i = 0usize;
    let mut previous_match: Option<usize> = None;

    for &want in needle {
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
        let found = (hay_i..hay.len()).find(|&i| hay[i].to_lowercase().next() == Some(want))?;
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

// ------------------------------------------------------- match classes

/// How well a query matched, as a handful of discrete kinds.
///
/// This is what makes the list stable to type into. A fuzzy score changes by
/// a point or two on every keystroke, so ordering by it lets results trade
/// places constantly, and the position a person is reaching for moves out
/// from under them. Ordering by class means a result only moves when the
/// **kind** of match changes, which is a thing that happens rarely and for a
/// reason the user can feel.
///
/// Declaration order is preference order, best first: `Ord` is derived and
/// the sort relies on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchClass {
    /// The title is exactly what was typed.
    ExactTitle,
    /// The title starts with what was typed.
    TitlePrefix,
    /// Every character landed on the start of a word: `vh` in View History.
    TitleWordStarts,
    /// The title contains what was typed, unbroken.
    TitleSubstring,
    /// The characters are all there in order, scattered.
    TitleSubsequence,
    /// Nothing in the title. Matched the extension's name or a keyword.
    Elsewhere,
    /// Nothing matched, but a word of the title is a near-miss for what was
    /// typed. Last on purpose: a guess, offered only when nothing else fits.
    TitleTypo,
}

/// Whether a character begins a word, by the same rule [`fuzzy`] scores.
fn begins_a_word(hay: &[char], at: usize) -> bool {
    at == 0
        || matches!(hay[at - 1], ' ' | '-' | '_' | '.' | '/' | ':')
        || (hay[at].is_uppercase() && hay[at - 1].is_lowercase())
}

/// Where `needle` appears unbroken in `hay`, in character positions.
fn find_run(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&at| hay[at..at + needle.len()] == *needle)
}

/// How this command matched, and which characters of its title to highlight.
///
/// `None` when it did not match at all, which is the answer for almost every
/// entry on almost every query.
fn classify(needle: &[char], command: &CommandRecord) -> Option<(MatchClass, Vec<usize>)> {
    let hay: Vec<char> = command.title.chars().collect();
    let hay_lower: Vec<char> = command.title.to_lowercase().chars().collect();

    // Lowercasing can change length in some scripts, and these indices are
    // handed to the window to slice the *original* title.
    let aligned = hay_lower.len() == hay.len();

    if aligned {
        if hay_lower == needle {
            return Some((MatchClass::ExactTitle, (0..needle.len()).collect()));
        }
        if hay_lower.starts_with(needle) {
            return Some((MatchClass::TitlePrefix, (0..needle.len()).collect()));
        }
    }

    let scattered = fuzzy_with(needle, &command.title);

    // Checked before the substring case on purpose. Typing initials is a
    // deliberate act and `sc` should find Screen Capture ahead of Discord,
    // which merely contains those two letters in a row.
    if let Some((_, matched)) = &scattered {
        if !matched.is_empty() && matched.iter().all(|&at| begins_a_word(&hay, at)) {
            return Some((MatchClass::TitleWordStarts, matched.clone()));
        }
    }

    if aligned {
        if let Some(at) = find_run(&hay_lower, needle) {
            return Some((
                MatchClass::TitleSubstring,
                (at..at + needle.len()).collect(),
            ));
        }
    }

    if let Some((_, matched)) = scattered {
        return Some((MatchClass::TitleSubsequence, matched));
    }

    // Nothing in the title. The other sources are searched but never
    // highlighted, since their indices would point into the wrong string.
    if fuzzy_with(needle, &command.extension_title).is_some()
        || command
            .keywords
            .iter()
            .any(|k| fuzzy_with(needle, k).is_some())
    {
        return Some((MatchClass::Elsewhere, Vec::new()));
    }

    // Last resort. Nothing matched, so ask whether this was a slip of the
    // fingers. No highlight comes back: the characters that were typed are
    // not the characters in the title, and pointing at them would be a lie.
    let budget = typo_budget(needle.len());
    looks_like_a_typo_of(needle, &command.title, budget)
        .then(|| (MatchClass::TitleTypo, Vec::new()))
}

/// How many single-character mistakes are forgiven at this query length.
///
/// Nothing under four characters, because at three a budget of one matches an
/// enormous share of any index and the list fills with things the user did not
/// ask for. The allowance grows with length because a longer word gives the
/// distance more to work with before a match becomes a coincidence.
fn typo_budget(length: usize) -> usize {
    match length {
        0..=3 => 0,
        4..=6 => 1,
        _ => 2,
    }
}

/// Edit distance counting an adjacent swap as one mistake, not two.
///
/// Optimal string alignment rather than plain Levenshtein, and the difference
/// is the whole point: **transposition is the typo people actually make.**
/// `chorme` for `chrome` is one slip of the fingers and Levenshtein calls it
/// two edits, which puts it outside any budget tight enough to be useful.
///
/// Returns `None` as soon as the distance cannot come in under `budget`, so
/// the common case (two unrelated words) stops after a row or two.
fn near_miss(a: &[char], b: &[char], budget: usize) -> Option<usize> {
    // A length difference is already that many edits.
    if a.len().abs_diff(b.len()) > budget {
        return None;
    }

    // Three rows, rotated rather than reallocated. All three start at full
    // width: the rotation below hands the oldest row back as scratch, and an
    // empty one arrives at `current[0] = i` as an out-of-bounds write.
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut current: Vec<usize> = vec![0; b.len() + 1];

    for i in 1..=a.len() {
        current[0] = i;
        let mut best = current[0];

        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut value = (current[j - 1] + 1)
                .min(prev[j] + 1)
                .min(prev[j - 1] + cost);

            // The transposition case: the two characters are swapped.
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                value = value.min(prev2[j - 2] + 1);
            }

            current[j] = value;
            best = best.min(value);
        }

        // Every alignment through this row already costs more than allowed.
        if best > budget {
            return None;
        }

        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut current);
    }

    (prev[b.len()] <= budget).then_some(prev[b.len()])
}

/// Whether some word of the title is a near-miss for what was typed.
///
/// Word by word rather than against the whole title, because a title is
/// usually several words and the typo is in one of them: `chorme` should find
/// Google Chrome, and comparing it against the whole name never will.
fn looks_like_a_typo_of(needle: &[char], title: &str, budget: usize) -> bool {
    if budget == 0 {
        return false;
    }

    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .any(|word| {
            let word: Vec<char> = word.chars().collect();
            near_miss(needle, &word, budget).is_some()
        })
}

/// How a query matched this command, if it did.
///
/// Public because the class is a fact about the result worth having outside
/// ranking: the tests assert stability against it, and grouping the list by
/// it is the obvious next use.
pub fn match_class(query: &str, command: &CommandRecord) -> Option<MatchClass> {
    let needle: Vec<char> = query.trim().to_lowercase().chars().collect();
    classify(&needle, command).map(|(class, _)| class)
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

    // Lowercased and collected once for the whole search rather than once per
    // candidate. See `fuzzy_with` for what that was costing.
    let needle: Vec<char> = query.to_lowercase().chars().collect();

    // Scored by reference, cloned afterwards.
    //
    // The previous version cloned a whole `CommandRecord`, keywords vector and
    // all, for every candidate that matched, and then threw all but the first
    // `limit` of them away. On an empty query that is fourteen hundred deep
    // clones to produce a hundred results. Borrowing until the truncation is
    // done means only the survivors are ever copied.
    //
    // (class, weight, command, matched). See the sort below for why the
    // ordering is these four things and not a single score.
    let mut scored: Vec<(MatchClass, i64, &CommandRecord, Vec<usize>)> = Vec::new();

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

        // An empty query is the root list, where everything matched equally
        // and the order is purely what you reach for most.
        let (class, matched) = if query.is_empty() {
            (MatchClass::ExactTitle, Vec::new())
        } else {
            match classify(&needle, command) {
                Some(found) => found,
                None => continue,
            }
        };

        let mut weight = frecency.score(&command.id, now);

        // A bare PATH executable is usually not what someone means. There are
        // roughly a thousand of them against a couple of hundred real
        // applications, so without this a CLI tool wins on any short query.
        // A penalty rather than exclusion: they are still reachable by name.
        if command.mode == "exe" {
            weight -= 12;
        }

        scored.push((class, weight, command, matched));
    }

    /*
     * Four keys, and every one of them is stable while you type.
     *
     * Class first, so a result only changes position when the *kind* of match
     * changes. Then how much you reach for it. Then the shorter title, which
     * is the honest reading of two equally good matches: the query covers more
     * of it. Then the name, so ties are alphabetical rather than arbitrary.
     *
     * What is deliberately absent is the fuzzy score itself. It moves by a
     * point or two on every keystroke, so ordering by it lets results trade
     * places constantly and the row someone is reaching for slides out from
     * under their finger. It still decides the class; it just no longer
     * decides the position within one.
     */
    scored.sort_by(|(a_class, a_weight, a, _), (b_class, b_weight, b, _)| {
        a_class
            .cmp(b_class)
            .then_with(|| b_weight.cmp(a_weight))
            .then_with(|| a.title.chars().count().cmp(&b.title.chars().count()))
            .then_with(|| a.title.cmp(&b.title))
    });
    scored.truncate(limit);

    scored
        .into_iter()
        .map(|(_, weight, command, matched)| RankedCommand {
            command: command.clone(),
            score: weight,
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
