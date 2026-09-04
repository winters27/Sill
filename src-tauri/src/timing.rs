//! What the launcher costs to reach.
//!
//! Two numbers, and they are the two the audit refused to let anybody claim
//! without measuring: how long from pressing the hotkey to being able to type,
//! and how long from starting the process to the hotkey working at all.
//!
//! ## Why a summon is two halves
//!
//! Rust can see when the window was told to show itself. It cannot see when
//! the page finished painting, and the page is the part somebody is waiting
//! for: a window that is up but blank is not a launcher you can use. So the
//! window reports the second half back, and a summon is only complete when it
//! has.
//!
//! A summon that never reports is kept as half a measurement rather than
//! thrown away. "Shown in 9 ms and never painted" is the most interesting
//! thing this could ever record and dropping it would hide exactly the
//! failure worth knowing about.
//!
//! ## What a source and an extension cost
//!
//! A summon says the launcher opened quickly. It says nothing about which part
//! of a search was slow, and "Sill feels slow when I type" has never had an
//! answer beyond a guess. So each search source and each extension load adds
//! its own time up here: how many times, how long in total, and the worst one.
//!
//! Three numbers rather than a list of every call. A list of ten thousand
//! keystrokes is a memory leak with a nice name, and the question anybody
//! actually asks is "which source is the slow one" and "was it always slow or
//! was it once".
//!
//! ## Why this costs nothing
//!
//! Two instants and a push onto a bounded queue, on a path a person triggers.
//! Nothing runs on a timer and nothing is measured while the launcher is
//! closed, which is the state it is in nearly all the time.
//!
//! The per-source part runs on a path that does run per keystroke, so it is
//! held to the same standard: one `Instant`, one uncontended lock, and a
//! lookup in a map of at most a dozen `&'static str` keys. **Nothing is
//! allocated**, because the source names are constants and an extension's name
//! is copied once, the first time that extension is opened. Against a search
//! that costs milliseconds this is not measurable, and while nobody is typing
//! it is not running.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

/// How many summons are kept.
///
/// Enough to see a distribution rather than an anecdote, few enough that the
/// whole thing is a few hundred bytes. The one before last is what somebody
/// means by "it felt slow just then".
const KEPT: usize = 20;

/// How many extensions may have their own line.
///
/// A bound rather than a limit anybody reaches: the store's largest users have
/// a couple of dozen installed, and this only counts the ones actually opened
/// this session. It exists because the key is a name that arrives from a
/// manifest, and nothing that grows from a name may grow without end.
const MOST_EXTENSIONS: usize = 64;

/// What one source or one extension has cost this session.
///
/// The three numbers that answer the question. An average alone hides the one
/// slow call; a worst alone hides that it is slow every time.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    /// The source, or the extension's id.
    pub name: String,
    pub count: u64,
    /// Microseconds, because a search source answers in a couple of
    /// milliseconds and milliseconds would round most of them to zero.
    pub total_us: u64,
    pub slowest_us: u64,
}

impl Cost {
    /// The mean, which is what to read first.
    pub fn average_us(&self) -> u64 {
        self.total_us.checked_div(self.count).unwrap_or(0)
    }
}

/// The running total behind one [`Cost`], without its name.
#[derive(Debug, Clone, Copy, Default)]
struct Adding {
    count: u64,
    total_us: u64,
    slowest_us: u64,
}

impl Adding {
    fn add(&mut self, took: Duration) {
        // Saturating rather than wrapping: a machine that slept mid-search
        // reports an absurd duration, and an absurd number in a diagnostic is
        // better than a small one that used to be absurd.
        let us = u64::try_from(took.as_micros()).unwrap_or(u64::MAX);

        self.count = self.count.saturating_add(1);
        self.total_us = self.total_us.saturating_add(us);
        self.slowest_us = self.slowest_us.max(us);
    }

    fn named(&self, name: &str) -> Cost {
        Cost {
            name: name.to_string(),
            count: self.count,
            total_us: self.total_us,
            slowest_us: self.slowest_us,
        }
    }
}

/// One summon, from the hotkey to being able to type.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summon {
    /// Hotkey to the window being shown, in milliseconds.
    pub shown_ms: u128,
    /// Shown to the page having painted, in milliseconds.
    ///
    /// `None` for a summon the window never reported. See the module note:
    /// that is a measurement, not a missing one.
    pub painted_ms: Option<u128>,
}

impl Summon {
    /// The whole thing, when it is whole.
    pub fn total_ms(&self) -> Option<u128> {
        self.painted_ms.map(|painted| self.shown_ms + painted)
    }
}

/// What has been measured so far.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Process start to the hotkey being live, in milliseconds.
    pub cold_start_ms: Option<u128>,
    /// The most recent summons, oldest first.
    pub summons: Vec<Summon>,
    /// The middle of the complete ones, which is what to quote.
    ///
    /// The median rather than the mean, because the slow ones are slow for
    /// reasons that have nothing to do with Sill (a machine waking a disk, a
    /// display coming out of sleep) and one of those drags an average
    /// somewhere no summon ever was.
    pub median_ms: Option<u128>,
    /// What each search source has cost, slowest first.
    pub sources: Vec<Cost>,
    /// What each extension opened this session has cost, slowest first.
    pub extensions: Vec<Cost>,
}

#[derive(Default)]
struct Inner {
    /// When the summon in flight began, if there is one.
    began: Option<Instant>,
    /// Whether the summon in flight has been shown but not yet painted.
    awaiting_paint: bool,
    kept: VecDeque<Summon>,
    cold_start: Option<Duration>,
    /// Keyed by a constant, so recording one allocates nothing.
    sources: BTreeMap<&'static str, Adding>,
    /// Keyed by an extension id, copied once the first time it is opened.
    extensions: BTreeMap<String, Adding>,
}

/// The timings, held as managed state rather than in a static.
///
/// A static would be a fifth singleton in a codebase that has written down
/// that it does not want them, for something that is per-application by
/// nature.
pub struct Timings {
    inner: Mutex<Inner>,
}

impl Default for Timings {
    fn default() -> Self {
        Self::new()
    }
}

impl Timings {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }

    /// The hotkey fired.
    pub fn summon_began(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            // A summon already in flight loses its paint half. Better than
            // attributing the next paint to the wrong press.
            if inner.awaiting_paint {
                inner.awaiting_paint = false;
            }

            inner.began = Some(Instant::now());
        }
    }

    /// The window has been shown and the page told to paint.
    pub fn summon_shown(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            let Some(began) = inner.began else {
                return;
            };

            let shown_ms = began.elapsed().as_millis();

            inner.awaiting_paint = true;
            push(
                &mut inner.kept,
                Summon {
                    shown_ms,
                    painted_ms: None,
                },
            );
        }
    }

    /// The page has painted, which is when somebody can actually type.
    pub fn summon_painted(&self) {
        let line = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };

            if !inner.awaiting_paint {
                // A paint with no summon behind it: the page reloaded, or the
                // window was shown some other way. Nothing to attribute it to.
                return;
            }

            let Some(began) = inner.began.take() else {
                return;
            };

            inner.awaiting_paint = false;

            let Some(last) = inner.kept.back_mut() else {
                return;
            };

            let total = began.elapsed().as_millis();
            last.painted_ms = Some(total.saturating_sub(last.shown_ms));

            format!(
                "summon {total} ms ({} to show, {} to paint)",
                last.shown_ms,
                last.painted_ms.unwrap_or(0),
            )
        };

        // Logged outside the lock. `say!` writes to a file, and holding a
        // mutex across a disk write is how a hot path becomes a slow one.
        crate::say!("{line}");
    }

    /// The launcher is ready to be summoned.
    pub fn ready(&self, since_start: Duration) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.cold_start = Some(since_start);
        }

        crate::say!("ready in {} ms", since_start.as_millis());
    }

    /// Times a source for as long as the returned value is alive.
    ///
    /// A guard rather than a call at the end, because every one of these
    /// commands has several ways out: an empty query returns early, a lock
    /// gives up, a `?` propagates. A stopwatch stopped on the last line only
    /// measures the path that reaches the last line, which is the one that was
    /// never in doubt.
    pub fn timing(&self, source: &'static str) -> Timed<'_> {
        Timed {
            timings: self,
            source,
            began: Instant::now(),
        }
    }

    /// One search source answered, and this is what it took.
    ///
    /// A constant for the name rather than a `String`, deliberately: this runs
    /// on a keystroke and the sources are a fixed list, so nothing here
    /// allocates. See the module note on what that buys.
    pub fn source_took(&self, source: &'static str, took: Duration) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.sources.entry(source).or_default().add(took);
        }

        crate::detail!("{source} answered in {} us", took.as_micros());
    }

    /// One extension was opened, and this is what it took.
    ///
    /// The 300 ms extension load is the number this project has spent the most
    /// effort on, and until now the only way to see it was a stopwatch. Which
    /// extension is slow is the useful half: "extensions are slow" is not
    /// actionable and "this one takes 800 ms" is.
    pub fn extension_took(&self, extension: &str, took: Duration) {
        if let Ok(mut inner) = self.inner.lock() {
            // Looked up before inserting, so a full map still counts the
            // extensions already in it rather than going silent.
            if let Some(adding) = inner.extensions.get_mut(extension) {
                adding.add(took);
            } else if inner.extensions.len() < MOST_EXTENSIONS {
                inner
                    .extensions
                    .entry(extension.to_string())
                    .or_default()
                    .add(took);
            }
        }

        crate::detail!("{extension} loaded in {} ms", took.as_millis());
    }

    pub fn report(&self) -> Report {
        let Ok(inner) = self.inner.lock() else {
            return Report {
                cold_start_ms: None,
                summons: Vec::new(),
                median_ms: None,
                sources: Vec::new(),
                extensions: Vec::new(),
            };
        };

        let summons: Vec<Summon> = inner.kept.iter().copied().collect();

        Report {
            cold_start_ms: inner.cold_start.map(|d| d.as_millis()),
            median_ms: median(&summons),
            summons,
            sources: worst_first(inner.sources.iter().map(|(name, add)| add.named(name))),
            extensions: worst_first(inner.extensions.iter().map(|(name, add)| add.named(name))),
        }
    }
}

/// One source being timed, until it goes out of scope.
///
/// See [`Timings::timing`] on why this is a guard.
pub struct Timed<'a> {
    timings: &'a Timings,
    source: &'static str,
    began: Instant,
}

impl Drop for Timed<'_> {
    fn drop(&mut self) {
        self.timings.source_took(self.source, self.began.elapsed());
    }
}

/// Slowest on average first, which is the order somebody reads them in.
///
/// By the average rather than by the total, because the total is mostly a
/// count: the root list is searched on every keystroke and an extension is
/// opened once, so ordering by total would always put the same thing on top
/// whatever it cost.
fn worst_first(costs: impl Iterator<Item = Cost>) -> Vec<Cost> {
    let mut costs: Vec<Cost> = costs.collect();
    costs.sort_by(|a, b| {
        b.average_us()
            .cmp(&a.average_us())
            .then_with(|| a.name.cmp(&b.name))
    });
    costs
}

fn push(kept: &mut VecDeque<Summon>, one: Summon) {
    if kept.len() == KEPT {
        kept.pop_front();
    }

    kept.push_back(one);
}

/// The middle complete summon, or nothing if none has completed.
pub fn median(summons: &[Summon]) -> Option<u128> {
    let mut complete: Vec<u128> = summons.iter().filter_map(Summon::total_ms).collect();

    if complete.is_empty() {
        return None;
    }

    complete.sort_unstable();
    Some(complete[complete.len() / 2])
}

/// How long this process has been running.
///
/// Asked of Windows rather than measured from the first line of our own code,
/// because everything before that line is part of what somebody waited for:
/// the loader, the runtime, the linked libraries. Measuring from `main` would
/// report a number that flatters us by exactly the part we cannot see.
#[cfg(windows)]
pub fn since_process_start() -> Option<Duration> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: four owned structures, and the pseudo-handle for this process
    // needs no closing.
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    }
    .is_ok();

    if !ok {
        return None;
    }

    let started = filetime_to_unix_nanos(created)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();

    now.checked_sub(started)
        .map(|nanos| Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX)))
}

#[cfg(not(windows))]
pub fn since_process_start() -> Option<Duration> {
    None
}

/// A Windows FILETIME as nanoseconds since the Unix epoch.
///
/// FILETIME counts hundred-nanosecond intervals from 1601, which is 11,644,473,600
/// seconds before 1970.
#[cfg(windows)]
fn filetime_to_unix_nanos(time: windows::Win32::Foundation::FILETIME) -> Option<u128> {
    const EPOCH_DIFFERENCE_SECONDS: u64 = 11_644_473_600;

    let ticks = (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime);
    let since_1601 = Duration::from_nanos(ticks.checked_mul(100)?);

    since_1601
        .checked_sub(Duration::from_secs(EPOCH_DIFFERENCE_SECONDS))
        .map(|d| d.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summon(shown: u128, painted: Option<u128>) -> Summon {
        Summon {
            shown_ms: shown,
            painted_ms: painted,
        }
    }

    #[test]
    fn a_summon_is_worth_nothing_until_it_has_painted() {
        // Shown is not the same as usable. A window that is up and blank is
        // not a launcher you can type into.
        assert_eq!(summon(9, None).total_ms(), None);
        assert_eq!(summon(9, Some(4)).total_ms(), Some(13));
    }

    /// The median, because the slow ones are slow for reasons that have
    /// nothing to do with Sill and one of them drags a mean somewhere no
    /// summon ever was.
    #[test]
    fn the_middle_one_is_what_gets_quoted() {
        let measured = [
            summon(5, Some(5)),
            summon(5, Some(5)),
            summon(5, Some(5)),
            summon(5, Some(5)),
            // A machine waking a disk.
            summon(900, Some(400)),
        ];

        assert_eq!(median(&measured), Some(10));
    }

    #[test]
    fn a_summon_that_never_painted_is_not_counted_as_fast() {
        let measured = [summon(9, None), summon(20, Some(10))];
        assert_eq!(median(&measured), Some(30));
    }

    #[test]
    fn nothing_measured_is_said_rather_than_guessed() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[summon(9, None)]), None);
    }

    #[test]
    fn only_the_last_few_are_kept() {
        let mut kept = VecDeque::new();

        for at in 0..(KEPT + 5) {
            push(&mut kept, summon(at as u128, Some(1)));
        }

        assert_eq!(kept.len(), KEPT);
        // The oldest went, not the newest.
        assert_eq!(kept.front().map(|s| s.shown_ms), Some(5));
        assert_eq!(kept.back().map(|s| s.shown_ms), Some((KEPT + 4) as u128));
    }

    /// A paint arriving with no summon behind it is not attributed to the one
    /// before, which would report a summon that took as long as somebody left
    /// the launcher open.
    #[test]
    fn a_stray_paint_is_ignored() {
        let timings = Timings::new();

        timings.summon_began();
        timings.summon_shown();
        timings.summon_painted();

        // The page reloading, long afterwards.
        timings.summon_painted();

        let report = timings.report();
        assert_eq!(report.summons.len(), 1);
    }

    /// Pressing the hotkey twice before the first paint leaves one record
    /// waiting rather than attributing the paint to the wrong press.
    #[test]
    fn a_second_press_does_not_steal_the_first_ones_paint() {
        let timings = Timings::new();

        timings.summon_began();
        timings.summon_shown();
        timings.summon_began();
        timings.summon_shown();
        timings.summon_painted();

        let report = timings.report();
        assert_eq!(report.summons.len(), 2);
        assert!(report.summons[0].painted_ms.is_none());
        assert!(report.summons[1].painted_ms.is_some());
    }

    /// A source is added up rather than listed, and the worst one is kept.
    ///
    /// The worst matters on its own. An average of 3 ms over a thousand
    /// keystrokes hides the one that took 400, and the one that took 400 is
    /// what somebody felt and is asking about.
    #[test]
    fn a_source_is_added_up_and_its_worst_call_is_kept() {
        let timings = Timings::new();

        // The worst one in the middle, deliberately. Recorded last it would
        // also be the most recent, and a test where those coincide passes for
        // an implementation that only ever keeps the latest.
        timings.source_took("commands", Duration::from_micros(2_000));
        timings.source_took("commands", Duration::from_micros(400_000));
        timings.source_took("commands", Duration::from_micros(4_000));

        let report = timings.report();
        let commands = &report.sources[0];

        assert_eq!(commands.name, "commands");
        assert_eq!(commands.count, 3);
        assert_eq!(commands.total_us, 406_000);
        assert_eq!(
            commands.slowest_us, 400_000,
            "the worst call is gone, which is the one somebody noticed"
        );
        assert_eq!(commands.average_us(), 135_333);
    }

    /// The slow one is first, and it is the slow one *per call*.
    ///
    /// Ordering by the total would always put the root list on top: it is
    /// searched on every keystroke and an extension is opened once, so the
    /// total is mostly a count of how often something ran.
    #[test]
    fn the_slowest_per_call_is_named_first_not_the_busiest() {
        let timings = Timings::new();

        for _ in 0..500 {
            timings.source_took("commands", Duration::from_micros(3_000));
        }
        timings.source_took("files", Duration::from_micros(90_000));

        let report = timings.report();
        let named: Vec<&str> = report
            .sources
            .iter()
            .map(|cost| cost.name.as_str())
            .collect();

        assert_eq!(named, ["files", "commands"]);
    }

    /// An extension's name arrives from a manifest, so the list is bounded.
    #[test]
    fn extensions_cannot_grow_the_list_without_end() {
        let timings = Timings::new();

        for at in 0..(MOST_EXTENSIONS + 20) {
            timings.extension_took(&format!("extension-{at}"), Duration::from_millis(1));
        }

        assert_eq!(timings.report().extensions.len(), MOST_EXTENSIONS);
    }

    /// A full list still counts the extensions already in it.
    ///
    /// The bound is there to stop the map growing, not to stop measuring. An
    /// early return once it is full would freeze every extension's numbers at
    /// whatever they were when the sixty-fourth appeared.
    #[test]
    fn a_full_list_keeps_counting_what_is_already_in_it() {
        let timings = Timings::new();

        for at in 0..MOST_EXTENSIONS {
            timings.extension_took(&format!("extension-{at}"), Duration::from_millis(1));
        }

        // The one that does not fit.
        timings.extension_took("one-too-many", Duration::from_millis(1));
        // And then one that is already in.
        timings.extension_took("extension-0", Duration::from_millis(9));

        let report = timings.report();
        let first = report
            .extensions
            .iter()
            .find(|cost| cost.name == "extension-0")
            .expect("extension-0 was recorded before the list filled");

        assert_eq!(first.count, 2);
        assert_eq!(first.slowest_us, 9_000);
        assert!(report.extensions.iter().all(|c| c.name != "one-too-many"));
    }

    /// Nothing measured says nothing, rather than a zero somebody would quote.
    #[test]
    fn a_session_that_searched_nothing_reports_no_sources() {
        let report = Timings::new().report();

        assert!(report.sources.is_empty());
        assert!(report.extensions.is_empty());
    }
}
