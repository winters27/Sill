//! What the machine is doing right now.
//!
//! The reading half of P2.6. [`crate::processes`] says what is running;
//! this says what it is costing, which is a different question because it
//! needs two readings and the gap between them.
//!
//! ## Why anything is remembered at all
//!
//! Processor time is a total that only ever rises, so a single reading says
//! how busy something has been since it started, which nobody is asking.
//! "Right now" is the difference between two readings divided by the time
//! between them, and that is the only reason this holds any state.
//!
//! It is forgotten when the view closes. A reading taken against a sample from
//! an hour ago would average an hour, and a live figure that is secretly an
//! hourly mean is worse than no figure.

use serde::Serialize;

/// One reading of what the machine is doing.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    /// Percent of all cores together, 0 to 100.
    pub cpu: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    /// How many programs are running.
    pub count: usize,
    /// The heaviest few, for the bars.
    pub top: Vec<Consumer>,
    /// What Sill itself costs.
    ///
    /// Shown because it is the claim the whole project makes. A launcher that
    /// says it idles at almost nothing should be the easiest thing on the list
    /// to check.
    pub sill: u64,
}

/// One program in the readout.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Consumer {
    pub name: String,
    pub bytes: u64,
    /// Percent of all cores since the previous reading.
    pub cpu: f32,
    pub path: Option<String>,
}

/// How many programs the readout names.
///
/// A widget, not an audit. Five is enough to see what is unusual and few
/// enough to read without scrolling.
const NAMED: usize = 5;

/// Holds the previous reading so the next one can subtract it.
#[derive(Default)]
pub struct Meter {
    inner: std::sync::Mutex<Option<Sample>>,
}

struct Sample {
    at: std::time::Instant,
    /// Processor time per process, in 100-nanosecond units.
    per_process: std::collections::HashMap<u32, u64>,
    system_busy: u64,
    system_total: u64,
}

/// Busy over total, as a percentage, given two readings.
///
/// Its own function because it is the arithmetic worth being sure about and it
/// needs no machine to check: a window of zero must not divide, and a counter
/// that appears to go backwards (which happens across a sleep) must read as
/// nothing rather than as a huge number.
pub fn percent(busy: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }

    ((busy as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
}

impl Meter {
    /// Forgets the previous reading.
    pub fn forget(&self) {
        if let Ok(mut held) = self.inner.lock() {
            *held = None;
        }
    }

    /// Reads the machine now, against whatever was read last.
    ///
    /// The first reading after a forget has nothing to subtract, so its
    /// processor figures are zero. That is honest rather than a placeholder:
    /// there is genuinely no answer yet, and inventing one would put a number
    /// on screen that describes no interval.
    #[cfg(windows)]
    pub fn read(&self) -> Reading {
        let now = std::time::Instant::now();
        let (system_busy, system_total) = system_times();
        let running = crate::processes::running();

        let mut per_process = std::collections::HashMap::new();
        for process in &running {
            if let Some(time) = processor_time(process.pid) {
                per_process.insert(process.pid, time);
            }
        }

        let previous = self.inner.lock().ok().and_then(|mut held| {
            held.replace(Sample {
                at: now,
                per_process: per_process.clone(),
                system_busy,
                system_total,
            })
        });

        // The whole machine, from the two system totals rather than by adding
        // the processes up: what the processes account for leaves out the
        // kernel's own time, and a figure that never reaches 100% while the
        // machine is pinned is one nobody believes.
        let cpu = previous
            .as_ref()
            .map(|was| {
                percent(
                    system_busy.saturating_sub(was.system_busy),
                    system_total.saturating_sub(was.system_total),
                )
            })
            .unwrap_or(0.0);

        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as f64)
            .unwrap_or(1.0);

        // The processor time available to everything between the two
        // readings, in the same 100-nanosecond units the counters use.
        let window = previous
            .as_ref()
            .map(|was| now.duration_since(was.at).as_secs_f64() * 10_000_000.0 * cores)
            .unwrap_or(0.0);

        let share = |pid: u32| -> f32 {
            let Some(was) = previous.as_ref().and_then(|s| s.per_process.get(&pid)) else {
                return 0.0;
            };

            let spent = per_process
                .get(&pid)
                .copied()
                .unwrap_or(0)
                .saturating_sub(*was);

            percent(spent, window as u64)
        };

        let (used, total) = memory();
        let mine = std::process::id();

        Reading {
            cpu,
            memory_used: used,
            memory_total: total,
            count: running.len(),
            top: running
                .iter()
                .take(NAMED)
                .map(|process| Consumer {
                    name: process.name.clone(),
                    bytes: process.bytes,
                    cpu: share(process.pid),
                    path: process.path.clone(),
                })
                .collect(),
            sill: running
                .iter()
                .find(|p| p.pid == mine)
                .map(|p| p.bytes)
                .unwrap_or(0),
        }
    }

    #[cfg(not(windows))]
    pub fn read(&self) -> Reading {
        Reading::default()
    }
}

/// Busy and total processor time for the machine, in 100-nanosecond units.
#[cfg(windows)]
fn system_times() -> (u64, u64) {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;

    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: three owned structures, written by the call.
    if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_err() {
        return (0, 0);
    }

    let whole = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;

    // Kernel time already includes idle, which is why busy is the difference
    // rather than kernel plus user.
    let total = whole(kernel) + whole(user);
    (total.saturating_sub(whole(idle)), total)
}

/// Processor time one process has used, in 100-nanosecond units.
#[cfg(windows)]
fn processor_time(pid: u32) -> Option<u64> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // SAFETY: four owned structures, and the handle is closed on both paths.
    let ok = unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    unsafe {
        let _ = CloseHandle(handle);
    }

    ok.ok()?;

    let whole = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
    Some(whole(kernel) + whole(user))
}

/// Physical memory in use, and how much there is.
#[cfg(windows)]
fn memory() -> (u64, u64) {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };

    // SAFETY: the structure declares its own size, which is what the call
    // reads to know which version it was handed.
    if unsafe { GlobalMemoryStatusEx(&mut status) }.is_err() {
        return (0, 0);
    }

    (
        status.ullTotalPhys.saturating_sub(status.ullAvailPhys),
        status.ullTotalPhys,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_half_busy_interval_reads_as_half() {
        assert_eq!(percent(50, 100), 50.0);
        assert_eq!(percent(0, 100), 0.0);
        assert_eq!(percent(100, 100), 100.0);
    }

    /// The first reading has no interval behind it.
    #[test]
    fn no_interval_is_nothing_rather_than_a_division() {
        assert_eq!(percent(0, 0), 0.0);
        assert_eq!(percent(500, 0), 0.0, "a window of zero must not divide");
    }

    /// Counters can appear to go backwards across a sleep or a core change,
    /// and the callers subtract with `saturating_sub`, so this sees zero. What
    /// it must never do is report more than the machine has.
    #[test]
    fn nothing_ever_reads_above_a_full_machine() {
        assert_eq!(percent(200, 100), 100.0, "clamped rather than 200%");
    }
}
