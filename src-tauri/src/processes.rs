//! What is running, and what it is costing.
//!
//! P2.6. The audit called this the efficiency philosophy made into a feature,
//! and that is the right way to read it: Sill's claim is that it idles at
//! almost nothing, and a launcher that says so should be able to show you what
//! everything else is doing.
//!
//! ## Enumerated when asked, never at rest
//!
//! A process list is wrong the moment anything starts or stops, so there is
//! nothing to cache and caching it would cost more than rebuilding it. It sits
//! behind a row of its own for the reason the app volume list does: the root
//! list runs on every keystroke whether or not anybody asked about processes,
//! and walking every process on the machine is not something to do because
//! somebody typed the letter p.
//!
//! ## Working set, not "memory"
//!
//! What Task Manager's Memory column shows is the working set, and it is the
//! number people recognise, so it is the number shown. It is not the same as
//! what a process has committed, and neither is a good measure of what would
//! be freed by closing it: shared pages are counted for every process holding
//! them. The point here is to find the one that is unreasonable, which this
//! answers well enough, rather than to audit anybody's allocator.

use serde::Serialize;

/// One running program.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Process {
    pub pid: u32,
    /// The executable's own name, without its path.
    pub name: String,
    /// Where it was launched from, when that can be read.
    ///
    /// A process running as another user, or as the system, refuses to say,
    /// and that is ordinary rather than an error.
    pub path: Option<String>,
    /// Working set, in bytes.
    pub bytes: u64,
    /// Whether it has a window somebody could be looking at.
    ///
    /// What separates "Firefox, which you are using" from the forty service
    /// processes nobody has ever seen. The list leads with these because they
    /// are the ones anybody means to quit.
    pub visible: bool,
}

/// Everything running, heaviest first.
///
/// Sorted by what it costs rather than by name, because the question this
/// answers is "what is eating my machine" and an alphabetical answer to that
/// is a list somebody has to read all of.
#[cfg(windows)]
pub fn running() -> Vec<Process> {
    let owners = windowed_pids();
    let mut out = Vec::new();

    for pid in pids() {
        // Nothing useful can be said about a process that will not open, and
        // that is most of the system's own. Skipped rather than listed as a
        // row of blanks.
        let Some((name, path, bytes)) = describe(pid) else {
            continue;
        };

        out.push(Process {
            pid,
            name,
            path,
            bytes,
            visible: owners.contains(&pid),
        });
    }

    // By weight alone. Windowed first was tried and is wrong for the question
    // this answers: it put a 1.2 GB background process below a 9 MB one that
    // happened to own a window, and the thing eating the machine is the thing
    // somebody came here to find. Whether it has a window is shown on the row
    // instead, which is where that belongs.
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.name.cmp(&b.name)));
    out
}

/// Every process id on the machine.
#[cfg(windows)]
fn pids() -> Vec<u32> {
    use windows::Win32::System::ProcessStatus::K32EnumProcesses;

    // Grown until it is not filled, which is how this call says "there were
    // more". A fixed buffer silently truncates, and the process you were
    // looking for is the one that did not fit.
    let mut capacity = 1_024usize;

    loop {
        let mut buffer = vec![0u32; capacity];
        let mut needed = 0u32;

        // SAFETY: the buffer and its size in bytes are passed together, and
        // `needed` is written by the call.
        let ok = unsafe {
            K32EnumProcesses(
                buffer.as_mut_ptr(),
                (buffer.len() * std::mem::size_of::<u32>()) as u32,
                &mut needed,
            )
        };

        if !ok.as_bool() {
            return Vec::new();
        }

        let returned = needed as usize / std::mem::size_of::<u32>();

        if returned < buffer.len() {
            buffer.truncate(returned);
            return buffer;
        }

        capacity *= 2;

        // A machine with a million processes is a machine with a problem this
        // list is not going to solve.
        if capacity > 1 << 20 {
            return buffer;
        }
    }
}

/// A process's name, path and working set, if it will say.
#[cfg(windows)]
fn describe(pid: u32) -> Option<(String, Option<String>, u64)> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, K32GetModuleFileNameExW, PROCESS_MEMORY_COUNTERS,
        PROCESS_MEMORY_COUNTERS_EX2,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    // Limited information is what a normal process is allowed to ask about
    // another. Asking for more fails on anything running as the system, which
    // is most of what a full list contains.
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        )
    }
    .or_else(|_| unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    .ok()?;

    let mut wide = [0u16; 512];
    // SAFETY: the buffer is owned here and its length is passed with it.
    let written = unsafe { K32GetModuleFileNameExW(Some(handle), None, &mut wide) } as usize;

    let path = (written > 0).then(|| String::from_utf16_lossy(&wide[..written]));

    /*
     * The **private** working set, not the working set.
     *
     * `WorkingSetSize` includes pages shared with other processes, and a
     * browser engine shares a great deal of its code. Adding it up across the
     * seven processes one application runs counts the shared half seven times:
     * this readout claimed 561 MB for a tree that Task Manager, which shows
     * the private figure, put at 142.6 MB. It was reported as the widget
     * reading the wrong amount, and it was.
     *
     * `PROCESS_MEMORY_COUNTERS_EX2` carries exactly the number Task Manager
     * shows. It needs Windows 10 2004 or newer, and the call says which
     * version it was handed through `cb`, so an older one simply fails and is
     * fallen back on below rather than being guessed at.
     */
    let mut counters = PROCESS_MEMORY_COUNTERS_EX2 {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
        ..Default::default()
    };

    // SAFETY: the struct declares its own size, which is what the call reads
    // to know which version it was handed. The pointer cast is what the API
    // asks for: every version of these counters begins with the same header.
    let measured = unsafe {
        GetProcessMemoryInfo(
            handle,
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2 as *mut PROCESS_MEMORY_COUNTERS,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32,
        )
    };

    // SAFETY: the handle came from OpenProcess and is not used again.
    unsafe {
        let _ = CloseHandle(handle);
    }

    if measured.is_err() {
        return None;
    }

    let name = path
        .as_deref()
        .and_then(file_name_of)
        .unwrap_or_else(|| format!("pid {pid}"));

    /*
     * The working set is the fallback, not the answer.
     *
     * `PrivateWorkingSetSize` is zero on a Windows older than 2004, where the
     * call filled in only the part it understood. Over-reporting is better
     * than reporting nothing, and the header is always there.
     */
    let bytes = if counters.PrivateWorkingSetSize > 0 {
        counters.PrivateWorkingSetSize as u64
    } else {
        counters.WorkingSetSize as u64
    };

    Some((name, path, bytes))
}

/// Who started whom, for every process on the machine.
///
/// `K32EnumProcesses` gives ids and nothing else, so the parent has to come
/// from a ToolHelp snapshot. It is needed for one question and it is a question
/// worth answering: **what a program costs is what its whole tree costs.**
#[cfg(windows)]
fn parents() -> std::collections::HashMap<u32, u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let mut out = std::collections::HashMap::new();

    // SAFETY: the snapshot handle is closed on every path out, and the entry
    // declares its own size, which is what the calls read.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return out;
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                out.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }

    out
}

/// Every process descended from `root`, including it.
///
/// A launcher built on a webview is not one process and saying so would be a
/// convenient lie: the renderers, the GPU process and the crash handler are
/// all there because Sill is running, and they go when it does.
#[cfg(windows)]
/// Names that are an engine rather than a program.
///
/// A row saying `msedgewebview2.exe` tells a reader nothing. The same name
/// covers this launcher, every other application on the machine built the same
/// way, and parts of Windows, so several of them at once read as one enormous
/// anonymous consumer. That is how it was reported: 1,367 MB of "WebView2" on
/// one machine, most of it somebody else's application.
const ENGINES: &[&str] = &["msedgewebview2.exe", "chrome_crashpad_handler.exe"];

fn is_engine(name: &str) -> bool {
    ENGINES.iter().any(|one| name.eq_ignore_ascii_case(one))
}

/// Which program each process belongs to, as a map from one to the other.
///
/// A process that is a program in its own right owns itself. One that is an
/// engine is walked up its parent chain until something that is not an engine
/// is found, so a renderer is counted under the application that started it.
///
/// Built once for a whole reading rather than asked per process, because the
/// parent map costs a pass over every process on the machine and asking five
/// hundred times would be five hundred of those.
///
/// Bounded like `tree_of` is: a reused process id can point a chain at itself,
/// and a walk with no bound would not come back.
#[cfg(windows)]
pub fn owners(of: &[Process]) -> std::collections::HashMap<u32, u32> {
    use std::collections::HashMap;

    let parents = parents();
    let named: HashMap<u32, &str> = of.iter().map(|p| (p.pid, p.name.as_str())).collect();
    let mut out = HashMap::with_capacity(of.len());

    for process in of {
        let mut at = process.pid;

        if !is_engine(&process.name) {
            out.insert(process.pid, at);
            continue;
        }

        for _ in 0..64 {
            let Some(&parent) = parents.get(&at) else {
                break;
            };

            match named.get(&parent) {
                // The program. This is the name somebody recognises.
                Some(name) if !is_engine(name) => {
                    at = parent;
                    break;
                }
                // Another engine process: keep going up.
                Some(_) => at = parent,
                // The parent would not open, which is most of the system's
                // own. Nothing better is available than where we started.
                None => break,
            }
        }

        out.insert(process.pid, at);
    }

    out
}

#[cfg(not(windows))]
pub fn owners(of: &[Process]) -> std::collections::HashMap<u32, u32> {
    of.iter().map(|p| (p.pid, p.pid)).collect()
}

pub fn tree_of(root: u32) -> std::collections::HashSet<u32> {
    use std::collections::HashSet;

    let parents = parents();
    let mut found: HashSet<u32> = HashSet::new();
    found.insert(root);

    // Walked from every process up to its ancestors rather than down from the
    // root, because the map is child to parent. Bounded by the chain's own
    // length so a cycle, which a reused id can produce, cannot spin.
    for &pid in parents.keys() {
        let mut at = pid;

        for _ in 0..64 {
            let Some(&parent) = parents.get(&at) else {
                break;
            };

            if parent == root || found.contains(&parent) {
                found.insert(pid);
                break;
            }

            if parent == 0 || parent == at {
                break;
            }

            at = parent;
        }
    }

    found
}

#[cfg(not(windows))]
pub fn tree_of(root: u32) -> std::collections::HashSet<u32> {
    std::collections::HashSet::from([root])
}

// ------------------------------------------------------------- ending one

/// How long a reading of what is running is reused for.
///
/// The same second the audio sessions and the open windows get, and for the
/// same reason: filtering the list is a keystroke at a time, and typing six
/// letters must not walk every process on the machine six times. Longer than
/// that would start showing a list that disagrees with the desktop.
pub const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(1);

/// What is running, from the reading that is reused for a moment.
pub fn listed(held: &crate::state::Fresh<Vec<Process>>) -> Vec<Process> {
    held.get(running)
}

/// Throws the last reading away.
///
/// Called after something has been quit, because the list is the one thing on
/// screen that is about to be wrong: the row that was just ended would sit
/// there for another second saying it is still running.
pub fn forget(held: &crate::state::Fresh<Vec<Process>>) {
    held.forget();
}

/// Processes Sill will not offer to end.
///
/// Ending any of these does not close a program, it takes the session or the
/// machine with it: Windows treats several of them as critical and bugchecks
/// outright, and `svchost.exe` is whichever handful of services happen to
/// share that host. None of them is ever what somebody scrolling a list of
/// what is using memory meant to click.
///
/// Deliberately short, and it is not a list of things that are inconvenient to
/// close. `explorer.exe` is not here on purpose: restarting it is a thing
/// people do deliberately, Windows brings it back, and refusing would be Sill
/// deciding what somebody may do with their own desktop.
///
/// A name rather than a path, because that is what a row carries and what the
/// check below has to compare against.
pub fn is_protected(name: &str) -> bool {
    const CRITICAL: &[&str] = &[
        "smss.exe",
        "csrss.exe",
        "wininit.exe",
        "winlogon.exe",
        "services.exe",
        "lsass.exe",
        "lsaiso.exe",
        "svchost.exe",
    ];

    let lower = name.to_ascii_lowercase();
    CRITICAL.contains(&lower.as_str())
}

/// The two ids that are not programs.
///
/// 0 is the idle process, which is what a processor does when it has nothing
/// to do, and 4 is the kernel itself. Neither opens, so neither is in the list
/// this draws, and both are refused here anyway because a check that only
/// holds while the list happens to exclude something is not a check.
fn is_the_kernel(pid: u32) -> bool {
    pid == 0 || pid == 4
}

/// Why a process will not be ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// It has already exited.
    Gone,
    /// The id belongs to something else now.
    ///
    /// **The reason this check exists.** Windows reuses process ids, and the
    /// gap between a row being drawn and Enter being pressed on it is long
    /// enough for the program to exit and the number to be handed to something
    /// else. Acting on the id alone means ending whatever inherited it, and
    /// the symptom is indistinguishable from Sill choosing at random.
    Reused { now: String },
    /// It is Sill, or something Sill started.
    ///
    /// The whole tree rather than the one process, because a launcher built on
    /// a webview is not one process: quitting the renderer or the extension
    /// host out of this list would look exactly like Sill crashing.
    ///
    /// It costs something, and the cost is worth writing down. Anything Sill
    /// spawns directly is in that tree, so a script Sill started cannot be
    /// ended from this list. Applications are not affected, because they are
    /// opened through the shell and belong to it rather than to us. The trade
    /// is deliberate: the list refusing one row is a small thing, and the
    /// launcher appearing to crash when somebody quits a row called
    /// `msedgewebview2.exe` is not.
    Ourselves,
    /// Ending it would take the session with it.
    Protected,
    /// Not a program at all.
    TheKernel,
}

impl Refused {
    /// Said to whoever pressed the key, naming what they thought they had.
    pub fn say(&self, was: &str) -> String {
        match self {
            Self::Gone => format!("{was} is not running any more"),
            Self::Reused { now } => {
                format!("That is {now} now, not {was}. Nothing was ended.")
            }
            Self::Ourselves => "Sill will not end itself".to_string(),
            Self::Protected => {
                format!("{was} is part of Windows, and ending it would end the session")
            }
            Self::TheKernel => format!("{was} is not a program that can be ended"),
        }
    }
}

/// Whether the process a row named may be ended.
///
/// Its own function, taking what the machine says rather than reading it, so
/// the rule worth proving can be proved without a process to lose: **an id is
/// never acted on unless it still names what the row said it was.**
///
/// The order is the meaning. Identity is settled before anything decides
/// using a name, because every decision below it is about the wrong program
/// otherwise.
pub fn may_end(pid: u32, was: &str, now: Option<&str>, ours: bool) -> Result<(), Refused> {
    let Some(now) = now else {
        return Err(Refused::Gone);
    };

    if !now.eq_ignore_ascii_case(was) {
        return Err(Refused::Reused {
            now: now.to_string(),
        });
    }

    if is_the_kernel(pid) {
        return Err(Refused::TheKernel);
    }

    if ours {
        return Err(Refused::Ourselves);
    }

    if is_protected(now) {
        return Err(Refused::Protected);
    }

    Ok(())
}

/// What a process id is called right now, if it is called anything.
#[cfg(windows)]
pub fn named(pid: u32) -> Option<String> {
    describe(pid).map(|(name, _, _)| name)
}

#[cfg(not(windows))]
pub fn named(_pid: u32) -> Option<String> {
    None
}

/// [`may_end`], asked of the machine as it is at this moment.
///
/// Both readings are taken here rather than passed in, and both are taken
/// **now** rather than when the row was drawn. That is the whole point: the
/// row is a photograph and this is the check that it still describes anything.
#[cfg(windows)]
fn still_endable(pid: u32, was: &str) -> Result<(), String> {
    let ours = tree_of(std::process::id()).contains(&pid);

    may_end(pid, was, named(pid).as_deref(), ours).map_err(|refused| refused.say(was))
}

/// Asks a program to close, the way its own close button does.
///
/// `WM_CLOSE` to every window it has, never `TerminateProcess`: the program
/// gets to run its shutdown and gets to put up "save changes?" if there is
/// unsaved work. This is what Enter does on a process row, and [`force_quit`]
/// is what it deliberately is not.
///
/// A program with no window cannot be asked, and is told so rather than
/// quietly killed. Falling through to a terminate here would make the safe
/// action and the dangerous one the same key on the rows where it matters
/// most, which is every background process in the list.
///
/// ## Packaged apps answer "no window", and that is a real gap
///
/// A Store app's visible window belongs to `ApplicationFrameHost`, not to the
/// app, so `GetWindowThreadProcessId` names the host and the app looks
/// windowless. Windows 11's own Notepad is one of these, which is how it was
/// found: the probe watched it refuse to close for ten seconds while sitting
/// on screen.
///
/// Left alone rather than worked around here. The same reading is what
/// `Process::visible` and `windowing::list` have always used, so a fix belongs
/// where that enumeration is rather than in one action, and the way this fails
/// is a sentence pointing at Force Quit rather than something happening to the
/// wrong program.
#[cfg(windows)]
pub fn quit(pid: u32, was: &str) -> Result<String, String> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    still_endable(pid, was)?;

    let its_own: Vec<isize> = visible_windows()
        .into_iter()
        .filter(|(owner, _)| *owner == pid)
        .map(|(_, window)| window)
        .collect();

    if its_own.is_empty() {
        return Err(format!(
            "{was} has no window to close. Force Quit ends it outright."
        ));
    }

    let mut asked = 0usize;

    for window in its_own {
        // SAFETY: posts a message to a window the enumeration above just
        // reported and returns immediately.
        let posted = unsafe {
            PostMessageW(
                Some(HWND(window as *mut core::ffi::c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            )
        };

        if posted.is_ok() {
            asked += 1;
        }
    }

    if asked == 0 {
        return Err(format!("{was} would not take the request to close"));
    }

    // Present tense, and honestly so. The program has been asked and may put
    // up a dialog, or may decline; saying it closed would be a claim about
    // something that has not happened yet.
    Ok(format!("Asked {was} to close"))
}

/// Ends a program outright, without asking it.
///
/// `TerminateProcess`, which is the one action here that destroys work: there
/// is no shutdown, no save prompt and no chance to write anything out. It is
/// never what Enter does. It sits in the action panel, below the one that
/// asks, and the ordering is the registry's rather than a comment's: [`quit`]
/// claims the primary for a process row and this does not.
#[cfg(windows)]
pub fn force_quit(pid: u32, was: &str) -> Result<String, String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    still_endable(pid, was)?;

    // SAFETY: the handle is closed on every path out below.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }
        .map_err(|err| format!("Windows would not let Sill end {was}: {err}"))?;

    // SAFETY: the handle came from the call above and carries the right to do
    // this. The exit code is the one Windows itself uses for a killed process.
    let ended = unsafe { TerminateProcess(handle, 1) };

    // SAFETY: the handle came from OpenProcess and is not used again.
    unsafe {
        let _ = CloseHandle(handle);
    }

    ended.map_err(|err| format!("{was} would not end: {err}"))?;

    Ok(format!("Ended {was}"))
}

#[cfg(not(windows))]
pub fn quit(_pid: u32, _was: &str) -> Result<String, String> {
    Err("Only Windows has this.".to_string())
}

#[cfg(not(windows))]
pub fn force_quit(_pid: u32, _was: &str) -> Result<String, String> {
    Err("Only Windows has this.".to_string())
}

/// The last segment of a Windows path.
///
/// Its own function so it can be tested without a process: the interesting
/// cases are a path with no separator and one that ends in one, and neither
/// needs a machine.
pub fn file_name_of(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['\\', '/']);
    let name = trimmed.rsplit(['\\', '/']).next()?;

    // A drive is not a program. A bare `C:\` trims to `C:`, which is a
    // non-empty string and would otherwise be drawn as the name of something
    // running.
    if name.is_empty() || name.ends_with(':') {
        return None;
    }

    Some(name.to_string())
}

/// Every visible top-level window on the desktop, with whose it is.
///
/// One enumeration behind both questions this file asks about windows: which
/// processes own one, and which windows one process owns. They were going to
/// be two walks with two predicates, and two predicates about "has a window"
/// drift: the row would say a process has a window and quitting it would
/// answer that it has none, which reads as the launcher being broken rather
/// than as two functions disagreeing.
#[cfg(windows)]
fn visible_windows() -> Vec<(u32, isize)> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn visit(window: HWND, into: LPARAM) -> BOOL {
        // SAFETY: `into` is the address of the vector below, which outlives
        // the enumeration.
        let found = unsafe { &mut *(into.0 as *mut Vec<(u32, isize)>) };

        if unsafe { IsWindowVisible(window) }.as_bool() {
            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
            if pid != 0 {
                found.push((pid, window.0 as isize));
            }
        }

        true.into()
    }

    let mut found: Vec<(u32, isize)> = Vec::new();

    // SAFETY: the vector outlives the call, which returns before this does.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut _ as isize));
    }

    found
}

/// Which processes own a window somebody could see.
#[cfg(windows)]
fn windowed_pids() -> std::collections::HashSet<u32> {
    visible_windows().into_iter().map(|(pid, _)| pid).collect()
}

#[cfg(not(windows))]
pub fn running() -> Vec<Process> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_the_last_segment_however_the_path_is_written() {
        assert_eq!(
            file_name_of(r"C:\Program Files\App\app.exe").as_deref(),
            Some("app.exe")
        );
        assert_eq!(
            file_name_of("app.exe").as_deref(),
            Some("app.exe"),
            "no separator at all"
        );
        assert_eq!(
            file_name_of("C:/mixed/slashes/app.exe").as_deref(),
            Some("app.exe")
        );
    }

    #[test]
    fn a_path_that_names_nothing_is_nothing_rather_than_an_empty_row() {
        assert_eq!(file_name_of(""), None);
        assert_eq!(file_name_of(r"C:\"), None, "a bare drive names no program");
    }

    /// The one rule this whole check exists for.
    ///
    /// A process id is reused. The row was drawn when 4820 was Notepad, the
    /// key is pressed a minute later, and by then 4820 is a build server. If
    /// the id alone decides, the build server is what gets ended.
    #[test]
    fn an_id_that_has_been_handed_to_something_else_is_refused() {
        let refused = may_end(4820, "notepad.exe", Some("node.exe"), false)
            .expect_err("the id no longer names what the row said");

        assert_eq!(
            refused,
            Refused::Reused {
                now: "node.exe".to_string()
            }
        );
        assert!(
            refused.say("notepad.exe").contains("node.exe"),
            "the message has to name what it actually is: {}",
            refused.say("notepad.exe")
        );
    }

    #[test]
    fn a_process_that_has_already_exited_is_not_an_error_worth_dressing_up() {
        assert_eq!(
            may_end(4820, "notepad.exe", None, false),
            Err(Refused::Gone)
        );
    }

    #[test]
    fn sill_will_not_end_itself_or_anything_it_started() {
        assert_eq!(
            may_end(1234, "sill.exe", Some("sill.exe"), true),
            Err(Refused::Ourselves),
            "the launcher closing itself from its own list is not a feature"
        );
    }

    /// Ending one of these does not close a program, it ends the session.
    #[test]
    fn nothing_windows_needs_is_offered_up() {
        for critical in [
            "smss.exe",
            "csrss.exe",
            "wininit.exe",
            "winlogon.exe",
            "services.exe",
            "lsass.exe",
            "lsaiso.exe",
            "svchost.exe",
        ] {
            assert!(is_protected(critical), "{critical} is not protected");
            assert_eq!(
                may_end(900, critical, Some(critical), false),
                Err(Refused::Protected),
                "{critical} would have been ended",
            );
        }

        // Case is a fact about how the path was written, not about what the
        // program is. `SvcHost.exe` is the same process.
        assert!(is_protected("SVCHOST.EXE"));
    }

    #[test]
    fn the_kernel_is_not_a_program() {
        for pid in [0, 4] {
            assert_eq!(
                may_end(pid, "System", Some("System"), false),
                Err(Refused::TheKernel),
                "pid {pid} was treated as something to end",
            );
        }
    }

    /// The list is short on purpose. Refusing everything that looks system-ish
    /// would make the feature useless, and restarting Explorer is a thing
    /// people do deliberately.
    #[test]
    fn an_ordinary_program_is_endable_and_so_is_explorer() {
        assert!(may_end(4820, "notepad.exe", Some("notepad.exe"), false).is_ok());
        assert!(may_end(4820, "explorer.exe", Some("explorer.exe"), false).is_ok());
        assert!(!is_protected("explorer.exe"));
        assert!(!is_protected("chrome.exe"));
    }

    /// Identity is settled before anything reads the name.
    ///
    /// Otherwise the ordering decides the answer: a reused id whose new
    /// occupant happens to be protected would be refused for the wrong reason,
    /// and the message would name a program that is not there.
    #[test]
    fn a_reused_id_is_reported_as_reused_whatever_took_it_over() {
        assert_eq!(
            may_end(900, "notepad.exe", Some("lsass.exe"), false),
            Err(Refused::Reused {
                now: "lsass.exe".to_string()
            }),
        );
    }

    /// Only meaningful on the machine, so it is ignored. Run it to see the
    /// list this draws.
    #[test]
    #[ignore]
    fn what_is_running_here() {
        let all = running();
        println!("  {} processes", all.len());

        for p in all.iter().take(12) {
            println!(
                "  {:>9} KB  {}{}",
                p.bytes / 1024,
                p.name,
                if p.visible { "  (windowed)" } else { "" }
            );
        }

        assert!(!all.is_empty(), "this machine is running something");
        assert!(
            all.iter().any(|p| p.visible),
            "something on this desktop has a window"
        );
    }
}
