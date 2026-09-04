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
use std::sync::{Arc, Mutex};
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

/// Whether Node had to be started for this activation.
///
/// The two are different enough that reporting one as the other is a lie.
/// Sill keeps the extension runtime up for five minutes after anything last
/// used it, and a warm worker waiting for the next command; the first launch
/// after that has gone pays for a process start, a thread and a module
/// evaluation, and every launch afterwards pays for the last of the three.
///
/// Which one somebody gets is not a matter of luck. A person who opens an
/// extension in the morning and again after lunch pays cold both times, and
/// somebody working inside one pays warm all afternoon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// The extension runtime was not running and had to be started.
    Cold,
    /// It was already up, with a worker waiting.
    Warm,
}

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

/// What one extension costs to open, told twice.
///
/// Both halves, or one, or neither. An extension opened once this run has a
/// cold figure and no warm one, and saying nothing about the warm case is the
/// honest answer: guessing it from the cold one would be inventing the number
/// that matters most.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    /// The extension's id, as its manifest spells it.
    pub name: String,
    /// Openings that had to start the extension runtime first.
    pub cold: Option<Cost>,
    /// Openings that did not.
    pub warm: Option<Cost>,
    /**
    The most memory any of its commands was holding when it was closed, in
    bytes.

    Read on the way out rather than sampled, because that is the last moment it
    exists and because a timer watching an idle worker is the wakeup this
    launcher refuses to spend. It is what makes the panel a comparison at all:
    only one command is usually loaded, so a screen showing memory for what is
    running shows one number, and somebody hunting for the expensive extension
    has closed the other three by the time they come to look.

    The largest seen rather than the last, because an extension whose list has
    been narrowed to nothing is holding almost nothing at the moment it is
    closed, and what it cost is what it cost.
    */
    pub held_bytes: Option<u64>,
}

impl Opening {
    /// Whether anything at all is known about how long this takes to open.
    pub fn timed(&self) -> bool {
        self.cold.is_some() || self.warm.is_some()
    }

    /// The figure to compare two extensions by.
    ///
    /// The warm one, when there is one. Cold time is mostly Node starting,
    /// which every extension pays equally and none of them causes; what one
    /// extension does and another does not is the work it does once it is
    /// running. An extension only ever opened cold is compared on that,
    /// because the alternative is leaving it out of the comparison entirely.
    pub fn typical_us(&self) -> u64 {
        self.warm
            .as_ref()
            .or(self.cold.as_ref())
            .map(Cost::average_us)
            .unwrap_or(0)
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
    /// What each extension opened this run has cost, slowest first.
    pub extensions: Vec<Opening>,
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
    extensions: BTreeMap<String, BothWays>,
    /// Extensions somebody has pressed Enter on that have not drawn yet.
    ///
    /// Keyed by extension rather than by session, and that is deliberate. The
    /// clock has to start before the launch does, because the first thing the
    /// launch produces is the session id and the extension's first screen can
    /// reach the window before the call that started it has returned. There is
    /// no id to key by at the moment the clock starts, so the thing being
    /// opened is the key.
    ///
    /// The cost of that choice: two commands of the same extension opened
    /// within one activation of each other share an entry, and the second one
    /// to start wins. That is a slightly wrong number in a case somebody has
    /// to work at, against no number at all in the ordinary one.
    ///
    /// Bounded by the same limit as the map above, so a name arriving from a
    /// manifest cannot grow this without end. An entry is taken when the
    /// extension draws, so an opening that never draws leaves one behind until
    /// that extension is opened again, which is what makes the bound matter.
    opening: BTreeMap<String, (Instant, Start)>,
}

/// One extension's cost, kept apart by whether Node had to start.
#[derive(Debug, Clone, Copy, Default)]
struct BothWays {
    cold: Adding,
    warm: Adding,
    /// The most any of its commands was holding when it was closed.
    held_bytes: Option<u64>,
}

impl BothWays {
    fn add(&mut self, took: Duration, start: Start) {
        match start {
            Start::Cold => self.cold.add(took),
            Start::Warm => self.warm.add(took),
        }
    }

    /// Nothing where nothing was measured, rather than a row of zeroes.
    fn named(&self, name: &str) -> Opening {
        Opening {
            name: name.to_string(),
            cold: (self.cold.count > 0).then(|| self.cold.named(name)),
            warm: (self.warm.count > 0).then(|| self.warm.named(name)),
            held_bytes: self.held_bytes,
        }
    }
}

/// The timings, held as managed state rather than in a static.
///
/// A static would be a fifth singleton in a codebase that has written down
/// that it does not want them, for something that is per-application by
/// nature.
///
/// A handle rather than the thing itself, so it can be handed to the extension
/// API layer as well as being managed for the commands. **Both halves of an
/// extension's opening are recorded in different places**: the launcher's
/// action starts the clock, and the layer that hears the extension's first
/// render stops it. Two `Timings` would mean the panel showed openings nobody
/// had and the layer recorded openings nobody could see.
///
/// Cloning is what a handle is for and is not a second copy of anything. The
/// alternative was managing an `Arc<Timings>` and changing eight command
/// signatures to say so, which puts the wrapper in every reader's way to solve
/// a problem one type can solve on its own.
#[derive(Clone)]
pub struct Timings {
    inner: Arc<Mutex<Inner>>,
}

impl Default for Timings {
    fn default() -> Self {
        Self::new()
    }
}

impl Timings {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
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

    /// Somebody pressed Enter on one of an extension's commands.
    ///
    /// The clock starts here and not one step later. What follows before Node
    /// is even asked is a manifest read, the extension's saved preferences and
    /// a check that the required ones are filled in, and all of it is time
    /// somebody spends looking at a launcher that has not moved. A measurement
    /// that started after it would report a fast open on exactly the occasions
    /// that felt slow.
    pub fn opening_began(&self, extension: &str, start: Start) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };

        // Looked up before inserting, so a full map still re-times the
        // extensions already in it rather than going silent.
        if inner.opening.contains_key(extension) || inner.opening.len() < MOST_EXTENSIONS {
            inner
                .opening
                .insert(extension.to_string(), (Instant::now(), start));
        }
    }

    /// One of an extension's commands was closed, holding this much.
    ///
    /// The largest is kept rather than the last. An extension whose list has
    /// been narrowed to one row is holding almost nothing at the moment
    /// somebody closes it, and reporting that would say the extension was
    /// cheap because of how the person happened to leave it.
    ///
    /// An extension nobody has opened this run gets a row here, which is the
    /// same bound as everything else on this type: the key comes from a
    /// manifest, and nothing that grows from a name may grow without end.
    pub fn held_on_closing(&self, extension: &str, bytes: u64) {
        let Ok(mut inner) = self.inner.lock() else {
            return;
        };

        let room = inner.extensions.len() < MOST_EXTENSIONS;

        if let Some(both) = inner.extensions.get_mut(extension) {
            both.held_bytes = Some(both.held_bytes.unwrap_or(0).max(bytes));
        } else if room {
            inner
                .extensions
                .entry(extension.to_string())
                .or_default()
                .held_bytes = Some(bytes);
        }
    }

    /// The extension put something on screen, which is when the wait ends.
    ///
    /// **The first thing the person can see**, and nothing earlier. The
    /// launch call returns as soon as the worker has been told to start, long
    /// before the extension's module body has been evaluated or its first list
    /// built, and for the heavy extensions that evaluation is most of the
    /// wait. Timing to the call returning would have reported every extension
    /// as costing about the same, which is the answer that made the question
    /// worth asking.
    ///
    /// Called more than once per opening, because an extension re-renders. The
    /// entry is taken by the first one, so the rest cost a lookup that misses.
    pub fn opening_showed(&self, extension: &str) {
        let recorded = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };

            let Some((began, start)) = inner.opening.remove(extension) else {
                return;
            };

            let took = began.elapsed();

            if let Some(both) = inner.extensions.get_mut(extension) {
                both.add(took, start);
            } else if inner.extensions.len() < MOST_EXTENSIONS {
                inner
                    .extensions
                    .entry(extension.to_string())
                    .or_default()
                    .add(took, start);
            }

            took
        };

        crate::detail!("{extension} was on screen in {} ms", recorded.as_millis());
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
            extensions: slowest_first(
                inner
                    .extensions
                    .iter()
                    .map(|(name, both)| both.named(name))
                    .collect(),
            ),
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

/// Slowest to open first.
///
/// Time rather than memory, and one order rather than two, because a table has
/// one. Time is the figure every measured extension has: memory is only known
/// for one that has been closed or is running right now, so ordering by it
/// would put half the rows in an order decided by whether somebody happened to
/// press Escape. Which extension is expensive is said in words above the
/// table, on whichever of the two it is actually expensive on.
///
/// Ties break on the name so two extensions that cost the same do not swap
/// places between two readings.
pub fn slowest_first(mut openings: Vec<Opening>) -> Vec<Opening> {
    openings.sort_by(|a, b| {
        b.typical_us()
            .cmp(&a.typical_us())
            .then_with(|| a.name.cmp(&b.name))
    });
    openings
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

    /// One open, from Enter to the screen.
    fn opened(timings: &Timings, extension: &str, start: Start) {
        timings.opening_began(extension, start);
        timings.opening_showed(extension);
    }

    /// An extension's name arrives from a manifest, so the list is bounded.
    #[test]
    fn extensions_cannot_grow_the_list_without_end() {
        let timings = Timings::new();

        for at in 0..(MOST_EXTENSIONS + 20) {
            opened(&timings, &format!("extension-{at}"), Start::Warm);
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
            opened(&timings, &format!("extension-{at}"), Start::Warm);
        }

        // The one that does not fit.
        opened(&timings, "one-too-many", Start::Warm);
        // And then one that is already in.
        opened(&timings, "extension-0", Start::Warm);

        let report = timings.report();
        let first = report
            .extensions
            .iter()
            .find(|opening| opening.name == "extension-0")
            .expect("extension-0 was recorded before the list filled");

        assert_eq!(
            first.warm.as_ref().expect("it was opened warm").count,
            2,
            "a full list stopped counting an extension already in it"
        );
        assert!(report.extensions.iter().all(|c| c.name != "one-too-many"));
    }

    /// Nothing measured says nothing, rather than a zero somebody would quote.
    #[test]
    fn a_session_that_searched_nothing_reports_no_sources() {
        let report = Timings::new().report();

        assert!(report.sources.is_empty());
        assert!(report.extensions.is_empty());
    }

    /// The two kinds of open are kept apart.
    ///
    /// Folding them together would average a Node process start with a thread
    /// start, and the answer would describe neither. An extension only ever
    /// opened one way says nothing about the other, rather than zero.
    #[test]
    fn a_cold_open_and_a_warm_one_are_different_numbers() {
        let timings = Timings::new();

        opened(&timings, "emoji", Start::Cold);
        opened(&timings, "emoji", Start::Warm);
        opened(&timings, "emoji", Start::Warm);
        opened(&timings, "uuid-generator", Start::Warm);

        let report = timings.report();
        let emoji = report
            .extensions
            .iter()
            .find(|it| it.name == "emoji")
            .expect("emoji was opened");

        assert_eq!(emoji.cold.as_ref().expect("opened cold once").count, 1);
        assert_eq!(emoji.warm.as_ref().expect("opened warm twice").count, 2);

        let uuid = report
            .extensions
            .iter()
            .find(|it| it.name == "uuid-generator")
            .expect("uuid-generator was opened");

        assert!(
            uuid.cold.is_none(),
            "an extension never opened cold was given a cold figure anyway"
        );
    }

    /// The most it ever held, not the last thing it held.
    ///
    /// A list narrowed to one row on the way out is an extension holding
    /// almost nothing at the moment somebody closes it, and reporting that
    /// would say the extension was cheap because of how the person happened to
    /// leave it.
    #[test]
    fn the_memory_kept_is_the_worst_reading_not_the_latest() {
        let timings = Timings::new();

        timings.held_on_closing("emoji", 63 * 1024 * 1024);
        timings.held_on_closing("emoji", 12 * 1024 * 1024);

        let report = timings.report();
        let emoji = report
            .extensions
            .iter()
            .find(|it| it.name == "emoji")
            .expect("closing an extension records it even if it was never timed");

        assert_eq!(emoji.held_bytes, Some(63 * 1024 * 1024));
        assert!(
            !emoji.timed(),
            "a closing reading is not an opening and must not read as one"
        );
    }

    /// An open that never put anything on screen is not an open.
    ///
    /// The clock is started by pressing Enter and stopped by the extension
    /// drawing. A command that dies while its modules load never draws, and
    /// recording something for it would put a number on the panel that says
    /// the extension works.
    #[test]
    fn an_extension_that_never_drew_is_not_measured() {
        let timings = Timings::new();

        timings.opening_began("never-draws", Start::Cold);

        assert!(
            timings.report().extensions.is_empty(),
            "an extension that never drew was recorded as having opened"
        );
    }

    /// The slowest one is named first, because that is the question.
    ///
    /// Compared on the warm figure, which is what one extension does and
    /// another does not. Cold time is mostly Node starting, which every
    /// extension pays equally and none of them causes, so ordering by it would
    /// rank extensions by which one somebody happened to open first.
    #[test]
    fn the_expensive_one_is_named_first() {
        let quick = Opening {
            name: "uuid-generator".to_string(),
            cold: None,
            warm: Some(Cost {
                name: "uuid-generator".to_string(),
                count: 1,
                total_us: 40_000,
                slowest_us: 40_000,
            }),
            held_bytes: None,
        };
        let slow = Opening {
            name: "emoji".to_string(),
            // A cold figure that would win the comparison if cold counted.
            cold: Some(Cost {
                name: "emoji".to_string(),
                count: 1,
                total_us: 9_000_000,
                slowest_us: 9_000_000,
            }),
            warm: Some(Cost {
                name: "emoji".to_string(),
                count: 1,
                total_us: 900_000,
                slowest_us: 900_000,
            }),
            held_bytes: None,
        };
        let unopened_warm = Opening {
            name: "hacker-news".to_string(),
            cold: Some(Cost {
                name: "hacker-news".to_string(),
                count: 1,
                total_us: 300_000,
                slowest_us: 300_000,
            }),
            warm: None,
            held_bytes: None,
        };

        let order: Vec<String> = slowest_first(vec![quick, unopened_warm, slow])
            .into_iter()
            .map(|it| it.name)
            .collect();

        assert_eq!(order, ["emoji", "hacker-news", "uuid-generator"]);
    }
}
