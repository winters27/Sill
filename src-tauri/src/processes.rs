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

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };

    // SAFETY: the struct declares its own size, which is what the call reads
    // to know which version it was handed.
    let measured = unsafe {
        GetProcessMemoryInfo(
            handle,
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
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

    Some((name, path, counters.WorkingSetSize as u64))
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

/// Which processes own a window somebody could see.
#[cfg(windows)]
fn windowed_pids() -> std::collections::HashSet<u32> {
    use std::collections::HashSet;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn visit(window: HWND, into: LPARAM) -> BOOL {
        // SAFETY: `into` is the address of the set below, which outlives the
        // enumeration.
        let found = unsafe { &mut *(into.0 as *mut HashSet<u32>) };

        if unsafe { IsWindowVisible(window) }.as_bool() {
            let mut pid = 0u32;
            unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
            if pid != 0 {
                found.insert(pid);
            }
        }

        true.into()
    }

    let mut found: HashSet<u32> = HashSet::new();

    // SAFETY: the set outlives the call, which returns before this does.
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut _ as isize));
    }

    found
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
