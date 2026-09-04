/*!
Virtual desktops, and the windows that are on the other ones.

## What this delivers, and what it deliberately does not

**It finds windows on other virtual desktops and says which desktop they are
on.** Before this, `windowing` dropped them: a window on another desktop is
cloaked, cloaked windows were filtered out as ghosts, and so a window somebody
had deliberately put on desktop 2 was invisible to the launcher. It is listed
now, its row says where it is, and switching to it works, because Windows
changes desktop to show a window that is being brought to the front.

**It does not move a window to another desktop, and that is a finding rather
than a gap in the work.** See below.

## Two interfaces, and neither one does what you would expect

`IVirtualDesktopManager` is **documented and stable**, shipped since Windows
10 1607. It answers three things:

| Method | Cross-process | Used here |
| --- | --- | --- |
| `GetWindowDesktopId` | yes | yes |
| `IsWindowOnCurrentVirtualDesktop` | yes | yes |
| `MoveWindowToDesktop` | **no** | no |

**`MoveWindowToDesktop` will not touch a window belonging to another
process.** Measured on build 26200 against a Character Map this codebase
started itself: `E_ACCESSDENIED`, every time. Attaching thread input to the
target's thread did not change it, and neither did taking the foreground
first, which are the two things that unlock the other Win32 calls that behave
this way. It moves the caller's own windows and nothing else, which makes it
useless to a launcher, whose entire job is other people's windows.

So every "send this window to desktop 2" needs the **undocumented**
`IVirtualDesktopManagerInternal::MoveViewToDesktop`, plus a second
undocumented interface to turn a window handle into the application view that
takes. That is a mutating call into an undocumented vtable, and it cannot be
watched working on a machine with one virtual desktop: proving a move means
seeing a window land somewhere else. Giving this machine a second desktop
means `CreateDesktopW` and `RemoveDesktop`, two more slots in the same
undocumented vtable that nothing here has verified, aimed at somebody's live
shell. **An unverified mutating call into an undocumented interface is the
exact risk this item is rated High for**, so it is not here. The probe in
`suite/real_desktops.rs` does the whole move on a machine that has two
desktops, and it is what a later session should run before adding it.

`IVirtualDesktopManagerInternal` **is** used, for one question the documented
interface cannot answer: how many desktops there are and what order Task View
shows them in. That is what turns "on another desktop" into "on desktop 2".
**Three of its methods are called and all three are questions.** Nothing here
tells it anything.

## What makes reading it safe enough

Four things have to hold, and any one failing means the numbering is simply
off and rows say "on another desktop" instead.

1. **The Windows build is one this code was run against.** [`VERIFIED`] is the
   list, and it is short on purpose. An unlisted build gets
   [`Reach::Identity`].
2. **The interface id answers.** A wrong id is refused with `E_NOINTERFACE`
   and nothing is called. This is a stronger gate than a build number and it
   was measured rather than assumed: on build 26200, five other published ids
   for this same interface are all refused and only the pinned one answers.
   Microsoft issues a new id when the layout changes; that is what an
   interface id is for, and it is why an id that answers is evidence about the
   whole vtable rather than about one slot.
3. **Only three slots are ever callable.** The vtable in [`platform`] names
   every other slot as a plain integer, so a later edit cannot call one by
   accident. `MoveViewToDesktop` is one of those integers. That this matters
   was confirmed rather than assumed: moving `GetCount` down by one slot and
   running the probe produced `STATUS_ACCESS_VIOLATION` in the test process,
   not an error return. **A vtable read at the wrong offset does not fail. It
   crashes**, which is why three separate things have to agree before one is
   read at all.
4. **The answer agrees with the documented interface.** [`agrees`] is the
   tripwire: the desktop the undocumented list calls current has to be the
   same desktop the documented interface names for the window in front. Two
   independent routes to one value, and a vtable read at the wrong offset
   would have to produce the right identity by accident to pass it. This is
   also how the layout was established in the first place, by hand, before any
   of it was written down here.

**Verified against Windows 11 Pro 10.0.26200.9168 on 2026-09-03.** `GetCount`
said one desktop, `GetDesktops` returned an array of one, and the identity in
it was the identity the documented `GetWindowDesktopId` gave for all sixteen
open windows.

## What it costs

An apartment, and about a millisecond per window enumeration on a machine that
has any cloaked window at all. A machine with none pays nothing: the batch
returns before it stands anything up.

The apartment is deliberate and it was measured. See [`platform::with_com`].
*/

use serde::{Deserialize, Serialize};

/// One virtual desktop, by the identity Windows gives it.
///
/// A `u128` rather than a Windows `GUID`, so that everything deciding what to
/// do with one is ordinary Rust that compiles and runs anywhere. The Windows
/// half converts at the boundary and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DesktopId(pub u128);

impl std::fmt::Display for DesktopId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.0;
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            (n >> 96) as u32,
            (n >> 80) as u16,
            (n >> 64) as u16,
            (n >> 48) as u16,
            n & 0xffff_ffff_ffff,
        )
    }
}

/// How much this machine can be asked about virtual desktops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Everything: which desktop a window is on, and the desktops themselves
    /// in the order Task View shows them, so they can be numbered.
    Ordered,
    /// Only what the documented interface answers. A window can still be found
    /// and still says it is somewhere else; it cannot say which desktop,
    /// because nothing here may count them.
    Identity,
    /// Not this operating system, or one too old to have the documented
    /// interface at all.
    None,
}

/**
The Windows builds the undocumented half has actually been run against.

**One entry, and adding a second means running the probe on that build**, not
reasoning about whether it is probably fine. `suite/real_desktops.rs` is that
probe: it asks the undocumented interface for the desktops, asks the
documented interface the same question a different way, and fails if the two
disagree. A build that passes it can be added here.

This is the difference between this feature being off on a machine nobody has
tested and being subtly wrong on one.
*/
pub const VERIFIED: &[u32] = &[26200];

/// The build `IVirtualDesktopManager` first shipped in: Windows 10 1607.
///
/// Below it there is no documented interface either, so there is nothing to
/// fall back to and the answer is [`Reach::None`].
pub const DOCUMENTED_SINCE: u32 = 14393;

/// What this build is allowed to be asked.
///
/// Pure, so the refusing cases are testable on the machine where the feature
/// works, which is the only machine anyone is going to run the tests on.
pub fn reach(build: u32) -> Reach {
    if build < DOCUMENTED_SINCE {
        return Reach::None;
    }

    if VERIFIED.contains(&build) {
        Reach::Ordered
    } else {
        Reach::Identity
    }
}

/// Which desktop this is, as Task View numbers them.
///
/// Counted from one, because that is what Task View writes on them, and
/// `None` for a desktop that is not in the list. A desktop can be closed
/// between reading the list and drawing a row, and a row that invented a
/// number for it would be pointing somewhere that does not exist.
pub fn number_of(desktop: DesktopId, ordered: &[DesktopId]) -> Option<usize> {
    ordered
        .iter()
        .position(|known| *known == desktop)
        .map(|at| at + 1)
}

/// What a row says about a window that is not on this desktop.
///
/// The unnumbered wording is not a fallback nobody will see: it is what every
/// Windows build outside [`VERIFIED`] shows, because the number is the one
/// part that needs the undocumented interface. It still tells somebody the
/// thing they need to know before pressing Enter, which is that the screen is
/// about to change.
pub fn label(number: Option<usize>) -> String {
    match number {
        Some(number) => format!("on desktop {number}"),
        None => "on another desktop".to_string(),
    }
}

/**
Whether the undocumented list can be believed.

The tripwire. `internal` is the desktop the undocumented interface calls
current and `documented` is the one the documented interface names for the
window in front. They are two independent routes to one fact.

The list also has to contain that desktop. An ordered list of desktops that
does not include the one being looked at is not a list of desktops.
*/
pub fn agrees(desktops: &[DesktopId], internal: DesktopId, documented: DesktopId) -> bool {
    internal == documented && desktops.contains(&internal)
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod platform {
    use super::DesktopId;

    use windows::core::{IUnknown, Interface, GUID, HRESULT};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IServiceProvider, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Shell::Common::IObjectArray;
    use windows::Win32::UI::Shell::{IVirtualDesktopManager, VirtualDesktopManager};

    /// The shell's own service host, which is where the undocumented
    /// interface lives. Not in the `windows` crate, because nothing about it
    /// is documented.
    const IMMERSIVE_SHELL: GUID = GUID::from_u128(0xc2f03a33_21f5_47fa_b4bb_156362a2f239);

    /// The service to ask that host for.
    const DESKTOP_SERVICE: GUID = GUID::from_u128(0xc5e0cdca_7b6e_41b2_9fc4_d93975cc467b);

    /*
    `IVirtualDesktopManagerInternal`, as Windows 11 build 26200 lays it out.

    **This identity is the version pin.** Asking for the wrong one is refused
    rather than answered: five other published identities for this same
    interface were tried against build 26200 and every one of them came back
    `E_NOINTERFACE`. A Windows that changed the layout therefore hands this
    code nothing, which is the failure everybody wants.

    Only `GetCount`, `GetCurrentDesktop` and `GetDesktops` are declared as
    functions. Every other slot is a `usize`, so the compiler will not let a
    later edit call one by accident, and a reader can see at a glance that
    nothing here changes anything. `MoveViewToDesktop`, the one that would
    move a window between desktops, is the first of those integers and the
    module header says why it stays one.
    */
    windows::core::imp::define_interface!(
        Internal,
        InternalVtbl,
        0x53f5ca0b_158f_4124_900c_057158060b27
    );

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    pub struct InternalVtbl {
        base: windows::core::IUnknown_Vtbl,
        GetCount: unsafe extern "system" fn(*mut core::ffi::c_void, *mut u32) -> HRESULT,
        MoveViewToDesktop: usize,
        CanViewMoveDesktops: usize,
        GetCurrentDesktop: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
        GetDesktops: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut *mut core::ffi::c_void,
        ) -> HRESULT,
    }

    /*
    One desktop, as the undocumented interface hands it back.

    Same treatment: `IsViewVisible` sits before `GetID` in this vtable and is
    named as an integer, because the only thing wanted from a desktop object
    is its identity. Passing an identity buffer where an application view was
    expected is precisely the accident this shape prevents.
    */
    windows::core::imp::define_interface!(
        Desktop,
        DesktopVtbl,
        0x3f07f4be_b107_441a_af0f_39d82529072c
    );

    #[repr(C)]
    #[allow(non_snake_case, dead_code)]
    pub struct DesktopVtbl {
        base: windows::core::IUnknown_Vtbl,
        IsViewVisible: usize,
        GetID: unsafe extern "system" fn(*mut core::ffi::c_void, *mut GUID) -> HRESULT,
    }

    windows::core::imp::interface_hierarchy!(Internal, IUnknown);
    windows::core::imp::interface_hierarchy!(Desktop, IUnknown);

    fn id_of(guid: GUID) -> DesktopId {
        DesktopId(guid.to_u128())
    }

    fn hwnd_of(id: isize) -> HWND {
        HWND(id as *mut core::ffi::c_void)
    }

    /**
    Puts this thread in an apartment, once, and leaves it there.

    **The apartment is the whole cost of this feature, and it was measured
    rather than reasoned about.** The first version of this function
    initialised COM and uninitialised it around every call, which is what
    `uia` does. That put the window list at **8 to 11 ms**, up from about one
    and a half, and the calls were not where the time went: standing the
    manager up costs **4 µs** once the apartment is warm, and asking which
    desktop a window is on costs **30 to 70 µs**. It was `CoUninitialize` on
    the last reference tearing the multi-threaded apartment's RPC machinery
    down, so that the next call had to build it again.

    Never uninitialised, then, which is exactly what `icons::ensure_com` does
    and for the same reason: the thread will make more shell calls and tearing
    the apartment down under them costs more than leaving it up. What stays
    resident is an apartment. **Nothing here holds a manager, a desktop or a
    list between calls**, so there is still no cached answer to go stale, and
    a window that moves desktop is somewhere else the next time anybody looks.

    Multi-threaded, for the reason `uia::with_com` is: these calls cross into
    the shell's process, they run on a blocking pool thread with no message
    loop, and a single-threaded apartment that makes a cross-process call
    without pumping messages is the ordinary way to hang. Somebody else may
    already have put this thread in a different apartment, which is fine to
    work in and must not be touched.
    */
    fn with_com<T>(work: impl FnOnce() -> windows::core::Result<T>) -> windows::core::Result<T> {
        use std::cell::Cell;

        thread_local! {
            static DONE: Cell<bool> = const { Cell::new(false) };
        }

        DONE.with(|done| {
            if done.get() {
                return;
            }

            // SAFETY: initialises the calling thread's apartment and nothing
            // else. The result is deliberately dropped: every way this can
            // fail means COM is already usable on this thread.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }

            done.set(true);
        });

        work()
    }

    fn manager() -> windows::core::Result<IVirtualDesktopManager> {
        // SAFETY: a documented shell object with no arguments. COM is up,
        // because every caller is inside `with_com`.
        unsafe { CoCreateInstance(&VirtualDesktopManager, None, CLSCTX_ALL) }
    }

    /// Which desktop a window is on, as the documented interface answers.
    ///
    /// `None` covers a window that has closed and a window Windows will not
    /// place, which is not the same as an error worth showing anybody: the
    /// caller decides what an unknown desktop means.
    pub fn of_window(id: isize) -> Option<DesktopId> {
        with_com(|| {
            // SAFETY: the handle is passed straight through and the identity
            // is written into a value owned here.
            unsafe { manager()?.GetWindowDesktopId(hwnd_of(id)) }
        })
        .ok()
        .map(id_of)
        // A window with no desktop yet reads back as all zeroes rather than as
        // an error, and treating that as an identity would compare equal to
        // the next window in the same state.
        .filter(|desktop| desktop.0 != 0)
    }

    /// Whether a window is on the desktop that is on screen.
    pub fn on_current(id: isize) -> Option<bool> {
        with_com(|| {
            // SAFETY: as above.
            unsafe { manager()?.IsWindowOnCurrentVirtualDesktop(hwnd_of(id)) }
        })
        .ok()
        .map(|on| on.as_bool())
    }

    /**
    Which of these windows are on a desktop other than the one on screen.

    A batch, because the caller is the window enumeration and this must not
    stand a manager up once per window. Called with the handles that are
    cloaked and nothing else, which on an ordinary machine is a very short
    list: a cloaked window is either a suspended store application or a window
    on another desktop, and only the second kind belongs in a switcher.

    **Both conditions are required.** A suspended `TextInputHost` is cloaked,
    reports itself as on the current desktop, and has no desktop identity at
    all. Asking for the identity as well as the flag is what tells the two
    apart, and asking for both means the uncertain case falls out as "not
    elsewhere", which is what the list already does today.
    */
    pub fn elsewhere(ids: &[isize]) -> Vec<(isize, DesktopId)> {
        if ids.is_empty() {
            return Vec::new();
        }

        with_com(|| {
            let manager = manager()?;
            let mut found = Vec::new();

            for id in ids {
                // SAFETY: both calls take a handle and write into values owned
                // here. A closed window is an error, not a crash.
                unsafe {
                    let Ok(desktop) = manager.GetWindowDesktopId(hwnd_of(*id)) else {
                        continue;
                    };

                    if desktop.to_u128() == 0 {
                        continue;
                    }

                    let Ok(on) = manager.IsWindowOnCurrentVirtualDesktop(hwnd_of(*id)) else {
                        continue;
                    };

                    if !on.as_bool() {
                        found.push((*id, id_of(desktop)));
                    }
                }
            }

            Ok(found)
        })
        .unwrap_or_default()
    }

    /**
    Sends a window to a desktop, through the documented interface.

    **This refuses any window belonging to another process**, which is every
    window a launcher has to deal with. Kept because it is the call the probe
    holds that finding against: the day a Windows lets it through, the probe
    fails and moving windows between desktops becomes something Sill can
    offer without an undocumented mutating call. Nothing else calls it.
    */
    pub fn send(id: isize, to: DesktopId) -> Result<(), String> {
        with_com(|| {
            let guid = GUID::from_u128(to.0);
            // SAFETY: the identity outlives the call, which is synchronous.
            unsafe { manager()?.MoveWindowToDesktop(hwnd_of(id), &guid) }
        })
        .map_err(|err| format!("that window would not move desktop: {err}"))
    }

    /**
    Every desktop, in the order Task View shows them.

    The only place the undocumented interface is used, and the reason it is
    used at all: nothing documented can count desktops or order them.

    Three questions are asked and nothing is told. The count is read first and
    held against the length of the array, which is the cheapest possible check
    on having read the right vtable at all, and the desktop the undocumented
    interface calls current is handed back for [`super::agrees`] to hold
    against the documented one.
    */
    pub fn ordered() -> Result<(Vec<DesktopId>, DesktopId), String> {
        with_com(|| {
            // SAFETY: a shell object with no arguments, then one service
            // lookup that either hands back the pinned interface or an error.
            // Every pointer below is taken ownership of immediately, so each
            // is released exactly once.
            unsafe {
                let host: IServiceProvider = CoCreateInstance(&IMMERSIVE_SHELL, None, CLSCTX_ALL)?;
                let internal: Internal = host.QueryService(&DESKTOP_SERVICE)?;

                let mut count = 0u32;
                (Interface::vtable(&internal).GetCount)(internal.as_raw(), &mut count).ok()?;

                let mut current = core::ptr::null_mut();
                (Interface::vtable(&internal).GetCurrentDesktop)(internal.as_raw(), &mut current)
                    .ok()?;
                let current = Desktop::from_raw(current);

                let mut here = GUID::zeroed();
                (Interface::vtable(&current).GetID)(current.as_raw(), &mut here).ok()?;

                let mut array = core::ptr::null_mut();
                (Interface::vtable(&internal).GetDesktops)(internal.as_raw(), &mut array).ok()?;
                let array = IObjectArray::from_raw(array);

                let held = array.GetCount()?;
                if held != count {
                    return Err(windows::core::Error::from_hresult(
                        windows::Win32::Foundation::E_UNEXPECTED,
                    ));
                }

                let mut desktops = Vec::with_capacity(held as usize);
                for at in 0..held {
                    let desktop: Desktop = array.GetAt(at)?;
                    let mut id = GUID::zeroed();
                    (Interface::vtable(&desktop).GetID)(desktop.as_raw(), &mut id).ok()?;
                    desktops.push(id_of(id));
                }

                Ok((desktops, id_of(here)))
            }
        })
        .map_err(|err| format!("Windows would not list the virtual desktops: {err}"))
    }

    /// This machine's Windows build, as a number.
    ///
    /// From the registry rather than from `GetVersionEx`, which reports what
    /// an application's manifest says it supports rather than what is running.
    /// A build that cannot be read is zero, which [`super::reach`] treats as
    /// older than anything and turns the feature off.
    pub fn build() -> u32 {
        crate::apps::read_string(
            windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "CurrentBuildNumber",
        )
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::DesktopId;

    pub fn of_window(_id: isize) -> Option<DesktopId> {
        None
    }
    pub fn on_current(_id: isize) -> Option<bool> {
        None
    }
    pub fn elsewhere(_ids: &[isize]) -> Vec<(isize, DesktopId)> {
        Vec::new()
    }
    pub fn send(_id: isize, _to: DesktopId) -> Result<(), String> {
        Err("windows only".to_string())
    }
    pub fn ordered() -> Result<(Vec<DesktopId>, DesktopId), String> {
        Err("windows only".to_string())
    }
    pub fn build() -> u32 {
        0
    }
}

pub use platform::{build, of_window, on_current, send};

/**
Every desktop, in Task View's order, or a reason there is no such list.

The gate, in one place. The build has to be one the undocumented half was run
against, the pinned interface has to answer, and what it says has to agree
with what the documented interface says about the same desktop. Anything else
is a refusal, and a refusal here costs a number on a row rather than a
feature: the window is still listed and still says it is elsewhere.
*/
pub fn desktops() -> Result<Vec<DesktopId>, String> {
    if reach(build()) != Reach::Ordered {
        return Err(
            "Sill has not been tested against this build of Windows, so it will not ask it \
             how many virtual desktops there are"
                .to_string(),
        );
    }

    let (desktops, internal) = platform::ordered()?;

    let documented = here().ok_or_else(|| "Windows would not name this desktop".to_string())?;

    if !agrees(&desktops, internal, documented) {
        return Err(
            "Windows described this desktop two different ways, so Sill will not act on either"
                .to_string(),
        );
    }

    Ok(desktops)
}

/// The desktop on screen, through the documented interface only.
///
/// Read off a window rather than asked for directly, because the documented
/// interface has no "which desktop is showing" question: it only answers about
/// windows. The window in front is on the desktop in front.
pub fn here() -> Option<DesktopId> {
    of_window(crate::windowing::front()?)
}

/**
Which of these windows are somewhere else, and which desktop each is on.

What the window list calls with the cloaked handles. The number is `None`
either because this Windows is not one the ordered list may be read on, or
because the desktop has been closed since; both mean the row says "on another
desktop" rather than naming one, and neither is a reason to drop the window.

**The ordered list is only fetched when something was found**, so the
undocumented interface is never touched on the ordinary machine where every
window is on the desktop being looked at.
*/
pub fn elsewhere(ids: &[isize]) -> Vec<(isize, Option<usize>)> {
    let found = platform::elsewhere(ids);
    if found.is_empty() {
        return Vec::new();
    }

    let ordered = desktops().unwrap_or_default();

    found
        .into_iter()
        .map(|(id, desktop)| (id, number_of(desktop, &ordered)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: DesktopId = DesktopId(1);
    const TWO: DesktopId = DesktopId(2);
    const THREE: DesktopId = DesktopId(3);

    fn three() -> Vec<DesktopId> {
        vec![ONE, TWO, THREE]
    }

    #[test]
    fn an_untested_build_gets_the_documented_half_and_no_more() {
        // The whole risk posture in one assertion. A Windows nobody has run
        // the probe against must not have the undocumented interface called
        // on it, and must still be able to find a window on another desktop.
        assert_eq!(reach(26200), Reach::Ordered);
        assert_eq!(reach(26201), Reach::Identity);
        assert_eq!(reach(22631), Reach::Identity);
        assert_eq!(reach(14393), Reach::Identity);
    }

    #[test]
    fn a_windows_without_the_documented_interface_gets_nothing() {
        assert_eq!(reach(DOCUMENTED_SINCE - 1), Reach::None);
        // A build that could not be read at all is zero, and zero must not
        // read as "new enough".
        assert_eq!(reach(0), Reach::None);
    }

    #[test]
    fn every_verified_build_is_one_the_probe_can_have_run_on() {
        // A typo here would silently turn the undocumented half on for a build
        // nobody tested, which is the one thing this module exists to prevent.
        for build in VERIFIED {
            assert!(
                *build >= DOCUMENTED_SINCE,
                "{build} is older than the documented interface"
            );
            assert_eq!(reach(*build), Reach::Ordered);
        }
    }

    #[test]
    fn a_desktop_is_numbered_the_way_task_view_numbers_it() {
        // Counted from one, because that is what Task View writes on them.
        assert_eq!(number_of(ONE, &three()), Some(1));
        assert_eq!(number_of(THREE, &three()), Some(3));
    }

    #[test]
    fn a_desktop_that_has_been_closed_is_not_given_a_number() {
        // Desktops can be closed between reading the list and drawing the row
        // that was built from it. A row that invented a number would point at
        // a desktop nobody has.
        assert_eq!(number_of(DesktopId(99), &three()), None);
        assert_eq!(number_of(ONE, &[]), None);
    }

    #[test]
    fn an_unnumbered_window_still_says_it_is_somewhere_else() {
        // What every Windows outside VERIFIED shows. It has to be useful on
        // its own, because it is not a rare case: it is most machines.
        assert_eq!(label(None), "on another desktop");
        assert_eq!(label(Some(2)), "on desktop 2");
    }

    #[test]
    fn the_tripwire_needs_both_halves_to_say_the_same_thing() {
        // This is what stands between a vtable read at the wrong offset and
        // Sill believing whatever it returned.
        assert!(agrees(&three(), TWO, TWO));

        // The undocumented interface named a desktop the documented one did
        // not.
        assert!(!agrees(&three(), TWO, THREE));

        // The list does not contain the desktop that is supposedly showing,
        // which is not a list of desktops.
        assert!(!agrees(&[ONE, TWO], THREE, THREE));

        // Nothing at all agrees with nothing.
        assert!(!agrees(&[], ONE, ONE));
    }

    #[test]
    fn a_desktop_identity_prints_the_way_windows_writes_one() {
        // Only ever seen in a log line, and a log line that says
        // "DesktopId(210...)" is not one anybody can match against Task View
        // or the registry.
        let id = DesktopId(0x9eed2078_e378_4e5e_b607_dc2f15c64937);
        assert_eq!(id.to_string(), "9eed2078-e378-4e5e-b607-dc2f15c64937");
    }

    #[test]
    fn asking_about_no_windows_asks_windows_nothing() {
        // The path every ordinary machine takes on every keystroke. It must
        // not stand up an apartment, a manager or a service host to answer a
        // question about an empty list.
        assert!(elsewhere(&[]).is_empty());
    }
}
