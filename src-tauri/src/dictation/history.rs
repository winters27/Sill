//! Every finished dictation, and what can be counted from them.
//!
//! Append-only JSON lines rather than one JSON array: a dictation ends with a
//! single `write`, with no read-modify-write of the whole file, so a crash
//! mid-write costs the last line instead of the entire history. A malformed
//! line is skipped on read for the same reason.
//!
//! This is the store three separate features read: the history list, the
//! statistics on the settings panel, and "get last transcription".

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::dictation::error::{DictationError, Result};

/// Entries kept. Beyond this the file is trimmed on the next write.
const KEEP: usize = 2_000;

/// How far past `KEEP` the file grows before it is trimmed.
///
/// Trimming rewrites the whole file, so doing it the moment the cap is passed
/// would rewrite on every single dictation forever. With slack it happens
/// once every few hundred.
const SLACK: usize = 250;

/// Typing speed the "time saved" figure is measured against.
///
/// 40 words per minute is the usual figure quoted for an average adult typing
/// prose, as opposed to the 60 to 80 a fast touch typist reaches. Stated as a
/// constant rather than buried in the arithmetic because it is an assumption,
/// and the UI says so where the number is shown.
pub const TYPING_WPM: f64 = 40.0;

/// One completed dictation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Unix seconds when the transcript arrived.
    pub at: i64,
    pub text: String,
    /// Counted once on write, so the list and the statistics agree and
    /// neither has to re-split every transcript to draw a row.
    pub words: usize,
    /// Length of the recording.
    pub spoken_ms: u64,
    /// How long the transcription itself took.
    pub transcribe_ms: u64,
    pub provider: String,
    /// The model, when the provider has one worth naming.
    #[serde(default)]
    pub model: String,
    /// The application that was frontmost, when app context is on.
    #[serde(default)]
    pub app: Option<String>,
}

/// Counted totals over some window of history.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub dictations: usize,
    pub total_words: usize,
    /// Total time spent speaking, in seconds.
    pub spoken_seconds: u64,
    /// Words per minute actually spoken, over the whole window.
    pub words_per_minute: u32,
    /// Seconds saved against typing the same words at [`TYPING_WPM`].
    ///
    /// Clamped at zero: dictating slower than you type is possible, and a
    /// negative "time saved" is a worse answer than none.
    pub seconds_saved: u64,
}

/// Which slice of history to count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Range {
    Today,
    Week,
    Month,
    AllTime,
}

impl Range {
    /// Earliest timestamp this range includes, given the current time.
    fn since(self, now: i64) -> i64 {
        const DAY: i64 = 60 * 60 * 24;
        match self {
            Range::Today => now - DAY,
            Range::Week => now - DAY * 7,
            Range::Month => now - DAY * 30,
            Range::AllTime => i64::MIN,
        }
    }
}

pub fn path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| DictationError::Platform(format!("app data dir: {e}")))?
        .join("dictation-history.jsonl"))
}

/// Words in a transcript.
///
/// Whitespace separated, which is what every "words per minute" figure means
/// and what the transcript's own whitespace collapsing already normalised.
pub fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Appends one entry, trimming the file when it has grown well past the cap
/// or when the retention policy has something to drop.
pub fn record(app: &AppHandle, entry: &Entry) -> Result<()> {
    let retain_days = app
        .try_state::<crate::dictation::service::DictationService>()
        .map(|service| service.settings().retain_days)
        .unwrap_or(0);
    let file = path(app)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut line = serde_json::to_string(entry)
        .map_err(|e| DictationError::Other(format!("Could not encode the transcript: {e}")))?;
    line.push('\n');

    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?;
    handle.write_all(line.as_bytes())?;
    drop(handle);

    trim_if_needed(&file, retain_days);
    Ok(())
}

/// Every entry, newest first.
pub fn load(app: &AppHandle) -> Vec<Entry> {
    path(app).map(|file| read(&file)).unwrap_or_default()
}

/// The most recent transcript, if there is one.
pub fn last(app: &AppHandle) -> Option<Entry> {
    load(app).into_iter().next()
}

/// Deletes the whole history. Returns how many entries went.
pub fn clear(app: &AppHandle) -> Result<usize> {
    let file = path(app)?;
    let count = read(&file).len();
    if file.is_file() {
        std::fs::remove_file(&file)?;
    }
    Ok(count)
}

/// Removes one entry by timestamp. Returns whether anything matched.
pub fn remove(app: &AppHandle, at: i64) -> Result<bool> {
    let file = path(app)?;
    let mut entries = read(&file);
    let before = entries.len();
    entries.retain(|entry| entry.at != at);

    if entries.len() == before {
        return Ok(false);
    }

    // Written oldest first, because that is the order the file is in.
    entries.reverse();
    write_all(&file, &entries)?;
    Ok(true)
}

/// Totals over `range`, given the current time.
pub fn stats(entries: &[Entry], range: Range, now: i64) -> Stats {
    let since = range.since(now);
    let mut out = Stats::default();
    let mut spoken_ms = 0u64;

    for entry in entries.iter().filter(|entry| entry.at >= since) {
        out.dictations += 1;
        out.total_words += entry.words;
        spoken_ms += entry.spoken_ms;
    }

    out.spoken_seconds = spoken_ms / 1_000;

    let spoken_minutes = spoken_ms as f64 / 60_000.0;
    if spoken_minutes > 0.0 {
        out.words_per_minute = (out.total_words as f64 / spoken_minutes).round() as u32;
    }

    // What typing the same words would have cost, less what speaking them
    // actually cost. Clamped, because dictating slower than you type is
    // possible and a negative saving is a worse answer than none.
    let typing_seconds = (out.total_words as f64 / TYPING_WPM) * 60.0;
    out.seconds_saved = (typing_seconds - spoken_ms as f64 / 1_000.0).max(0.0) as u64;

    out
}

/// Reads the file, newest first, skipping anything unparseable.
fn read(file: &Path) -> Vec<Entry> {
    let Ok(handle) = std::fs::File::open(file) else {
        return Vec::new();
    };

    let mut entries: Vec<Entry> = BufReader::new(handle)
        .lines()
        .map_while(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        // A line half-written by a crash is skipped rather than failing the
        // whole read: losing one transcript is not worth losing the rest.
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();

    entries.reverse();
    entries
}

/// Rewrites the file with `entries`, which must be oldest first.
fn write_all(file: &Path, entries: &[Entry]) -> Result<()> {
    let mut body = String::new();
    for entry in entries {
        if let Ok(line) = serde_json::to_string(entry) {
            body.push_str(&line);
            body.push('\n');
        }
    }

    // Staged and renamed, so an interrupted rewrite cannot leave a truncated
    // history where a whole one used to be.
    let staging = file.with_extension("jsonl.partial");
    std::fs::write(&staging, body)?;
    if let Err(err) = std::fs::rename(&staging, file) {
        let _ = std::fs::remove_file(&staging);
        return Err(DictationError::Io(err));
    }
    Ok(())
}

fn trim_if_needed(file: &Path, retain_days: u32) {
    let entries = read(file);
    let by_count = entries.len() > KEEP + SLACK;
    let expired = retain_days > 0
        && entries
            .iter()
            .any(|entry| older_than(entry, retain_days, now()));

    // Rewriting the file is the cost here, so it is only paid when something
    // would actually come out of it. Without the second question a machine
    // with a retention policy and nothing old enough to drop would rewrite its
    // whole history on every dictation forever.
    if !by_count && !expired {
        return;
    }

    let mut keep = keep_within(entries, retain_days, now());
    keep.reverse();
    if let Err(err) = write_all(file, &keep) {
        crate::say!("could not trim the dictation history: {err}");
    }
}

/// Now, in unix seconds.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0)
}

/// Whether one entry has outlived the policy.
fn older_than(entry: &Entry, retain_days: u32, now: i64) -> bool {
    entry.at < now - (retain_days as i64) * 86_400
}

/// What survives both limits, newest first.
///
/// Two rules, different in kind. The count stops the file growing without end
/// and is not a policy anybody chose; the age is. Both are applied, and **the
/// order between them does not matter**: the list arrives newest first and an
/// entry's age rises with its position, so filtering by age and then taking
/// the newest few gives what taking the newest few and then filtering does.
/// Worth writing down because it reads as though it should matter, and a test
/// asserting the order would pass whichever way round it was written.
///
/// `retain_days` of zero means no age limit at all, which is the same spelling
/// the clipboard uses for the same idea.
pub fn keep_within(entries: Vec<Entry>, retain_days: u32, now: i64) -> Vec<Entry> {
    let by_age: Vec<Entry> = if retain_days == 0 {
        entries
    } else {
        entries
            .into_iter()
            .filter(|entry| !older_than(entry, retain_days, now))
            .collect()
    };

    // `read` returns newest first, so the newest are already the front.
    by_age.into_iter().take(KEEP).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;
    const HOUR: i64 = 3_600;

    /// Zero is the spelling for "no age limit", the same as the clipboard's.
    #[test]
    fn keeping_everything_is_what_zero_days_means() {
        let entries = vec![entry(NOW - 400 * 24 * HOUR, 3, 900), entry(NOW, 3, 900)];

        let kept = keep_within(entries, 0, NOW);

        assert_eq!(kept.len(), 2, "a year old entry survives no policy at all");
    }

    #[test]
    fn a_transcript_older_than_the_policy_goes_and_the_rest_stay() {
        let entries = vec![
            entry(NOW - 1 * HOUR, 3, 900),
            entry(NOW - 29 * 24 * HOUR, 3, 900),
            entry(NOW - 31 * 24 * HOUR, 3, 900),
        ];

        let kept = keep_within(entries, 30, NOW);

        assert_eq!(kept.len(), 2, "only the one past thirty days should go");
        assert!(
            kept.iter().all(|e| e.at > NOW - 30 * 24 * HOUR),
            "something older than the policy survived"
        );
    }

    /// The boundary, stated rather than left to a comparison nobody checked.
    #[test]
    fn an_entry_exactly_at_the_limit_is_kept() {
        let at_the_edge = vec![entry(NOW - 30 * 24 * HOUR, 3, 900)];

        assert_eq!(
            keep_within(at_the_edge, 30, NOW).len(),
            1,
            "the limit is how long something is kept, not when it is already gone"
        );
    }

    /// The age limit drops entries the count would have been happy to keep.
    ///
    /// Deliberately a handful of entries rather than thousands: well under
    /// `KEEP`, the count limit does nothing at all, so the only thing that can
    /// remove anything here is the policy. A version of this with more than
    /// `KEEP` entries passes whether or not the age filter runs, because the
    /// count alone would have cut it to the same answer.
    #[test]
    fn age_removes_what_the_count_alone_would_have_kept() {
        let mut entries = vec![entry(NOW, 3, 900)];
        for day in 0..5 {
            entries.push(entry(NOW - (60 + day) * 24 * HOUR, 3, 900));
        }

        assert_eq!(
            keep_within(entries.clone(), 0, NOW).len(),
            6,
            "with no policy the count keeps all six"
        );
        assert_eq!(
            keep_within(entries, 30, NOW).len(),
            1,
            "only today's entry is inside the policy"
        );
    }

    /// With no age policy the count still bounds the file.
    #[test]
    fn the_count_still_bounds_a_history_with_no_age_policy() {
        let entries: Vec<Entry> = (0..KEEP + 50)
            .map(|i| entry(NOW - i as i64 * 60, 3, 900))
            .collect();

        assert_eq!(keep_within(entries, 0, NOW).len(), KEEP);
    }

    fn entry(at: i64, words: usize, spoken_ms: u64) -> Entry {
        Entry {
            at,
            text: "word ".repeat(words).trim().to_string(),
            words,
            spoken_ms,
            transcribe_ms: 400,
            provider: "local".into(),
            model: "small.en".into(),
            app: None,
        }
    }

    #[test]
    fn words_are_counted_the_way_a_wpm_figure_means_them() {
        assert_eq!(count_words("hello there world"), 3);
        assert_eq!(count_words("  padded   out  "), 2);
        assert_eq!(count_words(""), 0);
    }

    #[test]
    fn speaking_rate_is_words_over_time_actually_spoken() {
        // 100 words in exactly one minute.
        let stats = stats(&[entry(NOW, 100, 60_000)], Range::AllTime, NOW);

        assert_eq!(stats.dictations, 1);
        assert_eq!(stats.total_words, 100);
        assert_eq!(stats.words_per_minute, 100);
        assert_eq!(stats.spoken_seconds, 60);
    }

    #[test]
    fn time_saved_is_measured_against_typing_the_same_words() {
        // 40 words is one minute of typing at the baseline. Spoken in 15
        // seconds, so 45 seconds are saved.
        let stats = stats(&[entry(NOW, 40, 15_000)], Range::AllTime, NOW);
        assert_eq!(stats.seconds_saved, 45);
    }

    #[test]
    fn dictating_slower_than_typing_saves_nothing_rather_than_negative_time() {
        // Ten words taking two minutes is far slower than typing them, and a
        // negative "time saved" is a worse answer than none.
        let stats = stats(&[entry(NOW, 10, 120_000)], Range::AllTime, NOW);
        assert_eq!(stats.seconds_saved, 0);
    }

    #[test]
    fn an_empty_history_reports_zeroes_rather_than_dividing_by_zero() {
        let stats = stats(&[], Range::AllTime, NOW);
        assert_eq!(stats.words_per_minute, 0);
        assert_eq!(stats.seconds_saved, 0);
        assert_eq!(stats.dictations, 0);
    }

    #[test]
    fn a_range_counts_only_what_falls_inside_it() {
        let entries = vec![
            entry(NOW - HOUR, 10, 10_000),
            entry(NOW - HOUR * 48, 10, 10_000),
            entry(NOW - HOUR * 24 * 10, 10, 10_000),
        ];

        assert_eq!(stats(&entries, Range::Today, NOW).dictations, 1);
        assert_eq!(stats(&entries, Range::Week, NOW).dictations, 2);
        assert_eq!(stats(&entries, Range::Month, NOW).dictations, 3);
        assert_eq!(stats(&entries, Range::AllTime, NOW).dictations, 3);
    }

    #[test]
    fn reading_skips_a_line_a_crash_cut_in_half() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("history.jsonl");

        let good = serde_json::to_string(&entry(NOW, 3, 1_000)).unwrap();
        // A truncated line is exactly what an interrupted append leaves.
        std::fs::write(&file, format!("{good}\n{{\"at\":170000\n{good}\n")).unwrap();

        assert_eq!(
            read(&file).len(),
            2,
            "the two whole lines survive and the torn one is dropped"
        );
    }

    #[test]
    fn reading_returns_newest_first() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("history.jsonl");

        let older = serde_json::to_string(&entry(NOW - HOUR, 1, 100)).unwrap();
        let newer = serde_json::to_string(&entry(NOW, 2, 200)).unwrap();
        std::fs::write(&file, format!("{older}\n{newer}\n")).unwrap();

        let entries = read(&file);
        assert_eq!(entries[0].at, NOW, "the list is drawn newest first");
        assert_eq!(entries[1].at, NOW - HOUR);
    }

    #[test]
    fn trimming_keeps_the_newest_and_only_runs_past_the_slack() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("history.jsonl");

        let mut oldest_first: Vec<Entry> = (0..KEEP + SLACK)
            .map(|i| entry(NOW - (KEEP + SLACK - i) as i64, 1, 100))
            .collect();
        write_all(&file, &oldest_first).unwrap();

        trim_if_needed(&file, 0);
        assert_eq!(
            read(&file).len(),
            KEEP + SLACK,
            "at the slack boundary nothing is rewritten"
        );

        oldest_first.push(entry(NOW, 1, 100));
        write_all(&file, &oldest_first).unwrap();
        trim_if_needed(&file, 0);

        let kept = read(&file);
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0].at, NOW, "the newest entry is the one that survives");
    }
}
