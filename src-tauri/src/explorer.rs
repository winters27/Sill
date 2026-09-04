//! What is selected in the File Explorer window in front.
//!
//! The other half of "act on what I am already looking at". [`crate::selection`]
//! answers it for text by pressing Ctrl+C in somebody else's window; that
//! answer is useless in Explorer, where Ctrl+C puts file *references* on the
//! clipboard in a format nothing here reads and, worse, replaces whatever the
//! person had copied. Explorer already knows which items are highlighted, and
//! it will say so if it is asked properly.
//!
//! Asking properly is `IShellWindows`: the list of shell views Explorer has
//! open, each of which can be walked down to the `IFolderView` that owns the
//! highlight. Nothing is enumerated on a timer and nothing is held between two
//! presses, so the idle cost of this module is zero: it is a few COM calls
//! that happen while a key is down and release everything before they return.
//!
//! ## Only the window in front
//!
//! `IShellWindows` lists **every** Explorer window, and the one that matters
//! is the one the person is looking at. [`chosen`] refuses when the foreground
//! window is not one of them rather than falling back to the first, because a
//! key that recycled three files in a window behind the one on screen would be
//! indistinguishable from a launcher that had lost its mind.
//!
//! ## A hung Explorer must not take the launcher with it
//!
//! A COM call into another process blocks until that process answers, and
//! there is no cancellation. Explorer hanging is not exotic: a disconnected
//! network drive does it routinely. So [`selection`] runs the whole read on a
//! thread of its own and stops waiting after [`PATIENCE`]. On a timeout the
//! answer is "no files", the caller falls through to reading text, and the
//! abandoned thread finishes whenever Explorer does. One thread, once, against
//! a launcher that would otherwise be dead until reboot.
//!
//! ## Why there is no `IContextMenu` here
//!
//! `P8-01` asks for Explorer's shell verbs in Sill's action panel as well, and
//! this module deliberately does not provide them. The reasoning, so nobody
//! has to reconstruct it:
//!
//! - **Enumerating verbs loads other people's code into Sill.** Building a
//!   context menu for a file asks every shell extension registered for that
//!   file type to add its entries, and each of those is a DLL that COM loads
//!   into the calling process and never unloads. On an ordinary machine that
//!   is an archiver, a cloud drive, a version-control client and an antivirus
//!   product, and the launcher's whole claim is rule 23: about eleven
//!   megabytes of Rust sitting still. Paying tens of megabytes and a handful
//!   of permanent threads for a menu is the opposite of that.
//! - **A handler that hangs hangs Sill**, with none of the mitigation above
//!   available, because the hang would be inside a call this process made into
//!   code this process loaded. Explorer survives that because Windows restarts
//!   Explorer; nobody restarts Sill, and a dead Sill takes every global
//!   shortcut on the machine with it.
//! - **A handler that faults kills the process outright.** There is no
//!   catching it.
//!
//! The safe subset is real and worth having: the **static** verbs under a file
//! type's `shell` key in the registry are data, not code, and `ShellExecuteEx`
//! runs them out of process. That is Print, Edit, Play, Run as administrator
//! and whatever an application declared at install time, with nothing foreign
//! loaded here. It is not in this change because the action panel is built per
//! **kind** rather than per object (`actions_for` takes a mode, and
//! `ActionRegistry` holds actions whose ids are `&'static str`), so per-object
//! verbs need both of those opened up first. That is its own item, not a
//! detail of this one.
//!
//! The version that would deliver the dynamic handlers safely is a separate
//! short-lived process that enumerates and invokes on Sill's behalf and is
//! killed on a timeout, the way `exthost` already isolates extension code.
//! Also its own item. What must not happen is the in-process version, which is
//! the one that looks like three lines of code.

use crate::object::{Object, ObjectKind};

/// One highlighted item, as the shell describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The path on disk. Empty for an item that has none.
    pub path: String,
    /// Whether it is a folder to open rather than a file to act on.
    pub folder: bool,
}

/// How long Explorer gets to answer before the read is abandoned.
///
/// Generous by the standards of everything else on the keypress path, and it
/// has to be: this is a cross-process call into a program that may be busy
/// drawing a folder of forty thousand files. It is not a budget for the happy
/// case, which is under a millisecond; it is the line past which Explorer is
/// assumed to be stuck.
pub const PATIENCE: std::time::Duration = std::time::Duration::from_millis(600);

/// `SFGAO_FOLDER`: the shell will browse into this.
const FOLDER: u32 = 0x2000_0000;

/// `SFGAO_STREAM`: it is a single file, whatever else it is.
const STREAM: u32 = 0x0040_0000;

/**
Whether the shell's attributes describe a folder rather than a file.

**Both bits have to be read, not just the first.** A `.zip` is browsable, so
Explorer sets `SFGAO_FOLDER` on it and a naive read calls it a folder. It is a
file: it has bytes, it has a checksum, and the actions that apply to it are the
file ones. `SFGAO_STREAM` is the bit that says so, and it is set on every
archive, every `.cab` and every other compound file Explorer lets you walk
into.

Getting this wrong is not cosmetic. A zip called a folder offers "Open Terminal
Here" on a path no terminal can start in, and refuses the checksum and the
hash, which are the two things somebody selects an archive to ask for.
*/
pub fn is_folder(attributes: u32) -> bool {
    attributes & FOLDER != 0 && attributes & STREAM == 0
}

/**
Whether a window of this class could possibly be a shell view.

The cheap half of the question, asked before any COM object exists. Reading the
selection properly means creating `IShellWindows` and walking every Explorer
window there is, which was **measured at 15 to 23 ms** on a machine with one
Explorer window open and something else in front. That is paid on every press of
a universal key in a text editor, where the answer was always going to be "no
files": a class name is a few microseconds and answers it.

Rule 23 arriving on a keypress rather than at idle. The read is not wrong, it is
simply work done to reach a conclusion that was available for nothing.

`CabinetWClass` is a folder window and `ExploreWClass` is the old two-pane one.
`Progman` and `WorkerW` are the desktop, which is a shell view like any other
and where icons are selected exactly as they are in a folder.
*/
pub fn could_be_explorer(class: &str) -> bool {
    matches!(
        class,
        "CabinetWClass" | "ExploreWClass" | "Progman" | "WorkerW"
    )
}

/**
Whether a window of this class is a folder being browsed.

Narrower than [`could_be_explorer`] and deliberately so. That one answers "is
there any point asking the shell about this window", which the desktop
qualifies for: icons are selected on it exactly as they are in a folder. This
one answers "does this window have a folder open in it that a file dialog could
be pointed at", and the desktop does not, in the sense that matters: it is not
a window somebody navigated to and it is not what "the folder I am looking at"
means when a Save dialog is covering the screen.

Keeping them apart rather than reusing the looser one means [`folder_in_front`]
cannot pick `Progman` off the bottom of the Z-order and call it the folder the
person meant. It is the last window in every Z-order, so it would win whenever
no real Explorer window was open at all, which is the exact case where the
right answer is to do nothing.
*/
pub fn browses_a_folder(class: &str) -> bool {
    matches!(class, "CabinetWClass" | "ExploreWClass")
}

/**
Which open folder window is nearest the front.

`order` is the Z-order, frontmost first, and `open` is what `IShellWindows`
reported, in the order the shell happens to list them. The answer is an index
into `open`, because that is the list the COM interfaces are held in.

**Nearest the front rather than the foreground window**, and that is the whole
difference between this and [`chosen`]. A dialog jump is pressed *in a dialog*,
so Explorer is by definition not in front; the window somebody means is the one
the dialog is covering. Falling back to "the first one the shell listed" would
be the bug [`chosen`] refuses, arriving through a different door: a key that
jumps to a folder open on another monitor because that window happened to be
created first.
*/
pub fn nearest(order: &[isize], open: &[isize]) -> Option<usize> {
    order
        .iter()
        .filter(|window| **window != 0)
        .find_map(|window| open.iter().position(|shell| shell == window))
}

/**
Which of Explorer's open views the person is actually looking at.

Refuses rather than guessing. `IShellWindows` hands back every Explorer window
there is, in no order anybody should rely on, and the only one a key press can
mean is the one that had the keyboard when it was pressed. Falling back to the
first would mean a shortcut pressed in a text editor quietly acting on files in
an Explorer window on another monitor.

Takes handles rather than interfaces so the decision is a function over values
and the COM is left with nothing to decide.
*/
pub fn chosen(front: isize, windows: &[isize]) -> Option<usize> {
    // A null foreground handle is "nothing is in front", which no window
    // matches. Explorer never reports one, so without this a window whose
    // handle could not be read would match it.
    if front == 0 {
        return None;
    }

    windows.iter().position(|window| *window == front)
}

/**
The highlighted items, as things the action registry can be run against.

Items with no path on disk are dropped. Explorer shows plenty of them: This PC,
Control Panel, a network location, a camera. They are perfectly real shell
items and there is nothing Sill can do to one, so an object built from an empty
target would reach `Launch` and ask the shell to open `""`.

The id is the path, which is what a file result's id already is, so ranking and
the activity log see the same identity whether a file arrived from a search or
from Explorer.
*/
pub fn objects_from(items: &[Selected]) -> Vec<Object> {
    items
        .iter()
        .filter(|item| !item.path.trim().is_empty())
        .map(|item| Object {
            kind: if item.folder {
                ObjectKind::Folder
            } else {
                ObjectKind::File
            },
            id: format!("file:{}", item.path),
            target: item.path.clone(),
            title: crate::files_ops::name_of(std::path::Path::new(&item.path)),
            // The two modes the window already draws file rows under, so a
            // row that arrived from Explorer looks like one that arrived from
            // a search and the same action panel opens on it.
            mode: if item.folder { "folder" } else { "file" }.to_string(),
        })
        .collect()
}

/// Whatever is highlighted in the Explorer window in front, if that is what is
/// in front.
///
/// Empty for everything else, which is the signal the caller falls through on:
/// no Explorer, no selection, a selection of things that are not on disk, or
/// an Explorer that did not answer in time.
#[cfg(windows)]
pub fn selection() -> Vec<Selected> {
    let front = foreground_handle();
    if front == 0 {
        return Vec::new();
    }

    // Before any COM exists, because the great majority of presses are in
    // something that is not Explorer at all and the whole read is 15 to 23 ms.
    if !could_be_explorer(&crate::windowing::class_of(front)) {
        return Vec::new();
    }

    let (say, hear) = std::sync::mpsc::channel();

    // Its own thread, and deliberately not a pool one. The point is to be able
    // to walk away from it: a blocking-pool thread abandoned mid-call is a
    // slot the pool never gets back, and this one is expected to finish on its
    // own the moment Explorer unsticks.
    std::thread::spawn(move || {
        let _ = say.send(windows_impl::read(front));
    });

    match hear.recv_timeout(PATIENCE) {
        Ok(found) => found,
        Err(_) => {
            crate::say!("Explorer did not say what was selected within {PATIENCE:?}");
            Vec::new()
        }
    }
}

#[cfg(not(windows))]
pub fn selection() -> Vec<Selected> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn folder_in_front() -> Option<String> {
    None
}

/// The folder open in the Explorer window nearest the front.
///
/// The other way round from [`selection`], and for a reason that is the whole
/// of `P8-07`. A selection is read with Explorer in front, so the foreground
/// window is the answer and anything else is refused. A dialog jump is pressed
/// **in a file dialog**, which means Explorer is behind it by definition, and
/// the window somebody means is the one the dialog is covering.
///
/// So the Z-order decides instead of the foreground, through [`nearest`], and
/// the cheap half still comes first: the walk reads class names and nothing
/// else, and stops without creating a COM object at all when no folder window
/// is open. On a machine with no Explorer running this costs one enumeration
/// and returns nothing.
///
/// Same thread and the same [`PATIENCE`] as [`selection`], for the same
/// reason: a shell that is stuck on a disconnected drive must not take the
/// launcher with it.
#[cfg(windows)]
pub fn folder_in_front() -> Option<String> {
    let order = windows_impl::folder_windows();

    // Nothing to ask, so nothing is created. The common miss on a machine
    // where no folder window is open at all.
    if order.is_empty() {
        return None;
    }

    let (say, hear) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let _ = say.send(windows_impl::read_folder(&order));
    });

    match hear.recv_timeout(PATIENCE) {
        Ok(found) => found,
        Err(_) => {
            crate::say!("Explorer did not say which folder was open within {PATIENCE:?}");
            None
        }
    }
}

/// The foreground window's handle, or zero.
///
/// Raw rather than [`crate::windowing::foreground`], which enumerates and
/// refuses Sill's own window. Here the handle is only ever compared against
/// what Explorer reports, so an enumeration would be work done to answer a
/// question that a comparison already answers.
#[cfg(windows)]
fn foreground_handle() -> isize {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY: takes nothing, returns a handle, dereferences nothing.
    unsafe { GetForegroundWindow() }.0 as isize
}

#[cfg(windows)]
mod windows_impl {
    use super::{chosen, is_folder, Selected};

    use windows::core::{Interface, GUID};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IServiceProvider, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::SystemServices::{SFGAO_FLAGS, SFGAO_FOLDER, SFGAO_STREAM};
    use windows::Win32::System::Variant::VARIANT;
    use windows::Win32::UI::Shell::{
        IFolderView, IPersistFolder2, IShellBrowser, IShellItemArray, IShellWindows,
        IWebBrowserApp, SHGetNameFromIDList, ShellWindows, SIGDN_FILESYSPATH, SVGIO_SELECTION,
    };

    /// `SID_STopLevelBrowser`, the service that hands back the shell browser.
    ///
    /// Declared here because the `windows` crate does not generate the service
    /// ids from `shlguid.h`. Same situation as the interop interface
    /// `hello.rs` declares by hand, and the same remedy.
    const TOP_LEVEL_BROWSER: GUID = GUID::from_u128(0x4C96BE40_915C_11CF_99D3_00AA004AE837);

    /// Nobody selects more than this and means it.
    ///
    /// Ctrl+A in a folder of forty thousand files is one keystroke, and
    /// turning that into forty thousand objects, each with a path, would spend
    /// a keypress building a list the panel cannot draw. Refused whole rather
    /// than truncated: acting on the first hundred of a selection somebody
    /// made deliberately is worse than saying nothing.
    const TOO_MANY: u32 = 200;

    /// Reads the selection out of the Explorer window whose handle this is.
    ///
    /// Apartment-threaded, unlike `uia.rs` and like every other shell COM user
    /// here: the shell view objects are apartment objects and asking for them
    /// from a multi-threaded apartment marshals every call through a proxy for
    /// no benefit. This thread has no message loop, which is survivable
    /// because the calls are short and the caller has stopped waiting anyway
    /// once they are not.
    pub(super) fn read(front: isize) -> Vec<Selected> {
        // SAFETY: COM is initialised and uninitialised on this one thread
        // around the whole call, and every interface below is released by its
        // own Drop before the uninitialise.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let found = look(front).unwrap_or_default();

            if initialised {
                CoUninitialize();
            }

            found
        }
    }

    /// Every shell window the shell will admit to, and its handle.
    ///
    /// One implementation for both reads. Which window a press means is
    /// decided differently by each of them, [`chosen`] against the foreground
    /// and [`nearest`] against the Z-order, but *what the list is* is the same
    /// question and enumerating it twice in two ways is how the two answers
    /// would eventually stop agreeing about what a window is.
    ///
    /// # Safety
    ///
    /// Must be called with COM initialised on this thread.
    unsafe fn open_windows() -> windows::core::Result<(Vec<IWebBrowserApp>, Vec<isize>)> {
        let shell: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL)?;
        let count = shell.Count()?;

        // Every open Explorer window's handle, in the order the shell lists
        // them, so the index either decision returns addresses the same list.
        let mut browsers = Vec::with_capacity(count.max(0) as usize);
        let mut handles = Vec::with_capacity(count.max(0) as usize);

        for at in 0..count {
            let Ok(dispatch) = shell.Item(&VARIANT::from(at)) else {
                continue;
            };

            // Not every shell window is an Explorer window: Internet Explorer
            // is in this list on machines that still have it, and a window
            // that does not answer to this interface is not one to read.
            let Ok(browser) = dispatch.cast::<IWebBrowserApp>() else {
                continue;
            };

            let handle = browser.HWND().map(|hwnd| hwnd.0).unwrap_or(0);
            browsers.push(browser);
            handles.push(handle);
        }

        Ok((browsers, handles))
    }

    /// The shell view one of those windows is showing.
    ///
    /// # Safety
    ///
    /// Must be called with COM initialised on this thread.
    unsafe fn view_of(browser: &IWebBrowserApp) -> windows::core::Result<IFolderView> {
        let service: IServiceProvider = browser.cast()?;
        let shell_browser: IShellBrowser = service.QueryService(&TOP_LEVEL_BROWSER)?;
        shell_browser.QueryActiveShellView()?.cast()
    }

    /// # Safety
    ///
    /// Must be called with COM initialised on this thread.
    unsafe fn look(front: isize) -> windows::core::Result<Vec<Selected>> {
        let (browsers, handles) = open_windows()?;

        let Some(at) = chosen(front, &handles) else {
            return Ok(Vec::new());
        };

        let view = view_of(&browsers[at])?;
        let items: IShellItemArray = view.Items(SVGIO_SELECTION)?;

        let selected = items.GetCount()?;
        if selected == 0 || selected > TOO_MANY {
            if selected > TOO_MANY {
                crate::say!("{selected} items are selected, which is more than a panel can be");
            }
            return Ok(Vec::new());
        }

        let mut found = Vec::with_capacity(selected as usize);

        for at in 0..selected {
            let item = items.GetItemAt(at)?;

            // A shell item with no filesystem name is a real item Sill cannot
            // act on. It becomes an empty path here and `objects_from` drops
            // it, which keeps the deciding in one place.
            let path = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map(|name| {
                    let owned = name.to_string().unwrap_or_default();
                    windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
                    owned
                })
                .unwrap_or_default();

            let attributes = item
                .GetAttributes(FOLDER_OR_STREAM)
                .map(|flags| flags.0)
                .unwrap_or(0);

            found.push(Selected {
                path,
                folder: is_folder(attributes),
            });
        }

        Ok(found)
    }

    /// Every open folder window, frontmost first.
    ///
    /// `EnumWindows` walks the Z-order from the top down, which is what
    /// `windowing::list` already relies on for "which window did I use last".
    /// Only the class is read, so a window nobody is going to ask about costs
    /// one call and no allocation, and no COM object exists yet when this
    /// returns nothing.
    pub(super) fn folder_windows() -> Vec<isize> {
        use windows::core::BOOL;
        use windows::Win32::Foundation::{HWND, LPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};

        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            // SAFETY: the pointer is the Vec passed in below, which outlives
            // the enumeration because EnumWindows is synchronous.
            let found = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };

            // SAFETY: the handle came from the enumeration itself.
            if unsafe { IsWindowVisible(hwnd) }.as_bool() {
                let window = hwnd.0 as isize;
                if super::browses_a_folder(&crate::windowing::class_of(window)) {
                    found.push(window);
                }
            }

            BOOL(1)
        }

        let mut found: Vec<isize> = Vec::new();

        // SAFETY: the callback matches the required signature and the pointer
        // points at a live Vec for the duration of this synchronous call.
        unsafe {
            let _ = EnumWindows(
                Some(collect),
                LPARAM(&mut found as *mut Vec<isize> as isize),
            );
        }

        found
    }

    /// The path of the folder shown in the frontmost of those windows.
    pub(super) fn read_folder(order: &[isize]) -> Option<String> {
        // SAFETY: COM is initialised and uninitialised on this one thread
        // around the whole call, and every interface below is released by its
        // own Drop before the uninitialise.
        unsafe {
            let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let found = current_folder(order).ok().flatten();

            if initialised {
                CoUninitialize();
            }

            found
        }
    }

    /// # Safety
    ///
    /// Must be called with COM initialised on this thread.
    unsafe fn current_folder(order: &[isize]) -> windows::core::Result<Option<String>> {
        let (browsers, handles) = open_windows()?;

        let Some(at) = super::nearest(order, &handles) else {
            return Ok(None);
        };

        // `IPersistFolder2` rather than the view's items, because the question
        // is where the window is rather than what is highlighted in it. It is
        // also the only one of the two that answers for an empty folder, which
        // is a folder somebody is very likely to be saving into.
        let folder: IPersistFolder2 = view_of(&browsers[at])?.GetFolder()?;
        let id = folder.GetCurFolder()?;

        let path = SHGetNameFromIDList(id, SIGDN_FILESYSPATH).map(|name| {
            let owned = name.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(name.0 as *const _));
            owned
        });

        // Freed whichever way the name went, because `GetCurFolder` hands
        // over ownership of the list and an early return through `?` would
        // leak it on every press.
        windows::Win32::System::Com::CoTaskMemFree(Some(id as *const _));

        // A shell folder with no path on disk is This PC, or a library, or a
        // phone over MTP. Perfectly real, and not somewhere a file dialog can
        // be pointed.
        Ok(path.ok().filter(|path| !path.trim().is_empty()))
    }

    /// The two attribute bits [`is_folder`] reads, asked for together.
    ///
    /// `GetAttributes` returns only the bits it was asked about, so asking for
    /// one of these and reading the other always answers no.
    const FOLDER_OR_STREAM: SFGAO_FLAGS = SFGAO_FLAGS(SFGAO_FOLDER.0 | SFGAO_STREAM.0);

    /// The two bits [`super::is_folder`] names, as Windows spells them.
    ///
    /// Held against the crate's own constants rather than trusted, because
    /// this module hard-codes them so its decision stays a function over a
    /// plain `u32` that a test on any platform can call. A number copied out
    /// of a header is exactly the kind of thing that is right until it is not.
    #[test]
    fn the_bits_this_module_names_are_the_bits_windows_means() {
        assert_eq!(super::FOLDER, SFGAO_FOLDER.0);
        assert_eq!(super::STREAM, SFGAO_STREAM.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The zip.
    ///
    /// Explorer browses into an archive, so it carries the folder bit, and
    /// calling it a folder on the strength of that offers a terminal in a
    /// place no terminal can start and hides the two actions somebody selects
    /// an archive to reach.
    #[test]
    fn an_archive_is_a_file_however_browsable_it_is() {
        assert!(!is_folder(FOLDER | STREAM), "a zip is a file");
        assert!(is_folder(FOLDER), "a real folder is a folder");
        assert!(!is_folder(STREAM), "an ordinary file is a file");
        assert!(!is_folder(0), "nothing set is not a folder");
    }

    /// Attributes Sill never asked about must not turn a file into a folder.
    #[test]
    fn only_the_two_bits_that_were_asked_about_decide() {
        // Read-only, hidden, compressed and a pile of others, all set.
        let noise = 0x0000_FFFF;

        assert!(!is_folder(noise));
        assert!(is_folder(noise | FOLDER));
        assert!(!is_folder(noise | FOLDER | STREAM));
    }

    /// The class check that keeps 20 milliseconds off every press in a
    /// document.
    ///
    /// Measured: with an Explorer window open and something else in front, the
    /// full read cost 15 to 23 ms and returned nothing. A key bound to
    /// "whatever is selected" is pressed in an editor far more often than in
    /// Explorer, so that is the common case rather than the exotic one.
    #[test]
    fn nothing_but_a_shell_window_is_worth_asking_about() {
        for shell in ["CabinetWClass", "ExploreWClass", "Progman", "WorkerW"] {
            assert!(could_be_explorer(shell), "{shell}");
        }

        for other in [
            // A browser, an editor, a terminal, a Win32 dialog, and a window
            // whose class could not be read.
            "Chrome_WidgetWin_1",
            "MozillaWindowClass",
            "CASCADIA_HOSTING_WINDOW_CLASS",
            "#32770",
            "Notepad",
            "",
        ] {
            assert!(!could_be_explorer(other), "{other}");
        }
    }

    /// The rule that keeps a key from acting on a window nobody is looking at.
    ///
    /// The list Explorer hands back is every window it has open. Pressing a
    /// universal key in a text editor, with two Explorer windows open behind
    /// it, must read the editor's text rather than recycle whatever happens to
    /// be highlighted in one of them.
    #[test]
    fn only_the_explorer_window_in_front_is_read() {
        let open = [0x1111, 0x2222, 0x3333];

        assert_eq!(chosen(0x2222, &open), Some(1));
        assert_eq!(chosen(0x1111, &open), Some(0));

        // Something else entirely has the keyboard.
        assert_eq!(chosen(0x9999, &open), None);

        // No Explorer window at all.
        assert_eq!(chosen(0x1111, &[]), None);
    }

    /// Which windows are worth asking about, and which are worth jumping to.
    #[test]
    fn the_desktop_is_a_shell_view_but_not_a_folder_to_jump_to() {
        for folder in ["CabinetWClass", "ExploreWClass"] {
            assert!(browses_a_folder(folder), "{folder}");
            assert!(could_be_explorer(folder), "{folder}");
        }

        /*
         * The desktop, which both questions have to answer differently.
         *
         * It is a shell view: icons are selected on it exactly as they are in
         * a folder, so `could_be_explorer` says yes and reading a selection
         * from it is right. It is not somewhere a dialog gets pointed, and the
         * reason this matters is the Z-order: `Progman` is the last window in
         * every Z-order there is, so a jump that accepted it would silently
         * work on every machine with no folder window open at all, which is
         * exactly when the right answer is to do nothing.
         */
        for desktop in ["Progman", "WorkerW"] {
            assert!(could_be_explorer(desktop), "{desktop}");
            assert!(!browses_a_folder(desktop), "{desktop}");
        }

        for other in ["#32770", "Chrome_WidgetWin_1", ""] {
            assert!(!browses_a_folder(other), "{other}");
        }
    }

    /// The window nearest the front wins, not the first the shell listed.
    ///
    /// `IShellWindows` hands its windows back in creation order, which is the
    /// order they were opened rather than the order they are stacked. A jump
    /// pressed in a dialog means the folder window the dialog is covering, so
    /// the Z-order decides and the shell's own order is only an index into
    /// the interfaces.
    #[test]
    fn the_folder_window_nearest_the_front_is_the_one_meant() {
        // The shell opened three windows; the person has since raised the
        // last one and then the second.
        let open = [0x1111, 0x2222, 0x3333];
        let stacked = [0x2222, 0x3333, 0x1111];

        assert_eq!(nearest(&stacked, &open), Some(1));

        // A window that is stacked in front but is not one Explorer admits to
        // is skipped rather than matched.
        assert_eq!(nearest(&[0x9999, 0x3333], &open), Some(2));

        assert_eq!(nearest(&[], &open), None);
        assert_eq!(nearest(&stacked, &[]), None);
    }

    /// A handle Explorer could not report is not a match for a missing one.
    ///
    /// `IWebBrowserApp::HWND` failing leaves a zero in the shell's list, and a
    /// zero in the Z-order would then select it. Same guard as [`chosen`],
    /// arriving through the other decision.
    #[test]
    fn a_window_with_no_handle_is_never_the_nearest() {
        assert_eq!(nearest(&[0, 0x2222], &[0, 0x2222]), Some(1));
    }

    /// A handle that could not be read is not a match for one that could not.
    ///
    /// `IWebBrowserApp::HWND` failing leaves a zero in the list. Without the
    /// guard, a foreground handle of zero would select that window and Sill
    /// would read the selection of an Explorer window it could not identify.
    #[test]
    fn nothing_in_front_matches_nothing() {
        assert_eq!(chosen(0, &[0, 0x2222]), None);
    }

    #[test]
    fn a_selected_file_becomes_a_file_and_a_folder_a_folder() {
        let objects = objects_from(&[
            Selected {
                path: r"C:\work\notes.md".into(),
                folder: false,
            },
            Selected {
                path: r"C:\work\archive".into(),
                folder: true,
            },
        ]);

        assert_eq!(objects.len(), 2);

        assert_eq!(objects[0].kind, ObjectKind::File);
        assert_eq!(objects[0].mode, "file");
        assert_eq!(objects[0].title, "notes.md");
        assert_eq!(objects[0].target, r"C:\work\notes.md");
        assert_eq!(objects[0].id, r"file:C:\work\notes.md");

        assert_eq!(objects[1].kind, ObjectKind::Folder);
        assert_eq!(objects[1].mode, "folder");
        assert_eq!(objects[1].title, "archive");
    }

    /// This PC, Control Panel, a network place, a phone plugged in over MTP.
    ///
    /// All of them are selectable in Explorer and none has a path. An object
    /// built from one reaches `Launch`, which hands the shell an empty string.
    #[test]
    fn something_with_no_path_on_disk_is_not_offered() {
        let objects = objects_from(&[
            Selected {
                path: String::new(),
                folder: true,
            },
            Selected {
                path: "   ".into(),
                folder: false,
            },
            Selected {
                path: r"C:\work\notes.md".into(),
                folder: false,
            },
        ]);

        assert_eq!(objects.len(), 1, "{objects:?}");
        assert_eq!(objects[0].target, r"C:\work\notes.md");
    }

    /// Explorer's order is the order, because it is the order on screen.
    #[test]
    fn the_order_on_screen_is_the_order_in_the_list() {
        let objects = objects_from(&[
            Selected {
                path: r"C:\b.txt".into(),
                folder: false,
            },
            Selected {
                path: r"C:\a.txt".into(),
                folder: false,
            },
        ]);

        assert_eq!(objects[0].title, "b.txt");
        assert_eq!(objects[1].title, "a.txt");
    }
}
