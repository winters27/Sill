//! Talking to Everything directly, over its IPC protocol.
//!
//! Everything exposes a window, `EVERYTHING_TASKBAR_NOTIFICATION`, that
//! accepts a query as a `WM_COPYDATA` message and replies the same way. That
//! is exactly what `Everything64.dll` does internally, so going straight to
//! the protocol costs one dependency less and one process spawn less than
//! either alternative:
//!
//! - **`es.exe`** spawns a process per query. Correct, but `CreateProcess` is
//!   tens of milliseconds, paid on every keystroke of a search-as-you-type
//!   box.
//! - **The SDK DLL** is a normal function call, but `Everything64.dll` does
//!   not ship with Everything. Using it means vendoring a third-party binary
//!   into Sill.
//!
//! This needs a window to receive the reply on, and a window needs a thread
//! pumping messages, so one dedicated thread owns both and queries reach it
//! over a channel.

#![cfg(windows)]

use std::cell::RefCell;
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, PeekMessageW,
    RegisterClassExW, SendMessageW, TranslateMessage, HWND_MESSAGE, MSG, PM_REMOVE,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_COPYDATA, WNDCLASSEXW,
};

/// Prints protocol diagnostics when `SILL_EVERYTHING_DEBUG` is set.
macro_rules! trace {
    ($($arg:tt)*) => {
        if std::env::var_os("SILL_EVERYTHING_DEBUG").is_some() {
            eprintln!("[everything] {}", format!($($arg)*));
        }
    };
}

use crate::files::FileHit;

/// Everything's IPC message for a version 2 query, from its SDK headers.
const EVERYTHING_IPC_COPYDATA_QUERY2W: usize = 18;

/// Ask for the full path and file name in one field, which keeps the reply
/// parsing to a single variable-length record per item.
const REQUEST_FULL_PATH_AND_FILE_NAME: u32 = 0x0000_0004;

/// Sort by name ascending. Everything's IPC offers no relevance ordering, so
/// the ranking that matters is applied by the caller.
const SORT_NAME_ASCENDING: u32 = 1;

/// Item flag marking a folder rather than a file.
const ITEM_FOLDER: u32 = 1;

/// Search flags, from the same SDK headers.
pub const MATCH_CASE: u32 = 0x0000_0001;
pub const MATCH_PATH: u32 = 0x0000_0004;
pub const REGEX: u32 = 0x0000_0008;

/// Mirrors the Win32 struct.
///
/// Declared here rather than enabling `Win32_System_DataExchange`: it is three
/// fields, and this crate's features have already aborted a build once by
/// accumulating.
#[repr(C)]
struct CopyDataStruct {
    dw_data: usize,
    cb_data: u32,
    lp_data: *const core::ffi::c_void,
}

/// The query header, followed by the search string as UTF-16.
#[repr(C)]
struct Query2 {
    reply_hwnd: u32,
    reply_copydata_message: u32,
    search_flags: u32,
    offset: u32,
    max_results: u32,
    request_flags: u32,
    sort_type: u32,
}

thread_local! {
    /// Filled by the window procedure, drained by the query that is waiting.
    static REPLY: RefCell<Option<Vec<FileHit>>> = const { RefCell::new(None) };
}

type Job = (String, usize, u32, Sender<Vec<FileHit>>);

/// The channel into the thread that owns the reply window.
static QUERIES: OnceLock<Option<Sender<Job>>> = OnceLock::new();

/// Runs a search, or returns nothing if Everything is not available.
///
/// File search is an enhancement: a launcher that refused to work because a
/// third-party indexer is missing would be worse than one that quietly offers
/// less.
pub fn search(query: &str, limit: usize) -> Vec<FileHit> {
    search_with(query, limit, 0)
}

/// A search with Everything's own match flags applied.
pub fn search_with(query: &str, limit: usize, flags: u32) -> Vec<FileHit> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let Some(sender) = QUERIES.get_or_init(start_worker) else {
        return Vec::new();
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    if sender
        .send((query.to_string(), limit, flags, reply_tx))
        .is_err()
    {
        return Vec::new();
    }

    // Bounded so a wedged Everything cannot hang a keystroke.
    reply_rx
        .recv_timeout(std::time::Duration::from_millis(2000))
        .unwrap_or_default()
}

/// Whether Everything is running and answering.
pub fn available() -> bool {
    everything_window().is_some()
}

fn everything_window() -> Option<HWND> {
    // SAFETY: both arguments are static wide literals.
    let hwnd = unsafe { FindWindowW(w!("EVERYTHING_TASKBAR_NOTIFICATION"), PCWSTR::null()) };
    hwnd.ok().filter(|h| !h.is_invalid())
}

/// Starts the thread that owns the reply window, if Everything is present.
fn start_worker() -> Option<Sender<Job>> {
    everything_window()?;

    let (tx, rx) = mpsc::channel::<Job>();

    std::thread::Builder::new()
        .name("sill-everything".into())
        .spawn(move || {
            let Some(window) = create_reply_window() else {
                trace!("could not create the reply window");
                return;
            };
            trace!("reply window {:?}", window.0);

            for (query, limit, flags, reply) in rx {
                let hits = run_query(window, &query, limit, flags);
                let _ = reply.send(hits);
            }
        })
        .ok()?;

    Some(tx)
}

/// A message-only window, which exists purely to receive the reply.
fn create_reply_window() -> Option<HWND> {
    // SAFETY: the class name is a static literal, the procedure has the
    // required signature, and a null instance registers against this module.
    unsafe {
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            lpszClassName: w!("SillEverythingReply"),
            ..Default::default()
        };

        // A second registration of the same class fails harmlessly; the window
        // creation below is what actually matters.
        RegisterClassExW(&class);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("SillEverythingReply"),
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
        .ok()?;

        (!hwnd.is_invalid()).then_some(hwnd)
    }
}

/// Sends one query and pumps messages until the reply lands.
fn run_query(reply_window: HWND, query: &str, limit: usize, flags: u32) -> Vec<FileHit> {
    let Some(everything) = everything_window() else {
        return Vec::new();
    };

    let search: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();

    let header = Query2 {
        reply_hwnd: reply_window.0 as u32,
        reply_copydata_message: EVERYTHING_IPC_COPYDATA_QUERY2W as u32,
        search_flags: flags,
        offset: 0,
        max_results: limit as u32,
        request_flags: REQUEST_FULL_PATH_AND_FILE_NAME,
        sort_type: SORT_NAME_ASCENDING,
    };

    // The header and the search string travel as one contiguous block.
    let mut payload = Vec::with_capacity(std::mem::size_of::<Query2>() + search.len() * 2);
    // SAFETY: reading a #[repr(C)] struct as its own bytes.
    payload.extend_from_slice(unsafe {
        std::slice::from_raw_parts(
            &header as *const Query2 as *const u8,
            std::mem::size_of::<Query2>(),
        )
    });
    for unit in &search {
        payload.extend_from_slice(&unit.to_le_bytes());
    }

    let cds = CopyDataStruct {
        dw_data: EVERYTHING_IPC_COPYDATA_QUERY2W,
        cb_data: payload.len() as u32,
        lp_data: payload.as_ptr() as *const core::ffi::c_void,
    };

    REPLY.with(|r| *r.borrow_mut() = None);

    // SAFETY: the payload outlives the call, which is synchronous.
    let accepted = unsafe {
        SendMessageW(
            everything,
            WM_COPYDATA,
            Some(WPARAM(reply_window.0 as usize)),
            Some(LPARAM(&cds as *const CopyDataStruct as isize)),
        )
    };

    trace!(
        "query {:?} sent to {:?}, accepted={}",
        query,
        everything.0,
        accepted.0
    );

    if accepted.0 == 0 {
        return Vec::new();
    }

    pump_until_reply()
}

/// Runs the message loop until the window procedure has stored a reply.
///
/// Everything answers with its own `WM_COPYDATA`, which only arrives once this
/// thread dispatches messages.
fn pump_until_reply() -> Vec<FileHit> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);

    loop {
        if let Some(hits) = REPLY.with(|r| r.borrow_mut().take()) {
            return hits;
        }

        if std::time::Instant::now() >= deadline {
            trace!("timed out waiting for a reply");
            return Vec::new();
        }

        /*
         * Peek rather than Get.
         *
         * `GetMessageW` blocks until a message arrives, so if Everything never
         * replies the deadline above is never reached and the thread waits
         * forever. Peeking keeps the loop alive so the timeout can fire.
         */
        let mut msg = MSG::default();
        // SAFETY: standard message pump over an owned MSG.
        let had_message = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() };

        if had_message {
            // SAFETY: msg was filled by PeekMessageW.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        } else {
            // Nothing pending; yield rather than spin the core.
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
}

/// Receives Everything's reply.
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_COPYDATA {
        let cds = lparam.0 as *const CopyDataStruct;
        trace!(
            "WM_COPYDATA dwData={} cbData={}",
            if cds.is_null() { 0 } else { (*cds).dw_data },
            if cds.is_null() { 0 } else { (*cds).cb_data }
        );
        if !cds.is_null() && (*cds).dw_data == EVERYTHING_IPC_COPYDATA_QUERY2W {
            let bytes =
                std::slice::from_raw_parts((*cds).lp_data as *const u8, (*cds).cb_data as usize);
            let hits = parse_list(bytes);
            REPLY.with(|r| *r.borrow_mut() = Some(hits));
            return LRESULT(1);
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Parses `EVERYTHING_IPC_LIST2`.
///
/// Layout: a five-DWORD header, then one `{flags, data_offset}` pair per item,
/// then the item data. Each requested field is stored at `data_offset` as a
/// length in characters followed by that many UTF-16 units plus a terminator.
fn parse_list(bytes: &[u8]) -> Vec<FileHit> {
    let dword = |at: usize| -> Option<u32> {
        let slice = bytes.get(at..at + 4)?;
        Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    };

    // totitems, numitems, offset, request_flags, sort_type
    const HEADER: usize = 5 * 4;
    let Some(count) = dword(4) else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(count as usize);

    for i in 0..count as usize {
        let entry = HEADER + i * 8;
        let (Some(flags), Some(offset)) = (dword(entry), dword(entry + 4)) else {
            break;
        };

        let at = offset as usize;
        let Some(len) = dword(at) else { break };

        let start = at + 4;
        let end = start + len as usize * 2;
        let Some(raw) = bytes.get(start..end) else {
            break;
        };

        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();

        let path = String::from_utf16_lossy(&units);
        if path.is_empty() {
            continue;
        }

        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        out.push(FileHit {
            name,
            path,
            is_dir: flags & ITEM_FOLDER != 0,
        });
    }

    out
}
