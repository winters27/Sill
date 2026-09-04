//! Pointing somebody else's open or save dialog at a folder.
//!
//! The trick Listary and Flow are kept installed for. A Save As dialog is on
//! screen, the folder it should be saving into is open in Explorer behind it,
//! and the only ways across are retyping the path or clicking down eight
//! levels. One key should do it.
//!
//! ## The whole risk is that this types into another program
//!
//! A file dialog belongs to another process. Sill has to put a path in it, and
//! this codebase already has a scar from the obvious way of doing that:
//! synthetic keystrokes go wherever the keyboard is pointing, and the keyboard
//! has been pointing at somebody's own document. So:
//!
//! - **Nothing is synthesised.** The filename box in a file dialog is a real
//!   control with its own window handle, and `WM_SETTEXT` addressed to that
//!   handle either reaches that control or reaches nothing. It cannot land in
//!   a window this module did not name. That is the entire reason the feature
//!   is built this way round and not with keystrokes.
//! - **The window is identified before it is touched**, by structure rather
//!   than by looking plausible. See [`locate`].
//! - **Refusing is the normal outcome.** A key pressed anywhere that is not a
//!   file dialog does nothing at all, and says why in the log. A key that does
//!   nothing is a small disappointment; a key that guesses is text in
//!   somebody's document.
//!
//! ## Telling a file dialog from any other window
//!
//! Every Win32 dialog has the class `#32770`, message boxes and property
//! sheets included, so the class alone says almost nothing. What it is good
//! for is the opposite: it is a few microseconds and it answers *no* for the
//! overwhelming majority of presses, before a single child window is
//! enumerated. The same shape as `explorer.rs` checking a class before
//! creating a COM object, and for the same reason: the miss is the common
//! case.
//!
//! A window that gets past that has to prove itself by its controls, and all
//! three of these have to be there:
//!
//! | Control | Why it settles the question |
//! | --- | --- |
//! | `SHELLDLL_DefView` | the embedded shell listing. A dialog that browses the filesystem has one and a message box does not, whichever generation it is |
//! | a filename field | the thing a path is actually put into. See [`locate`] |
//! | an accept button | `IDOK`, which is what turns a typed path into a navigation |
//!
//! **The two generations are not built alike, and the difference is not
//! cosmetic.** The explorer-styled `GetOpenFileName` window puts its shell
//! view and a `ComboBoxEx32` under control id 1148 among the dialog's own
//! children. The modern `IFileDialog` window has **no filename control among
//! them at all**: the box is an unnamed `ComboBox` four levels down inside
//! `DUIViewWndClassName`, and the shell view is down there too. Both were
//! dumped off real windows on this machine, and the first version of this
//! module handled only the first and refused every ordinary Save As box.
//!
//! Asking about the parts rather than about which kind of dialog it is means a
//! dialog nobody here has seen either has the parts, in which case it works,
//! or does not, in which case the key does nothing.
//!
//! ## Setting the path rather than typing it
//!
//! [`jump_to`] writes the folder into the filename field with `WM_SETTEXT`,
//! **reads it back**, and only then tells the dialog's accept button it was
//! clicked. The read-back is not belt and braces: if the text did not land,
//! clicking accept would accept whatever the person had already typed, which
//! in a Save dialog is a file being written to the wrong place. No text, no
//! click.
//!
//! Everything crosses the process boundary through `SendMessageTimeout`.
//! `WM_SETTEXT` and `WM_GETTEXT` are two of the messages the window manager
//! marshals between processes, which is what makes this work at all, and the
//! timeout is what stops a hung dialog holding the launcher. Nothing is
//! posted: a posted `WM_SETTEXT` carries a pointer into this process's memory
//! and would be read as one in the other's.
//!
//! Then it waits for the dialog to clear the box, which is the dialog saying
//! it has moved, and only afterwards puts a name back in. Writing into that
//! control any sooner leaves the dialog and its own box disagreeing about what
//! is in it, and the next thing done to the dialog silently stops working.
//! Measured, twice, before it was believed.
//!
//! ## What it costs between two presses
//!
//! Nothing. There is no hook, no timer and nothing watching for a dialog to
//! appear: the question "is a file dialog in front" is asked once, while the
//! key is down, and answered from a handle that already exists.

/// The class every Win32 dialog has, file dialogs among them.
///
/// Deliberately not treated as evidence of anything on its own. See the module
/// note: a message box is one of these too, and so is half of the options
/// screen in any application old enough to have one.
pub const DIALOG: &str = "#32770";

/// The cheap half of "is there a dialog to jump in".
///
/// Answered from a class name before any control is enumerated, because a key
/// bound to this is pressed in an editor and a browser far more often than in
/// a Save dialog, and in those the honest answer costs a string comparison.
pub fn could_be_dialog(class: &str) -> bool {
    class == DIALOG
}

/// One control in somebody else's dialog, as a decision can see it.
///
/// A flat list with parent handles rather than a tree, because that is the
/// shape `EnumChildWindows` hands over and rebuilding it into a tree in the
/// Win32 layer would move the deciding into the part that cannot be tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub handle: isize,
    /// The window this one is a child of. The dialog itself for a direct
    /// child, and some other control for anything deeper.
    pub parent: isize,
    pub class: String,
    /// The dialog control id, which is how a file dialog's parts are named.
    pub id: i32,
}

/// Why a window was not treated as a file dialog.
///
/// Named cases rather than a bare `None`, because "the key did nothing" is the
/// outcome somebody will report and the log line has to be able to say which
/// of these it was. They are also the four fixtures worth writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not a dialog at all, or nothing in front.
    NotADialog,
    /// A dialog, but not one that browses the filesystem.
    NoShellView,
    /// A shell view, but nowhere to put a path.
    NoFileNameField,
    /// A filename field, but nothing to press to navigate.
    NoAcceptButton,
}

impl Refusal {
    /// One sentence, in the words of somebody who did not write this.
    ///
    /// It is what the action panel shows and what the log line says when a
    /// bound key appears to have done nothing, so it names the window rather
    /// than the mechanism: nobody pressing a key has a view about which shell
    /// control class was missing.
    pub fn reason(self) -> &'static str {
        match self {
            Refusal::NotADialog => "There is no open or save dialog in front.",
            Refusal::NoShellView => {
                "The window in front is a dialog, but not one that browses files."
            }
            Refusal::NoFileNameField => "The dialog in front has no file name box.",
            Refusal::NoAcceptButton => "The dialog in front has no button to navigate with.",
        }
    }
}

/// The two handles a jump writes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fields {
    /// The edit control the path goes into.
    pub edit: isize,
    /// The `IDOK` button, which turns a path in that box into a navigation.
    pub accept: isize,
}

/// A file dialog, once it has proved it is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialog {
    pub window: isize,
    pub fields: Fields,
}

/// The shell's own listing, embedded in the dialog.
///
/// The single most useful thing in the tree. It is present in both generations
/// of file dialog because both of them host a shell view, and it is absent
/// from message boxes, property sheets, print dialogs and the options screens
/// that make up nearly every other `#32770` on a machine.
const SHELL_VIEW: &str = "SHELLDLL_DefView";

/// `IDOK`. Open, Save, and whatever a program renamed its accept button to.
const ACCEPT: i32 = 1;

/// The control ids the older layout puts its filename field under.
///
/// `1148` is the `ComboBoxEx32` in the explorer-styled `GetOpenFileName`
/// window. `1152` is `edt1` and `1153` is `cmb13`, which are what the plain
/// common dialog has had since Windows 3.1 and what an application calling
/// `GetSaveFileName` with an old template gets.
///
/// Read together with the requirement below that the control is a direct child
/// of the dialog. An id is three digits and something else in a deep control
/// tree will eventually collide with one; the dialog's own children are the
/// ones these ids were assigned by.
const FILE_NAME: [i32; 3] = [1148, 1152, 1153];

/// The id an editable combo box gives the edit inside it.
///
/// What finds the box in the modern `IFileDialog`, which does not put a
/// filename control among the dialog's own children at all: it hosts one deep
/// inside `DUIViewWndClassName`, as an unnamed `ComboBox` whose `Edit` carries
/// this. Confirmed against a real Save dialog rather than assumed, and the
/// difference between the two layouts is real: the same machine's Open dialog
/// had the `1148` combo and no such host, and its shell view was a direct
/// child rather than four levels down.
const COMBO_EDIT: i32 = 1001;

/// The class of the thing an editable combo's edit sits in.
const COMBO: &str = "ComboBox";

/**
Which control takes the path, if this window is a file dialog at all.

The decision, as a function over values, so the fixtures below can ask it
about a message box without a message box existing. The Win32 half does no
deciding: it reads class names, control ids and parents, and hands them here.

The order of the checks is the order of certainty rather than the order of
use. A shell view is what makes this a window that browses files at all, so it
is asked first and its absence is the refusal worth reporting: an application's
own options dialog with a path box in it will fail here rather than on the
filename field, and "does not browse files" is the true sentence about it.
*/
pub fn locate(dialog: isize, controls: &[Control]) -> Result<Fields, Refusal> {
    if !controls.iter().any(|control| control.class == SHELL_VIEW) {
        return Err(Refusal::NoShellView);
    }

    let edit = named_field(dialog, controls)
        .or_else(|| hosted_field(controls))
        .ok_or(Refusal::NoFileNameField)?;

    let accept = controls
        .iter()
        .find(|control| control.id == ACCEPT && control.class == "Button")
        .map(|control| control.handle)
        .ok_or(Refusal::NoAcceptButton)?;

    Ok(Fields { edit, accept })
}

/// The filename box where a common dialog names it by id.
///
/// A direct child of the dialog, which is where a common dialog puts the parts
/// it names. Either the edit itself, in the oldest layout, or the combo that
/// holds one.
fn named_field(dialog: isize, controls: &[Control]) -> Option<isize> {
    let field = controls
        .iter()
        .find(|control| control.parent == dialog && FILE_NAME.contains(&control.id))?;

    if field.class == "Edit" {
        Some(field.handle)
    } else {
        edit_under(field.handle, controls)
    }
}

/**
The filename box where the modern dialog hides it.

The one editable combo box in the whole tree. That is a narrower thing than it
sounds: a file dialog's other combo is the file-type filter, which is a
drop-down list and has no edit inside it at all, and the address bar's edit
carries the band's id rather than a combo's.

**Exactly one, or none.** Two editable combos would mean this window is not
built the way a file dialog is, and picking the first of them would be the
guess the whole module refuses to make. Reached only after the shell view has
already established that this browses files, so it is a way of finding a known
control rather than a way of deciding what the window is.
*/
fn hosted_field(controls: &[Control]) -> Option<isize> {
    let inside_a_combo = |control: &&Control| {
        control.class == "Edit"
            && control.id == COMBO_EDIT
            && controls
                .iter()
                .any(|parent| parent.handle == control.parent && parent.class == COMBO)
    };

    let mut found = controls.iter().filter(inside_a_combo);
    let one = found.next()?;

    found.next().is_none().then_some(one.handle)
}

/// The edit control somewhere beneath this one.
///
/// Breadth first, because the combo's edit is its grandchild and anything
/// deeper than that is not the box. Bounded by the list it walks, which is the
/// dialog's own children: a cycle in the parent links would be a corrupt
/// enumeration rather than something to defend against, and there is none,
/// because a window cannot be its own ancestor.
fn edit_under(parent: isize, controls: &[Control]) -> Option<isize> {
    let mut generation = vec![parent];

    while !generation.is_empty() {
        let children: Vec<&Control> = controls
            .iter()
            .filter(|control| generation.contains(&control.parent))
            .collect();

        if let Some(edit) = children.iter().find(|control| control.class == "Edit") {
            return Some(edit.handle);
        }

        generation = children.iter().map(|control| control.handle).collect();
    }

    None
}

/// What a jump is going to do, worked out before anything is written anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jump {
    /// The folder to navigate to. Always a folder, never a file.
    pub folder: String,
    /// What goes back in the filename box once the navigation has happened.
    ///
    /// Empty for a jump to a folder from a box nobody had typed in. Otherwise
    /// either the name of the file that was jumped to, or the name the person
    /// had already typed and would have lost.
    pub name: String,
}

/**
Where a jump goes, and what is left in the box afterwards.

**A folder is navigated to; a file is never accepted.** Putting a file's full
path in the box and pressing the accept button is how a jump turns into
opening a file nobody asked to open, or into an overwrite prompt in a Save
dialog. So a file resolves to its own folder plus its name, the navigation
lands in that folder, and the name sits in the box waiting for the person to
press Enter themselves. One extra keystroke, and it is the keystroke that
means "yes, that one".

The name in the box is also how a half-typed filename survives. Somebody
typing `report.txt` in a Save dialog and then jumping to another folder has not
changed their mind about the name, and a jump that cleared it would be
reasonably described as having eaten it.

Refuses a path that is not absolute. A dialog navigates relative to wherever it
currently is, so `..\\notes` means something different depending on a state
this code cannot see, and the thing it means is not what anybody pressed the
key for.
*/
pub fn plan(target: &str, folder: bool, typed: &str) -> Result<Jump, String> {
    let target = target.trim();

    if target.is_empty() {
        return Err("There is no path to jump to.".to_string());
    }

    if !absolute(target) {
        return Err(format!("{target} is not a full path."));
    }

    if folder {
        return Ok(Jump {
            folder: tidy(target),
            // Whatever was in the box, when that was a name rather than a path
            // somebody was part-way through typing.
            name: plain_name(typed).unwrap_or_default(),
        });
    }

    let at = target
        .rfind(['\\', '/'])
        .ok_or_else(|| format!("{target} is not in a folder."))?;

    let name = target[at + 1..].to_string();
    if name.is_empty() {
        return Err(format!("{target} has no file name."));
    }

    Ok(Jump {
        folder: tidy(&target[..at + 1]),
        name,
    })
}

/// Whether a path names somewhere on its own, without a folder to be read
/// relative to.
///
/// A drive letter or a UNC share. Deliberately not `Path::is_absolute`, which
/// calls `\\notes` absolute on Windows: that is a path rooted on the current
/// drive, and which drive that is depends on where the dialog happens to be.
fn absolute(path: &str) -> bool {
    if path.starts_with(r"\\") {
        return true;
    }

    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// A folder path without the trailing separator, except where that is the
/// whole path.
///
/// `C:\\` is a folder and `C:` is a drive-relative path meaning "wherever this
/// process last was on C", which is exactly the ambiguity [`absolute`] exists
/// to refuse. Trimming it off the root would hand the dialog the one string
/// this module rejects everywhere else.
fn tidy(folder: &str) -> String {
    let trimmed = folder.trim_end_matches(['\\', '/']);

    if trimmed.len() < 3 {
        format!("{trimmed}\\")
    } else {
        trimmed.to_string()
    }
}

/// What was in the filename box, if it was a name rather than a path.
///
/// Anything with a separator or a colon in it is something the person was
/// part-way through typing as a location, and putting it back after a jump
/// somewhere else would leave the box saying one thing and the dialog showing
/// another.
fn plain_name(typed: &str) -> Option<String> {
    let typed = typed.trim();

    (!typed.is_empty() && !typed.contains(['\\', '/', ':'])).then(|| typed.to_string())
}

/// The file dialog in front, if what is in front is one.
///
/// The foreground window, unless that is Sill's own, in which case it is the
/// window the launcher is covering. Both are needed and they are the same
/// feature from two ends: a bound key is pressed with the dialog in front, and
/// the action panel's entry is chosen with the launcher in front and the
/// dialog behind it. `summon` already remembers the second one, so this costs
/// a read of a handle rather than another enumeration.
#[cfg(windows)]
pub fn in_front() -> Result<Dialog, Refusal> {
    let window = windows_impl::front_of_another_process();

    if window == 0 || !could_be_dialog(&crate::windowing::class_of(window)) {
        return Err(Refusal::NotADialog);
    }

    let fields = locate(window, &windows_impl::controls_of(window))?;

    Ok(Dialog { window, fields })
}

#[cfg(not(windows))]
pub fn in_front() -> Result<Dialog, Refusal> {
    Err(Refusal::NotADialog)
}

/// Every control in a window, which is what [`locate`] decides from.
///
/// Public because it is the seam between the part Windows decides and the part
/// this module decides, and the seam is the thing worth being able to look at.
/// The fixtures below describe what a file dialog is built like; only a real
/// window can say whether that description is true, and `suite/real_dialog.rs`
/// prints one to check.
#[cfg(windows)]
pub fn controls_of(window: isize) -> Vec<Control> {
    windows_impl::controls_of(window)
}

#[cfg(not(windows))]
pub fn controls_of(_window: isize) -> Vec<Control> {
    Vec::new()
}

/// What is in the dialog's filename box right now.
#[cfg(windows)]
pub fn typed_in(dialog: &Dialog) -> String {
    windows_impl::text_of(dialog.fields.edit)
}

#[cfg(not(windows))]
pub fn typed_in(_dialog: &Dialog) -> String {
    String::new()
}

/// Puts the path in the box and presses the dialog's own accept button.
///
/// Blocking, and the caller is expected to have taken it off the runtime: each
/// step waits for another process to answer, bounded by [`PATIENCE`].
#[cfg(windows)]
pub fn jump_to(dialog: &Dialog, jump: &Jump) -> Result<(), String> {
    windows_impl::jump_to(dialog, jump)
}

#[cfg(not(windows))]
pub fn jump_to(_dialog: &Dialog, _jump: &Jump) -> Result<(), String> {
    Err("windows only".to_string())
}

/// How long another process gets to answer one message.
///
/// Generous, because the message that matters is a navigation: the dialog goes
/// away and enumerates a folder before it answers, and that folder may be on a
/// share. It is not a budget for the happy case, which is immediate; it is the
/// line past which the dialog is assumed to be stuck and the launcher stops
/// waiting for it. Same reasoning and the same shape as
/// `explorer::PATIENCE`, which is about a shell that hangs on a disconnected
/// drive.
pub const PATIENCE: u32 = 3_000;

/// How long the dialog gets to finish moving before the jump is called a
/// failure.
///
/// Not a timer and not a poll in the sense rule 23 forbids: nothing here runs
/// unless a key was pressed, it stops the moment the dialog answers, and it is
/// gone before the keypress is. A navigation into a folder on a share can take
/// a moment, and the alternative to waiting for it is claiming it happened.
pub const SETTLES_WITHIN: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(windows)]
mod windows_impl {
    use super::{Control, Dialog, Jump, PATIENCE, SETTLES_WITHIN};

    use windows::core::BOOL;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetDlgCtrlID, GetForegroundWindow, GetParent, GetWindowThreadProcessId,
        SendMessageTimeoutW, BM_CLICK, SMTO_ABORTIFHUNG, WM_GETTEXT, WM_SETTEXT,
    };

    /// The window a jump means, or zero.
    ///
    /// Sill's own window is never it. A key pressed while the launcher is open
    /// means the thing behind the launcher, which is the same rule
    /// `windowing::foreground` follows and the same one `summon` records for
    /// handing focus back on dismissal.
    pub(super) fn front_of_another_process() -> isize {
        // SAFETY: takes nothing, returns a handle, dereferences nothing.
        let front = unsafe { GetForegroundWindow() };

        if !front.0.is_null() && !ours(front) {
            return front.0 as isize;
        }

        // The launcher is in front, so the dialog is the window it is covering,
        // which was recorded when the launcher appeared.
        crate::summon::previous_foreground().unwrap_or(0)
    }

    fn ours(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        // SAFETY: fills a u32 declared here.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid == std::process::id()
    }

    /// Every control in the dialog, with enough about each to decide by.
    ///
    /// `EnumChildWindows` walks the whole tree rather than one level, so the
    /// combo's edit arrives here alongside the combo, and the parent handle is
    /// what puts them back in order.
    pub(super) fn controls_of(dialog: isize) -> Vec<Control> {
        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            // SAFETY: the pointer is the Vec passed in below, which outlives
            // the enumeration because EnumChildWindows is synchronous.
            let found = unsafe { &mut *(lparam.0 as *mut Vec<Control>) };

            // SAFETY: all three take the handle the enumeration just supplied.
            // A window with no parent is not one of these, so a failed
            // GetParent becomes a parent of zero and matches nothing.
            let (parent, id) = unsafe {
                (
                    GetParent(hwnd).map(|p| p.0 as isize).unwrap_or(0),
                    GetDlgCtrlID(hwnd),
                )
            };

            found.push(Control {
                handle: hwnd.0 as isize,
                parent,
                class: crate::windowing::class_of(hwnd.0 as isize),
                id,
            });

            BOOL(1)
        }

        let mut found: Vec<Control> = Vec::new();

        // SAFETY: the callback matches the required signature and the pointer
        // points at a live Vec for the duration of this synchronous call.
        unsafe {
            let _ = EnumChildWindows(
                Some(HWND(dialog as *mut core::ffi::c_void)),
                Some(collect),
                LPARAM(&mut found as *mut Vec<Control> as isize),
            );
        }

        found
    }

    /// A control's text, read across the process boundary.
    ///
    /// `WM_GETTEXT` is marshalled by the window manager, which is what lets a
    /// buffer in this process be filled by a control in another one. Bounded
    /// by the buffer rather than by asking the length first: a filename box
    /// holds a path, and a path that does not fit in this is not one this
    /// module was going to put back anyway.
    pub(super) fn text_of(control: isize) -> String {
        let mut buffer = [0u16; 1024];
        let mut answered = 0usize;

        // SAFETY: the buffer is a local and its length in characters is passed
        // honestly. The call writes at most that many, including the
        // terminator, and the result is only read on success.
        let sent = unsafe {
            SendMessageTimeoutW(
                HWND(control as *mut core::ffi::c_void),
                WM_GETTEXT,
                WPARAM(buffer.len()),
                LPARAM(buffer.as_mut_ptr() as isize),
                SMTO_ABORTIFHUNG,
                PATIENCE,
                Some(&mut answered),
            )
        };

        if sent.0 == 0 {
            return String::new();
        }

        let written = answered.min(buffer.len() - 1);
        String::from_utf16_lossy(&buffer[..written])
    }

    /// Puts text in a control, and says whether the control took it.
    fn set_text(control: isize, text: &str) -> bool {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let mut answered = 0usize;

        // SAFETY: the string is a local that outlives the call, which is
        // synchronous. `WM_SETTEXT` is one of the messages the window manager
        // marshals across processes, so the other side receives a copy rather
        // than this pointer.
        let sent = unsafe {
            SendMessageTimeoutW(
                HWND(control as *mut core::ffi::c_void),
                WM_SETTEXT,
                WPARAM(0),
                LPARAM(PCWSTR(wide.as_ptr()).0 as isize),
                SMTO_ABORTIFHUNG,
                PATIENCE,
                Some(&mut answered),
            )
        };

        sent.0 != 0
    }

    pub(super) fn jump_to(dialog: &Dialog, jump: &Jump) -> Result<(), String> {
        if !set_text(dialog.fields.edit, &jump.folder) {
            return Err("the dialog did not answer".to_string());
        }

        /*
         * Read back before anything is pressed.
         *
         * This is the check that makes the whole feature safe rather than
         * merely careful. If the text did not land, pressing the dialog's
         * accept button accepts whatever was already in the box, and in a Save
         * As dialog that is a file written somewhere nobody chose. The failure
         * mode of a set that silently did nothing has to be a key that does
         * nothing, not a key that saves.
         */
        let landed = text_of(dialog.fields.edit);
        if landed.trim() != jump.folder {
            return Err(format!(
                "the dialog did not take the path: its box says {landed:?}"
            ));
        }

        /*
         * The dialog's own accept button, told it was clicked.
         *
         * `BM_CLICK` addressed to that one button handle, which is not a
         * keystroke and not a click at a screen position: the mouse is not
         * moved, no input is synthesised, and nothing can reach a window this
         * module did not name.
         *
         * **Measured against a real dialog rather than reasoned about.**
         * `suite/real_dialog.rs::which_message_navigates` puts a folder in the
         * box of four freshly opened Save dialogs, applies one message each,
         * and then makes each dialog resolve a bare name to prove where it
         * ended up. All four move it: `BM_CLICK`, `WM_COMMAND` sent, the same
         * posted, and a posted `VK_RETURN`. `BM_CLICK` is the one used because
         * it is sent to one button handle rather than posted to a queue, so a
         * delivery that fails is something this function can see rather than
         * something it finds out about by the jump silently not happening.
         */
        let mut answered = 0usize;

        // SAFETY: the handle came from this dialog's own enumeration and is
        // not dereferenced here.
        let sent = unsafe {
            SendMessageTimeoutW(
                HWND(dialog.fields.accept as *mut core::ffi::c_void),
                BM_CLICK,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                PATIENCE,
                Some(&mut answered),
            )
        };

        if sent.0 == 0 {
            return Err("the dialog did not navigate".to_string());
        }

        /*
         * Then wait for the dialog to say it has moved, by clearing the box.
         *
         * Not politeness and not a settling delay picked out of the air. The
         * first version wrote the name back the instant `BM_CLICK` returned,
         * and **the next thing done to that dialog stopped working**: a bare
         * file name set in the box and accepted did nothing at all, on a
         * dialog where the identical sequence worked when nothing had been
         * written immediately after the click. Writing into a control while
         * the dialog is part-way through rebuilding its view leaves the two
         * disagreeing about what is in it.
         *
         * Clearing the box is the dialog's own signal that it navigated rather
         * than a guess about how long that takes, which is also why the wait
         * running out is a refusal with a true sentence in it: the box still
         * holds the path this function put there, so the jump did not happen
         * and saying it did would be a lie the person finds out about when
         * they press Enter.
         */
        let started = std::time::Instant::now();
        while text_of(dialog.fields.edit).trim() == jump.folder {
            if started.elapsed() > SETTLES_WITHIN {
                return Err("the dialog did not move to that folder".to_string());
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        /*
         * And the box gets its name back.
         *
         * Navigating clears it, so without this a jump made in the middle of
         * typing a filename eats the filename, and a jump to a file leaves the
         * person in the right folder with nothing selected.
         */
        if !jump.name.is_empty() {
            set_text(dialog.fields.edit, &jump.name);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog handle every fixture hangs off.
    const WINDOW: isize = 0x1000;

    fn control(handle: isize, parent: isize, class: &str, id: i32) -> Control {
        Control {
            handle,
            parent,
            class: class.to_string(),
            id,
        }
    }

    /// The explorer-styled `GetOpenFileName` window, from the real one.
    ///
    /// Its shell view is a direct child and its filename box is three deep: a
    /// `ComboBoxEx32` under control id 1148, a `ComboBox` inside it, and the
    /// `Edit` inside that. Nothing about the dialog says which of the three
    /// takes the text, so the descent is the thing under test.
    fn explorer_styled() -> Vec<Control> {
        vec![
            control(0x1003, WINDOW, "SHELLDLL_DefView", 0),
            control(0x1010, WINDOW, "ComboBoxEx32", 1148),
            control(0x1011, 0x1010, "ComboBox", 1001),
            control(0x1012, 0x1011, "Edit", 1001),
            control(0x1020, WINDOW, "Button", 1),
            control(0x1021, WINDOW, "Button", 2),
            control(0x1030, WINDOW, "ListBox", 1120),
            control(0x1031, WINDOW, "Static", 1090),
        ]
    }

    /**
    The modern `IFileDialog` window, from the real one.

    Copied off a live Save dialog rather than imagined, because it is not
    built the way the older one is and guessing would have got it wrong. There
    is **no filename control among the dialog's own children at all**: the box
    is an unnamed `ComboBox` hosted four levels down inside the DirectUI tree,
    and only the `Edit` inside it carries an id.

    The second `ComboBox` is the file-type filter, and it is here on purpose.
    It is the thing that makes "find the combo" the wrong rule and "find the
    combo with an edit in it" the right one.
    */
    fn modern() -> Vec<Control> {
        vec![
            control(0x2001, WINDOW, "DUIViewWndClassName", 0),
            control(0x2002, 0x2001, "DirectUIHWND", 0),
            control(0x2003, 0x2002, "FloatNotifySink", 0),
            control(0x2004, 0x2003, "ComboBox", 0),
            control(0x2005, 0x2004, "Edit", 1001),
            control(0x2006, 0x2002, "CtrlNotifySink", 0),
            control(0x2007, 0x2006, "SHELLDLL_DefView", 1121),
            // The file-type filter: a combo with nothing to type in.
            control(0x2008, 0x2002, "FloatNotifySink", 0),
            control(0x2009, 0x2008, "ComboBox", 0),
            // The address bar, whose edit belongs to a band rather than to a
            // plain combo and carries the band's id.
            control(0x2010, WINDOW, "WorkerW", 0),
            control(0x2011, 0x2010, "ComboBoxEx32", 41477),
            control(0x2012, 0x2011, "ComboBox", 41477),
            control(0x2013, 0x2012, "Edit", 41477),
            control(0x2020, WINDOW, "Button", 1),
            control(0x2021, WINDOW, "Button", 2),
            control(0x2030, WINDOW, "ListBox", 1120),
        ]
    }

    /// The oldest `GetOpenFileName` window, which puts its edit straight in.
    fn classic() -> Vec<Control> {
        vec![
            control(0x3001, WINDOW, "SHELLDLL_DefView", 0),
            control(0x3010, WINDOW, "Edit", 1152),
            control(0x3011, WINDOW, "Static", 1090),
            control(0x3020, WINDOW, "Button", 1),
            control(0x3021, WINDOW, "Button", 2),
        ]
    }

    #[test]
    fn the_explorer_styled_dialog_takes_its_path_in_the_edit_inside_the_combo() {
        let fields = locate(WINDOW, &explorer_styled()).expect("a file dialog");

        assert_eq!(fields.edit, 0x1012, "the combo itself is not the edit");
        assert_eq!(fields.accept, 0x1020);
    }

    /// The layout that has no filename control among the dialog's children.
    ///
    /// The one this module got wrong first time: it looked only for the named
    /// ids, found none, and refused a perfectly ordinary Save As box. The
    /// address bar and the file-type filter are both in the fixture because
    /// both are things a looser rule picks up instead.
    #[test]
    fn the_modern_dialog_takes_its_path_in_the_combo_nobody_named() {
        let fields = locate(WINDOW, &modern()).expect("a file dialog");

        assert_eq!(fields.edit, 0x2005, "the address bar is not the file name");
        assert_eq!(fields.accept, 0x2020);
    }

    #[test]
    fn the_classic_dialog_takes_its_path_in_the_edit_itself() {
        let fields = locate(WINDOW, &classic()).expect("a file dialog");

        assert_eq!(fields.edit, 0x3010);
        assert_eq!(fields.accept, 0x3020);
    }

    /// Two boxes it could be means it is not known which.
    ///
    /// The fallback finds a control by shape rather than by name, so it only
    /// answers when the shape is unique. A second editable combo means this
    /// window is not built the way a file dialog is, and choosing the first
    /// would be the guess this module exists not to make.
    #[test]
    fn two_boxes_it_could_be_is_no_answer() {
        let mut ambiguous = modern();
        ambiguous.push(control(0x2040, 0x2009, "Edit", 1001));

        assert_eq!(locate(WINDOW, &ambiguous), Err(Refusal::NoFileNameField));
    }

    /**
    The one that decides whether this feature is safe to have.

    A message box is a `#32770` with an `IDOK` button, exactly like a file
    dialog, and it is the window most likely to be in front when somebody
    presses the key by mistake. Nothing here may be treated as a place to put
    a path.
    */
    #[test]
    fn a_message_box_is_not_a_file_dialog() {
        let box_ = vec![
            control(0x3001, WINDOW, "Static", 0xFFFF),
            control(0x3002, WINDOW, "Button", 1),
            control(0x3003, WINDOW, "Button", 2),
        ];

        assert_eq!(locate(WINDOW, &box_), Err(Refusal::NoShellView));
    }

    /// An application's own dialog with a path box in it.
    ///
    /// The realistic near miss: an options screen with a "Location" field and
    /// an OK button. It is a `#32770`, it has an `Edit`, and it browses
    /// nothing. Writing a path into it and pressing OK would apply a setting
    /// nobody chose.
    #[test]
    fn an_options_dialog_with_a_path_box_is_not_a_file_dialog() {
        let options = vec![
            control(0x4001, WINDOW, "Static", 1000),
            control(0x4002, WINDOW, "Edit", 1152),
            control(0x4003, WINDOW, "Button", 1),
        ];

        assert_eq!(locate(WINDOW, &options), Err(Refusal::NoShellView));
    }

    /// A shell view with no filename box.
    ///
    /// The folder picker, which browses folders and has no place to type one.
    /// There is nothing to set, so there is nothing to do, and guessing at the
    /// tree control instead is how a jump becomes a selection somebody did not
    /// make.
    #[test]
    fn a_dialog_that_browses_but_has_no_box_is_refused() {
        let picker = vec![
            control(0x5001, WINDOW, "SHELLDLL_DefView", 0),
            control(0x5002, WINDOW, "SysTreeView32", 100),
            control(0x5003, WINDOW, "Button", 1),
        ];

        assert_eq!(locate(WINDOW, &picker), Err(Refusal::NoFileNameField));
    }

    /// A filename field with nothing to press.
    #[test]
    fn a_dialog_with_no_accept_button_is_refused() {
        let mut half = classic();
        half.retain(|control| control.id != ACCEPT);

        assert_eq!(locate(WINDOW, &half), Err(Refusal::NoAcceptButton));
    }

    /// An id three digits long will collide with something eventually.
    ///
    /// A control deep inside somebody's dialog that happens to carry 1148 is
    /// not the file dialog's filename combo, and the requirement that the
    /// field is the dialog's own child is what says so.
    #[test]
    fn a_matching_id_that_is_not_the_dialogs_own_child_is_not_the_box() {
        let deep = vec![
            control(0x6001, WINDOW, "SHELLDLL_DefView", 0),
            control(0x6002, WINDOW, "SysListView32", 700),
            // Same id as the filename combo, three levels down inside a
            // control that has nothing to do with it.
            control(0x6003, 0x6002, "Edit", 1148),
            control(0x6004, WINDOW, "Button", 1),
        ];

        assert_eq!(locate(WINDOW, &deep), Err(Refusal::NoFileNameField));
    }

    /// The accept button has to be a button.
    #[test]
    fn something_that_is_not_a_button_is_not_the_accept_button() {
        let mut odd = classic();
        for control in &mut odd {
            if control.id == ACCEPT {
                control.class = "Static".to_string();
            }
        }

        assert_eq!(locate(WINDOW, &odd), Err(Refusal::NoAcceptButton));
    }

    /// A window with no children at all.
    #[test]
    fn a_dialog_with_nothing_in_it_is_refused() {
        assert_eq!(locate(WINDOW, &[]), Err(Refusal::NoShellView));
    }

    /// Every `#32770` on the machine is a candidate and almost none is a file
    /// dialog, so the class is a filter rather than an answer.
    #[test]
    fn only_a_dialog_class_is_worth_enumerating() {
        assert!(could_be_dialog("#32770"));

        for other in [
            "CabinetWClass",
            "Chrome_WidgetWin_1",
            "Notepad",
            "SHELLDLL_DefView",
            "",
        ] {
            assert!(!could_be_dialog(other), "{other}");
        }
    }

    #[test]
    fn a_folder_is_navigated_to_and_a_typed_name_survives() {
        let jump = plan(r"C:\work\reports", true, "summary.txt").expect("a folder");

        assert_eq!(jump.folder, r"C:\work\reports");
        assert_eq!(jump.name, "summary.txt");
    }

    /// A file jumps to the folder holding it and leaves its name in the box.
    ///
    /// Never accepted for the person. Putting the full path in and pressing
    /// the dialog's accept button opens that file, or in a Save dialog offers
    /// to overwrite it, and neither is what "jump here" means.
    #[test]
    fn a_file_is_never_the_thing_that_gets_accepted() {
        let jump = plan(r"C:\work\reports\q3.xlsx", false, "").expect("a file");

        assert_eq!(jump.folder, r"C:\work\reports");
        assert_eq!(jump.name, "q3.xlsx");
    }

    /// The file's own name wins over whatever was in the box.
    #[test]
    fn jumping_to_a_file_puts_that_files_name_in_the_box() {
        let jump = plan(r"C:\work\q3.xlsx", false, "draft.txt").expect("a file");

        assert_eq!(jump.name, "q3.xlsx");
    }

    /// A path half typed into the box is not a name to put back.
    #[test]
    fn a_path_in_the_box_is_not_carried_across() {
        for typed in [r"C:\other\thing.txt", "sub/folder", "C:", "  "] {
            let jump = plan(r"C:\work", true, typed).expect("a folder");
            assert_eq!(jump.name, "", "{typed:?} was carried across");
        }
    }

    /// The root of a drive keeps its separator.
    ///
    /// `C:\` is a folder; `C:` is "wherever this process last was on C", which
    /// is the one thing a dialog must never be handed, because where that is
    /// depends on state nothing here can see.
    #[test]
    fn the_root_of_a_drive_stays_a_root() {
        assert_eq!(plan(r"C:\", true, "").expect("a folder").folder, r"C:\");
        assert_eq!(plan(r"C:\\", true, "").expect("a folder").folder, r"C:\");

        let jump = plan(r"C:\notes.txt", false, "").expect("a file");
        assert_eq!(jump.folder, r"C:\");
        assert_eq!(jump.name, "notes.txt");
    }

    #[test]
    fn a_trailing_separator_is_dropped_from_a_real_folder() {
        assert_eq!(
            plan(r"C:\work\reports\", true, "")
                .expect("a folder")
                .folder,
            r"C:\work\reports"
        );
    }

    /// A share is a full path and a forward-slash path is one too.
    #[test]
    fn a_share_and_a_forward_slash_path_are_both_full_paths() {
        assert_eq!(
            plan(r"\\nas\media\film", true, "").expect("a share").folder,
            r"\\nas\media\film"
        );

        assert_eq!(
            plan("C:/work", true, "").expect("a folder").folder,
            "C:/work"
        );
    }

    /// Anything a dialog would read relative to where it already is.
    ///
    /// The dialog's current folder is state this code cannot see, so a
    /// relative path means something different every time the key is pressed,
    /// and none of those things is what was meant. `\notes` is in here on
    /// purpose: Windows calls it absolute and it is rooted on whichever drive
    /// the process was last on.
    #[test]
    fn a_path_that_needs_somewhere_to_start_from_is_refused() {
        for relative in [
            r"..\notes",
            r".\notes",
            "notes",
            r"\notes",
            "C:",
            "",
            "   ",
            "1:\\notes",
        ] {
            assert!(plan(relative, true, "").is_err(), "{relative:?}");
            assert!(plan(relative, false, "").is_err(), "{relative:?}");
        }
    }

    /// Every refusal says something a person could act on.
    #[test]
    fn every_refusal_has_a_sentence() {
        for refusal in [
            Refusal::NotADialog,
            Refusal::NoShellView,
            Refusal::NoFileNameField,
            Refusal::NoAcceptButton,
        ] {
            let reason = refusal.reason();
            assert!(
                reason.chars().next().is_some_and(char::is_uppercase),
                "{reason:?} is shown on its own and has to read as a sentence"
            );
            assert!(reason.ends_with('.'), "{reason:?}");
        }
    }
}
