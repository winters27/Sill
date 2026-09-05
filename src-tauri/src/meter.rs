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
    /// What Sill itself costs, across every process it is responsible for.
    ///
    /// Shown because it is the claim the whole project makes, and counted as
    /// the whole tree because that is the honest number: the renderers, the
    /// GPU process and the crash handler are all running because Sill is, and
    /// they go when it does. Reporting only the Rust core said 86 MB where the
    /// truth was 827 across eleven processes, which is the sort of number a
    /// person checks in Task Manager the first time they doubt it.
    pub sill: u64,
    /// How many processes that is.
    pub sill_processes: usize,
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

/// The heaviest programs, with every process counted against its own.
///
/// Grouped rather than listed. A launcher built on a webview is not one
/// process, so a list of processes shows several rows called
/// `msedgewebview2.exe` with nothing saying which application each belongs to,
/// and the reader has no way to tell one application's renderers from
/// another's. Reported from a machine where that added up to 1,367 MB of
/// apparent "WebView2", most of it a different program entirely.
///
/// Naming each row after its owner without also summing them would be worse:
/// seven rows that all say the same application, none of them its real cost.
///
/// The name and the icon come from the owning process when it is in the list.
/// It usually is, being the largest thing in its own tree, but a program whose
/// main process would not open still gets a row under whatever its heaviest
/// part is called, which is the honest fallback rather than dropping it.
fn consumers(
    running: &[crate::processes::Process],
    // Passed in rather than looked up here, so the grouping can be tested
    // against a machine that does not exist. Working out who owns what needs
    // Windows; deciding what to do about it does not.
    owners: &std::collections::HashMap<u32, u32>,
    share: impl Fn(u32) -> f32,
) -> Vec<Consumer> {
    use std::collections::HashMap;

    let by_pid: HashMap<u32, &crate::processes::Process> =
        running.iter().map(|p| (p.pid, p)).collect();

    // The heaviest member is carried alongside the totals, because the owning
    // process is not always in the list: anything running as another user or
    // as the system refuses to open, and its renderers are then a group whose
    // owner cannot be named. Naming that group after its largest part is the
    // honest answer. The first version printed the raw process id, which would
    // have put a row called `4060` in the readout.
    let mut totals: HashMap<u32, (u64, f32, u32)> = HashMap::new();
    for process in running {
        let owner = owners.get(&process.pid).copied().unwrap_or(process.pid);
        let entry = totals.entry(owner).or_insert((0, 0.0, process.pid));
        entry.0 += process.bytes;
        entry.1 += share(process.pid);

        if by_pid
            .get(&entry.2)
            .is_none_or(|heaviest| process.bytes > heaviest.bytes)
        {
            entry.2 = process.pid;
        }
    }

    let mut out: Vec<Consumer> = totals
        .into_iter()
        .map(|(owner, (bytes, cpu, heaviest))| {
            // The owner when it is there, its largest part when it is not.
            let named = by_pid
                .get(&owner)
                .or_else(|| by_pid.get(&heaviest))
                .copied();

            Consumer {
                name: named.map_or_else(|| String::from("something that would not open"), |p| {
                    p.name.clone()
                }),
                bytes,
                cpu,
                path: named.and_then(|p| p.path.clone()),
            }
        })
        .collect();

    // Heaviest first, and the name breaks a tie so the order does not shuffle
    // between two readings that happen to match.
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    out.truncate(NAMED);
    out
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
        let mine = crate::processes::tree_of(std::process::id());

        Reading {
            cpu,
            memory_used: used,
            memory_total: total,
            count: running.len(),
            top: consumers(&running, &crate::processes::owners(&running), &share),
            sill: running
                .iter()
                .filter(|p| mine.contains(&p.pid))
                .map(|p| p.bytes)
                .sum(),
            sill_processes: running.iter().filter(|p| mine.contains(&p.pid)).count(),
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

    /// A machine with two applications, each behind several engine processes.
    ///
    /// Deliberately shaped like the report that caused this: the engine
    /// processes are the heaviest things running, so a list of processes would
    /// be nothing but rows called `msedgewebview2.exe`, and the two
    /// applications would be indistinguishable.
    fn two_applications() -> (Vec<crate::processes::Process>, std::collections::HashMap<u32, u32>) {
        let at = |pid: u32, name: &str, bytes: u64| crate::processes::Process {
            pid,
            name: name.to_string(),
            path: Some(format!("C:/{name}")),
            bytes,
            visible: false,
        };

        let running = vec![
            at(1, "sill.exe", 30),
            at(2, "msedgewebview2.exe", 100),
            at(3, "msedgewebview2.exe", 40),
            at(10, "Other.exe", 20),
            at(11, "msedgewebview2.exe", 300),
            at(20, "notepad.exe", 5),
        ];

        // What `processes::owners` works out on a real machine.
        let owners = std::collections::HashMap::from([
            (1, 1),
            (2, 1),
            (3, 1),
            (10, 10),
            (11, 10),
            (20, 20),
        ]);

        (running, owners)
    }

    #[test]
    fn every_process_is_counted_against_the_program_that_owns_it() {
        let (running, owners) = two_applications();
        let top = consumers(&running, &owners, |_| 0.0);

        // Heaviest first: Other.exe with 320 over sill.exe with 170, even
        // though sill.exe owns more processes and the single largest process
        // on the machine belongs to Other.
        let named: Vec<(&str, u64)> = top.iter().map(|c| (c.name.as_str(), c.bytes)).collect();
        assert_eq!(
            named,
            vec![("Other.exe", 320), ("sill.exe", 170), ("notepad.exe", 5)],
        );
    }

    #[test]
    fn no_row_is_named_after_an_engine() {
        // The whole complaint. A reader must never be shown a row whose name
        // is the engine rather than the application.
        let (running, owners) = two_applications();

        for one in consumers(&running, &owners, |_| 0.0) {
            assert!(
                !one.name.contains("msedgewebview2"),
                "a row was named after the engine: {}",
                one.name,
            );
        }
    }

    #[test]
    fn the_cost_of_every_process_reaches_its_program() {
        // Summed, not sampled. Naming each row after its owner but showing one
        // process's figure would be a quieter version of the same bug.
        let (running, owners) = two_applications();
        let top = consumers(&running, &owners, |pid| if pid == 11 { 40.0 } else { 1.0 });

        let other = top.iter().find(|c| c.name == "Other.exe").expect("Other.exe");
        // 40 for the engine plus 1 for the application itself.
        assert_eq!(other.cpu, 41.0);
        assert_eq!(other.bytes, 320);

        let sill = top.iter().find(|c| c.name == "sill.exe").expect("sill.exe");
        assert_eq!(sill.cpu, 3.0);
    }

    #[test]
    fn a_program_that_would_not_open_still_gets_a_row() {
        // Its main process is missing from the list, which is ordinary for
        // anything running as another user. Dropping the row would hide real
        // memory; naming it after its heaviest part is the honest fallback.
        let running = vec![crate::processes::Process {
            pid: 2,
            name: "msedgewebview2.exe".to_string(),
            path: None,
            bytes: 100,
            visible: false,
        }];
        let owners = std::collections::HashMap::from([(2, 999)]);

        let top = consumers(&running, &owners, |_| 0.0);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].bytes, 100);

        // Named after its largest part. The first version printed the owner's
        // process id here, which would have put a row called `999` on screen,
        // and the original version of this test could not tell the difference
        // because it only counted rows.
        assert_eq!(top[0].name, "msedgewebview2.exe");
        assert!(
            top[0].name.parse::<u32>().is_err(),
            "a row was named after a process id: {}",
            top[0].name,
        );
    }

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
