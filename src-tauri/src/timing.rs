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
//! ## Why this costs nothing
//!
//! Two instants and a push onto a bounded queue, on a path a person triggers.
//! Nothing runs on a timer and nothing is measured while the launcher is
//! closed, which is the state it is in nearly all the time.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

/// How many summons are kept.
///
/// Enough to see a distribution rather than an anecdote, few enough that the
/// whole thing is a few hundred bytes. The one before last is what somebody
/// means by "it felt slow just then".
const KEPT: usize = 20;

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
}

#[derive(Default)]
struct Inner {
    /// When the summon in flight began, if there is one.
    began: Option<Instant>,
    /// Whether the summon in flight has been shown but not yet painted.
    awaiting_paint: bool,
    kept: VecDeque<Summon>,
    cold_start: Option<Duration>,
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

    pub fn report(&self) -> Report {
        let Ok(inner) = self.inner.lock() else {
            return Report {
                cold_start_ms: None,
                summons: Vec::new(),
                median_ms: None,
            };
        };

        let summons: Vec<Summon> = inner.kept.iter().copied().collect();

        Report {
            cold_start_ms: inner.cold_start.map(|d| d.as_millis()),
            median_ms: median(&summons),
            summons,
        }
    }
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
}
