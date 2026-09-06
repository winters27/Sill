//! Expanding a snippet where it was typed.
//!
//! A `WH_KEYBOARD_LL` hook keeps a rolling buffer of what has been typed, and
//! when the tail of that buffer is a snippet's keyword it deletes the keyword
//! and puts the snippet in its place.
//!
//! The hook itself follows the one already proven in
//! [`crate::dictation::hotkey`], with one important difference: **this one
//! never swallows a key.** Dictation's hook eats Enter and Escape while a
//! recording is running; this watches and lets everything through, so a
//! misbehaving snippet can never stop the keyboard working.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::snippets::store::Snippet;

/// How much typing is remembered.
///
/// Long enough for any reasonable keyword and short enough that the buffer is
/// never a meaningful record of what someone wrote. It lives in memory only
/// and is cleared by anything that means "a new context": Enter, Tab, Escape,
/// an arrow, or a click.
const BUFFER_CHARS: usize = 64;

/// What the hook has seen typed lately.
///
/// Separate from the matching so the whole rolling-buffer rule set can be
/// tested without a keyboard.
#[derive(Debug, Default)]
pub struct Typed {
    buffer: String,
}

impl Typed {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn as_str(&self) -> &str {
        &self.buffer
    }

    /// Records one typed character.
    pub fn push(&mut self, c: char) {
        // Whitespace ends a word but not a context: a keyword may be preceded
        // by a space, so the buffer keeps going.
        self.buffer.push(c);

        while self.buffer.chars().count() > BUFFER_CHARS {
            // Popping from the front by character, because a byte-wise drain
            // would split a multi-byte character in half.
            let mut chars = self.buffer.chars();
            chars.next();
            self.buffer = chars.as_str().to_string();
        }
    }

    /// Removes the last character, for a backspace.
    pub fn backspace(&mut self) {
        self.buffer.pop();
    }

    /// Forgets everything.
    ///
    /// Called for anything that means the caret has moved somewhere else:
    /// Enter, Tab, Escape, an arrow key, or a mouse click. Without this a
    /// keyword typed at the end of one line would still be sitting in the
    /// buffer three fields later.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Records every character one key event produced.
    ///
    /// More than one is ordinary rather than exotic. A key on a layout with a
    /// ligature produces two characters, a dead key followed by its base
    /// produces the accented form, and anything outside the basic plane, which
    /// is most emoji, is two UTF-16 units that are one character. This used to
    /// take the first unit and only when there was exactly one of them, so all
    /// three arrived as nothing or as half a character.
    pub fn push_all(&mut self, chars: &[char]) {
        for c in chars {
            self.push(*c);
        }
    }

    /// Drops the last `count` characters, after they have been replaced.
    pub fn consume(&mut self, count: usize) {
        for _ in 0..count {
            self.buffer.pop();
        }
    }
}

/// Backspace's virtual key.
const BACKSPACE: u32 = 0x08;

/// `VK_PROCESSKEY`.
///
/// Windows substitutes this for the real key when an input method editor has
/// taken it for a composition, which is every key typed into a Japanese,
/// Chinese or Korean IME while a word is being built. It is not a key and it
/// types nothing.
const PROCESS_KEY: u32 = 0xE5;

/// The most characters one key event can produce.
///
/// The size of the buffer handed to `ToUnicode`, so the two cannot disagree.
pub const PRODUCED_MAX: usize = 8;

/// Which modifiers were down when a key arrived.
///
/// Left and right Alt are separate because that is the only thing that tells
/// AltGr from a Ctrl+Alt shortcut. See [`Held::alt_gr`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
}

impl Held {
    /// Whether this is AltGr rather than two modifiers.
    ///
    /// **On a layout that has an AltGr key, right Alt is that key**, and the
    /// keyboard driver sends a synthetic left Ctrl press immediately before the
    /// right Alt press. So AltGr reaches a low-level hook as Ctrl down together
    /// with right Alt down, and there is no flag anywhere saying which of those
    /// two the user actually pressed.
    ///
    /// A genuine Ctrl+Alt chord is Ctrl with the **left** Alt, because that is
    /// the Alt a person reaches for when they mean Alt. That is the whole
    /// distinction, and it is the reason left and right Alt are held apart
    /// here rather than collapsed into `VK_MENU` the way the rest of Windows
    /// collapses them.
    ///
    /// It is not quite enough on its own. On a US layout right Alt is only Alt,
    /// so Ctrl plus right Alt is a real shortcut that happens to look exactly
    /// like AltGr. [`judge_key`] settles that case by asking Windows what the
    /// key types and treating "nothing" as the shortcut it was.
    pub fn alt_gr(self) -> bool {
        self.ctrl && self.right_alt && !self.left_alt
    }

    /// Whether a modifier is held that makes this a shortcut rather than
    /// typing.
    ///
    /// AltGr is deliberately not one, which is the fix: a German keyword
    /// containing `@` could never be typed while it was.
    pub fn chord(self) -> bool {
        (self.ctrl || self.left_alt || self.right_alt) && !self.alt_gr()
    }
}

/// What one key event does to the rolling buffer.
///
/// Borrowed rather than owned, so the typing path allocates nothing. This runs
/// for every keystroke on the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect<'a> {
    /// The buffer is untouched.
    Nothing,
    /// Drop the last character.
    Backspace,
    /// Forget everything, because what is in the buffer is no longer the text
    /// immediately behind the caret.
    Clear,
    /// Append these.
    Type(&'a [char]),
}

/// Whether the key went to an input method editor instead of to the field.
///
/// The buffer has to be forgotten when it did, and that is the answer to the
/// second half of this item rather than merely refusing to invent a character
/// for a key that has none. **While an IME is composing, characters are landing
/// in the field that Sill cannot see.** The buffer would still hold whatever
/// was typed before the composition started, no longer immediately behind the
/// caret, and the next keyword to match would delete that many characters from
/// the middle of the user's Japanese. Deleting somebody's text is worse than
/// failing to expand.
fn ime_took_it(vk: u32) -> bool {
    vk == PROCESS_KEY
}

/// Whether this key could type anything at all, before Windows is asked.
///
/// Purely about cost. `MapVirtualKeyW` followed by `ToUnicode` is the most
/// expensive thing on this path by a wide margin and it would otherwise run for
/// every keystroke on the machine including every arrow key and every shortcut.
/// Everything this refuses is a case [`judge_key`] answers without looking at a
/// character, and a test holds those two to that.
pub fn could_type(vk: u32, held: Held) -> bool {
    !ime_took_it(vk) && !resets_context(vk) && !held.chord() && vk != BACKSPACE
}

/// What one key does to the buffer.
///
/// Pure, and that is the point. The three failures this settles are a German
/// layout, a Japanese IME and a character outside the basic plane, none of
/// which can be installed on the machine Sill is developed on. Separating the
/// decision from everything that needs Windows makes each of them a fixture
/// instead of something nobody can run.
///
/// `produced` is what Windows said the key types, which is only consulted when
/// [`could_type`] said it was worth asking.
pub fn judge_key<'a>(vk: u32, held: Held, produced: &'a [char]) -> Effect<'a> {
    // An IME composition first, because Windows has replaced the real key with
    // `VK_PROCESSKEY` and every question below would be asked about the wrong
    // key.
    if ime_took_it(vk) || resets_context(vk) {
        return Effect::Clear;
    }

    // Before backspace, deliberately. Ctrl+Backspace deletes a whole word, so
    // dropping one character from the buffer would leave it holding text the
    // field no longer has.
    if held.chord() {
        return Effect::Clear;
    }

    if vk == BACKSPACE {
        return Effect::Backspace;
    }

    // A control character is not typing. Reachable only under AltGr and from
    // keys with no printable form, because everything that types a control
    // character on its own resets the context above.
    if produced.is_empty() || produced.iter().any(|c| c.is_control()) {
        /*
         * Ctrl and right Alt were down and the layout made nothing of them.
         *
         * So this was a Ctrl+Alt shortcut on a layout with no AltGr, which is
         * every US keyboard, and it has to reset the buffer like any other
         * shortcut. Windows is the only thing that knows which of the two it
         * was, and this is where its answer is read.
         */
        return if held.alt_gr() {
            Effect::Clear
        } else {
            Effect::Nothing
        };
    }

    Effect::Type(produced)
}

/// The characters in the UTF-16 units `ToUnicode` wrote.
///
/// Returns how many were decoded into `out`. A lone surrogate decodes to
/// nothing at all rather than to a replacement character: half of a character
/// is not a character, and putting one in the buffer would make every later
/// comparison against a keyword wrong.
pub fn produced(units: &[u16], out: &mut [char]) -> usize {
    let mut count = 0;

    for decoded in char::decode_utf16(units.iter().copied()) {
        if count == out.len() {
            break;
        }

        let Ok(c) = decoded else {
            return 0;
        };

        out[count] = c;
        count += 1;
    }

    count
}

/// Whether a virtual-key means the caret has gone somewhere else.
///
/// Anything that moves the caret invalidates the buffer, because what was
/// typed before it is no longer immediately behind the caret.
#[cfg(windows)]
pub fn resets_context(vk: u32) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse as vk_codes;

    matches!(
        vk,
        x if x == vk_codes::VK_RETURN.0 as u32
            || x == vk_codes::VK_TAB.0 as u32
            || x == vk_codes::VK_ESCAPE.0 as u32
            || x == vk_codes::VK_LEFT.0 as u32
            || x == vk_codes::VK_RIGHT.0 as u32
            || x == vk_codes::VK_UP.0 as u32
            || x == vk_codes::VK_DOWN.0 as u32
            || x == vk_codes::VK_HOME.0 as u32
            || x == vk_codes::VK_END.0 as u32
            || x == vk_codes::VK_PRIOR.0 as u32
            || x == vk_codes::VK_NEXT.0 as u32
            || x == vk_codes::VK_DELETE.0 as u32
    )
}

#[cfg(not(windows))]
pub fn resets_context(_vk: u32) -> bool {
    false
}

/// Shared state the hook thread and the app both touch.
#[derive(Clone, Default)]
pub struct Expander {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    typed: Mutex<Typed>,
    /// The keywords to watch for, cached here because the hook runs on its
    /// own thread with no route back to the store.
    ///
    /// An `ArcSwap` rather than a `Mutex<Vec<_>>`, and it matters. This is
    /// read on **every keystroke inside a low-level hook callback**, which
    /// Windows expects to return promptly: a `Mutex` there would deep-copy
    /// every snippet's whole text per character, and would block the user's
    /// actual keystroke for as long as a save held the lock. Reading an
    /// `ArcSwap` is lock free and costs a refcount bump.
    snippets: ArcSwap<Vec<Snippet>>,
    enabled: AtomicBool,
    running: AtomicBool,
    /// The thread the hook is installed on, so it can be told to stop.
    ///
    /// A low-level keyboard hook is called for **every keystroke on the
    /// machine**, in every application, forever. Leaving it installed when
    /// expansion is switched off means Sill is still in the path of every key
    /// the user presses in order to do nothing with it, which is the exact
    /// shape of cost rule 23 exists to refuse.
    ///
    /// Zero means not running. A thread id is never zero.
    thread: std::sync::atomic::AtomicU32,
    /// Set while the replacement is being typed, so the synthetic keystrokes
    /// Sill sends are not read back in as more typing.
    replacing: AtomicBool,
    /*
     * The other thing watching the keyboard.
     *
     * The hook stopped being only about snippets when double-tapping a
     * modifier arrived. There is one hook because there should only ever be
     * one: it is called for every keystroke on the machine, in every
     * application, and a second one for a second feature would double that
     * for nothing. Two consumers, one path through it.
     *
     * A `Mutex` rather than an `ArcSwap` because this is written on every
     * keystroke rather than read: it is a modifier and a timestamp, and the
     * lock is held for the length of a comparison.
     */
    taps: Mutex<crate::taps::Taps>,
    /// The key that stands in for four modifiers, if one has been chosen.
    hyper: Mutex<crate::hyper::Hyper>,
    /// What double-tapping does, or nothing when it does nothing.
    ///
    /// Read inside the hook, so it is swapped rather than locked, and held as
    /// one value so the modifier and what it opens cannot be read a moment
    /// apart and disagree.
    tap_binding: ArcSwap<Option<TapBinding>>,
    /// The global hotkeys, taken here ahead of any registration. Read on
    /// every keystroke, so an `ArcSwap` for the same reason `snippets` is.
    hotkeys: ArcSwap<Vec<crate::hotkeys::Hotkey>>,
    /// The key a hotkey is holding down, so its auto-repeat and its release
    /// are swallowed with it rather than fed to the program behind Sill.
    hotkey_held: std::sync::atomic::AtomicU32,
}

/// Which modifier opens what, when it is tapped twice.
#[derive(Debug, Clone)]
pub struct TapBinding {
    pub modifier: crate::taps::Modifier,
    pub window_ms: u64,
}

impl Expander {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_snippets(&self, snippets: Vec<Snippet>) {
        self.inner.snippets.store(Arc::new(snippets));
    }

    /// What double-tapping a modifier opens, or nothing.
    pub fn set_tap_binding(&self, binding: Option<TapBinding>) {
        self.inner.tap_binding.store(Arc::new(binding));

        // What the machine did a moment ago is no guide once the gesture has
        // changed underneath it.
        if let Ok(mut taps) = self.inner.taps.lock() {
            taps.reset();
        }
    }

    fn tap_binding(&self) -> arc_swap::Guard<Arc<Option<TapBinding>>> {
        self.inner.tap_binding.load()
    }

    /// Whether the hook is worth having installed at all.
    ///
    /// **One answer, asked in both places that decide.** Startup and the
    /// settings window each choose whether to arm it, and a hook armed by one
    /// rule and stopped by another is a hook that is on when it should be off
    /// or the reverse. Every drift of this shape in this codebase has cost an
    /// afternoon.
    pub fn wanted(&self) -> bool {
        self.is_enabled()
            || self.tap_binding().is_some()
            || self.hyper_on()
            || !self.inner.hotkeys.load().is_empty()
    }

    /// The global hotkeys the hook answers to. Replaced whole on every
    /// settings write, which is rare; read on every keystroke, which is not.
    pub fn set_hotkeys(&self, hotkeys: Vec<crate::hotkeys::Hotkey>) {
        self.inner.hotkeys.store(Arc::new(hotkeys));
        self.inner
            .hotkey_held
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Which key stands in for four modifiers, or none.
    ///
    /// Setting it forgets whatever was held, because a hyper key changed while
    /// it was down leaves nothing to send the release that would clear it, and
    /// every keystroke afterwards would become a chord nobody asked for.
    pub fn set_hyper(&self, key: Option<u32>) {
        if let Ok(mut hyper) = self.inner.hyper.lock() {
            hyper.set(key);
        }
    }

    pub fn hyper_on(&self) -> bool {
        self.inner
            .hyper
            .lock()
            .map(|hyper| hyper.on())
            .unwrap_or(false)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            self.reset();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        if let Ok(mut typed) = self.inner.typed.lock() {
            typed.clear();
        }
    }

    fn snippets(&self) -> arc_swap::Guard<Arc<Vec<Snippet>>> {
        self.inner.snippets.load()
    }
}

#[cfg(windows)]
pub use windows_impl::{arm, armed, facts, move_caret_back, rearm, replace, settled, stop, watch};

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::OnceLock;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyState, MapVirtualKeyW, ToUnicode, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
        VK_BACK, VK_CAPITAL, VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
        WM_SYSKEYUP,
    };

    /// The one expander the hook callback can reach.
    ///
    /// A hook procedure is a bare function pointer with no user data, so the
    /// state it needs has to be reachable from a static. Set once, before the
    /// hook is installed.
    static EXPANDER: OnceLock<Expander> = OnceLock::new();
    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// Installs the hook, once.
    pub fn watch(app: &AppHandle, expander: &Expander) {
        let _ = APP.set(app.clone());
        arm(expander);
    }

    /// The hook's thread and its lifetime, with no app handle involved.
    ///
    /// Split out so the thing this is really about can be tested: that the
    /// thread starts, and that stopping it ends it. The app handle is only
    /// wanted by the hook procedure, which a test never reaches.
    pub fn arm(expander: &Expander) {
        if expander.inner.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let _ = EXPANDER.set(expander.clone());

        let expander = expander.clone();
        std::thread::Builder::new()
            .name("snippet-hook".to_string())
            .spawn(move || {
                // SAFETY: the hook is installed and pumped on this thread,
                // which is what `SetWindowsHookExW` requires, and released
                // when the pump ends.
                unsafe {
                    let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
                    else {
                        crate::say!("could not install the snippet hook");
                        expander.inner.running.store(false, Ordering::SeqCst);
                        return;
                    };

                    // Published only once the hook is really installed, so
                    // stopping cannot race ahead of starting and post a quit
                    // to a thread that has not begun pumping.
                    expander
                        .inner
                        .thread
                        .store(GetCurrentThreadId(), Ordering::SeqCst);

                    /*
                     * Cleared however this thread ends, including by unwinding.
                     *
                     * Written as a guard rather than as two lines after the
                     * loop: a panic anywhere in the pump would otherwise skip
                     * them, leaving `running` true and `thread` non-zero
                     * forever, and `arm` returns early when `running` is true.
                     * The hook would be gone and could never be put back
                     * without restarting Sill.
                     */
                    struct ClearOnExit(Expander);

                    impl Drop for ClearOnExit {
                        fn drop(&mut self) {
                            self.0.inner.thread.store(0, Ordering::SeqCst);
                            self.0.inner.running.store(false, Ordering::SeqCst);
                        }
                    }

                    let _clear = ClearOnExit(expander.clone());

                    let mut message = MSG::default();
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {}

                    let _ = UnhookWindowsHookEx(hook);
                }
            })
            .ok();
    }

    /// Every keystroke this hook has been called for.
    ///
    /// The one thing that distinguishes an installed hook from a working one.
    /// **Windows silently removes a low-level hook whose callback takes too
    /// long** and tells nobody: the thread stays parked in `GetMessageW`, the
    /// handle stays valid, `armed` keeps answering true, and every keyword
    /// quietly stops firing. Without a count there is nothing to look at and
    /// no way to tell that from "the keyword is wrong".
    ///
    /// The dictation hook has had this since the day its trigger died for two
    /// silent reasons at once. Same idea, same reason.
    static KEYS_SEEN: AtomicU64 = AtomicU64::new(0);

    /// Whether the hook is installed right now.
    pub fn armed(expander: &Expander) -> bool {
        expander.inner.thread.load(Ordering::SeqCst) != 0
    }

    /// What can be said about the hook without guessing.
    ///
    /// `installed` is what Sill believes; `keys_seen` is what actually
    /// happened. Installed with a count stuck at zero while somebody types is
    /// the signature of a hook Windows took away.
    pub fn facts(expander: &Expander) -> (bool, u64) {
        (armed(expander), KEYS_SEEN.load(Ordering::Relaxed))
    }

    /// Takes the hook out and lets its thread finish.
    ///
    /// Not merely stopping it matching. The hook is on the machine's keyboard
    /// path whether or not it does anything with what it sees, so switching
    /// expansion off has to remove it rather than teach it to ignore
    /// everything.
    ///
    /// `WM_QUIT` rather than a flag: the thread is blocked in `GetMessageW`,
    /// so a flag would only be read the next time a message arrived, and no
    /// message ever arrives on that queue.
    pub fn stop(expander: &Expander) {
        let thread = expander.inner.thread.load(Ordering::SeqCst);
        if thread == 0 {
            return;
        }

        // SAFETY: posts to a thread id read from the hook thread itself. A
        // thread that has already finished makes this fail rather than fault,
        // which is why the result is only reported.
        unsafe {
            if PostThreadMessageW(thread, WM_QUIT, WPARAM(0), LPARAM(0)).is_err() {
                crate::say!("the snippet hook did not stop");
            }
        }
    }

    /// How long a re-install waits for the old hook's thread to let go.
    ///
    /// It is blocked in `GetMessageW` with a `WM_QUIT` already on its queue, so
    /// this is a scheduling delay rather than any real work. The bound exists
    /// because giving up and leaving the hook off is better than a thread that
    /// waits forever on the one path whose whole job is recovery.
    const LETS_GO_WITHIN: std::time::Duration = std::time::Duration::from_millis(500);

    /// Takes the hook out and puts it straight back, and says whether it
    /// worked.
    ///
    /// The only way to recover a hook Windows removed for being slow. It does
    /// that silently: the thread stays parked in `GetMessageW`, the handle
    /// stays valid and `armed` keeps answering true, so `arm` on its own sees a
    /// hook that is already running and returns without doing anything. The
    /// teardown is the part that matters.
    ///
    /// Safe to call repeatedly. `stop` on a hook that is not running returns at
    /// once, `arm` refuses a second install while one is starting, and the wait
    /// below is what stops the two from crossing: `arm` reads the same `running`
    /// flag the old thread clears on its way out, so re-installing before it has
    /// gone would look like an install that was already under way and quietly do
    /// nothing.
    ///
    /// **Never on a thread anybody is waiting for.** It sleeps.
    pub fn rearm(expander: &Expander) -> bool {
        stop(expander);

        let deadline = std::time::Instant::now() + LETS_GO_WITHIN;
        while expander.inner.running.load(Ordering::SeqCst) {
            if std::time::Instant::now() >= deadline {
                crate::say!("the old snippet hook thread would not let go");
                return false;
            }

            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        arm(expander);
        settled(expander)
    }

    /// Whether the hook is there, waiting briefly for an answer.
    ///
    /// `arm` returns before the hook exists. It spawns the thread that installs
    /// it, and the thread id is published from inside that thread once
    /// `SetWindowsHookExW` has really returned a handle, so `armed` immediately
    /// after `arm` says no about a hook that is about to be fine.
    ///
    /// This is the difference between reporting that Sill cannot watch the
    /// keyboard and reporting that it could not do so within a few milliseconds
    /// of being asked. Only the first is worth saying.
    ///
    /// **Never on a thread anybody is waiting for.** It sleeps.
    pub fn settled(expander: &Expander) -> bool {
        let deadline = std::time::Instant::now() + LETS_GO_WITHIN;

        while !armed(expander) {
            if std::time::Instant::now() >= deadline {
                return false;
            }

            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        true
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: Windows guarantees the pointer for `code >= 0`, and the
        // chain is always continued.
        let next = || unsafe { CallNextHookEx(None, code, wparam, lparam) };

        if code < 0 {
            return next();
        }

        // Counted before anything can return early, so the number answers
        // "is this hook being called at all" rather than "did it do anything".
        KEYS_SEEN.fetch_add(1, Ordering::Relaxed);

        let Some(expander) = EXPANDER.get() else {
            return next();
        };
        // Sill typing a replacement is not the user typing, whichever
        // consumer is looking.
        if expander.inner.replacing.load(Ordering::SeqCst) {
            return next();
        }

        let message = wparam.0 as u32;

        let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let up = message == WM_KEYUP || message == WM_SYSKEYUP;

        // Releases matter now. Without them a held modifier cannot be told
        // from one pressed twice, because Windows repeats a held key.
        if !down && !up {
            return next();
        }

        // SAFETY: for `code >= 0` lparam is a KBDLLHOOKSTRUCT.
        let event = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };

        /*
         * Sill's own synthetic keys must never feed the buffer, or the
         * replacement would be read back as more typing.
         *
         * **Ours specifically, not everything injected.** This tested
         * `LLKHF_INJECTED`, which means "not typed on a physical keyboard"
         * and covers a great deal more than us: key remappers, macro keys,
         * on-screen keyboards, Remote Desktop and every other remote session,
         * and accessibility tools. Snippet expansion silently did nothing for
         * anybody using any of them, with no error to explain why.
         *
         * `synthetic.rs` was written to say exactly this and the dictation
         * hook already follows it. This one stamped its own keys correctly
         * and then asked the wrong question about them.
         */
        if event.dwExtraInfo == crate::synthetic::SILL_SYNTHETIC {
            return next();
        }

        let vk = event.vkCode;

        /*
         * The hyper key, before anything else looks at the keystroke.
         *
         * First because it decides whether this key exists at all for
         * everything downstream. A key turned into a chord must not also feed
         * the snippet buffer or the double-tap watcher: it was never typed,
         * and counting it would let Hyper+S complete a keyword or a gesture
         * nobody made.
         */
        match expander
            .inner
            .hyper
            .lock()
            .map(|mut hyper| hyper.saw(vk, !up))
            .unwrap_or(crate::hyper::Verdict::Pass)
        {
            crate::hyper::Verdict::Pass => {}
            crate::hyper::Verdict::Swallow => return LRESULT(1),
            crate::hyper::Verdict::Chord(key) => {
                crate::input::hyper(key as u16);
                return LRESULT(1);
            }
        }

        /*
         * The global hotkeys, ahead of every registration on the machine.
         *
         * Matched on the key first and the modifiers second, so the cost on
         * an ordinary keystroke is a scan of a few entries and nothing else.
         * A match is swallowed on the way down, on every auto-repeat while it
         * is held, and on the way up, so the program behind Sill sees none of
         * it: a Menu key that summons the launcher must not also open a
         * context menu. What the chord does runs off this thread.
         */
        let holding = expander.inner.hotkey_held.load(Ordering::Relaxed);
        if up && holding == vk {
            expander.inner.hotkey_held.store(0, Ordering::Relaxed);
            return LRESULT(1);
        }
        if down {
            if holding == vk {
                return LRESULT(1);
            }
            let hotkeys = expander.inner.hotkeys.load();
            if hotkeys.iter().any(|one| one.chord.vk == vk) {
                let held = crate::hotkeys::held_now();
                if let Some(hit) = crate::hotkeys::hit(&hotkeys, vk, held) {
                    expander.inner.hotkey_held.store(vk, Ordering::Relaxed);
                    // Swallowing Win+X leaves Windows a lone Win tap, which
                    // opens the Start menu on release. A key that types
                    // nothing, sent while Win is still down, makes it a chord.
                    if hit.chord.win {
                        crate::input::blank();
                    }
                    if let Some(app) = APP.get() {
                        crate::hotkeys::dispatch(app, hit.target.clone());
                    }
                    return LRESULT(1);
                }
            }
        }

        if up {
            if let Ok(mut taps) = expander.inner.taps.lock() {
                taps.release(vk);
            }
            return next();
        }

        handle_tap(expander, vk);

        // The snippet buffer is the consumer that can be switched off on its
        // own. The hook may be installed purely for the gesture above.
        if expander.is_enabled() {
            handle_key(expander, vk);
        }

        next()
    }

    /// Feeds one key to the double-tap watcher, and acts if it completed one.
    fn handle_tap(expander: &Expander, vk: u32) {
        let binding = expander.tap_binding();
        let Some(binding) = binding.as_ref().as_ref() else {
            return;
        };

        let fired = {
            let Ok(mut taps) = expander.inner.taps.lock() else {
                return;
            };

            // A monotonic millisecond count. `Instant` cannot be handed to a
            // pure function and compared against a number in a test, and this
            // is read once per keystroke.
            taps.press(vk, now_ms(), binding.window_ms)
        };

        if fired != Some(binding.modifier) {
            return;
        }

        // Handed to the app rather than done here: showing a window is not
        // something a hook callback Windows expects to return promptly should
        // sit and wait on.
        if let Some(app) = APP.get() {
            let app = app.clone();

            std::thread::spawn(move || {
                crate::summon::show_with(&app, None);
            });
        }
    }

    /// Milliseconds since the process started, monotonically.
    fn now_ms() -> u64 {
        use std::sync::OnceLock;
        use std::time::Instant;

        static START: OnceLock<Instant> = OnceLock::new();
        START.get_or_init(Instant::now).elapsed().as_millis() as u64
    }

    fn handle_key(expander: &Expander, vk: u32) {
        let held = held_now();

        /*
         * Windows is asked what the key types, and the answer is part of the
         * decision rather than something read afterwards.
         *
         * On a layout with AltGr there is no other way. AltGr arrives as Ctrl
         * plus right Alt, which is indistinguishable from a Ctrl+Alt shortcut
         * until the layout has been consulted: on a German keyboard AltGr+Q is
         * the character `@` and on a US keyboard the identical key state is a
         * shortcut that types nothing.
         *
         * On the stack, and only when the cheap questions have not already
         * settled it, because `ToUnicode` runs for every keystroke on the
         * machine.
         */
        let mut chars = ['\0'; PRODUCED_MAX];
        let count = if could_type(vk, held) {
            character_for(vk, held, &mut chars)
        } else {
            0
        };

        let typed_now = match judge_key(vk, held, &chars[..count]) {
            Effect::Nothing => return,
            Effect::Clear => {
                expander.reset();
                return;
            }
            Effect::Backspace => {
                if let Ok(mut typed) = expander.inner.typed.lock() {
                    typed.backspace();
                }
                return;
            }
            Effect::Type(chars) => chars,
        };

        let matched = {
            let Ok(mut typed) = expander.inner.typed.lock() else {
                return;
            };
            typed.push_all(typed_now);

            crate::snippets::store::match_keyword(
                &expander.snippets(),
                typed.as_str(),
                // Called only if the snippet that matched is limited to
                // certain programs, which is rare. See `match_keyword`.
                || crate::dictation::context::foreground_app_full().map(|app| app.path),
            )
            .map(|snippet| (snippet.id.clone(), snippet.keyword.trim().chars().count()))
        };

        let Some((id, keyword_len)) = matched else {
            return;
        };

        if let Ok(mut typed) = expander.inner.typed.lock() {
            typed.consume(keyword_len);
        }

        // Handed to the app rather than expanded here: the replacement needs
        // the clipboard, the date and a paste, none of which belong on a hook
        // callback that Windows expects to return promptly.
        /*
         * Off the hook thread, like the double-tap above it.
         *
         * `emit` serialises the payload and dispatches it to every webview,
         * and doing that inside the callback puts all of it on the clock
         * Windows is measuring when it decides whether this hook is too slow
         * to keep. It is only reached on a match, so this is rare rather than
         * hot, and that is exactly why it was easy to leave here.
         */
        if let Some(app) = APP.get() {
            let app = app.clone();

            std::thread::spawn(move || {
                let _ = app.emit("snippets:expand", (id, keyword_len));
            });
        }
    }

    /// Which modifiers are down at this instant.
    ///
    /// `GetAsyncKeyState`, like the dictation hook, and not `GetKeyState` as
    /// this used to. `GetKeyState` answers for the calling thread's input
    /// queue, and the hook thread has one only to park in: it installs the
    /// hook and blocks in `GetMessageW` without ever being sent a keyboard
    /// message, so what it believes about the modifiers is whatever it
    /// believed at the start. `GetAsyncKeyState` asks the system.
    ///
    /// The synthetic left Ctrl that AltGr sends is a key event of its own and
    /// goes through this same hook before the character does, so by the time a
    /// character key arrives both halves of AltGr are already down as far as
    /// the system is concerned.
    fn held_now() -> Held {
        use windows::Win32::UI::Input::KeyboardAndMouse::{VK_LMENU, VK_RMENU};

        // SAFETY: takes a virtual key and returns a plain value.
        let down =
            |vk: VIRTUAL_KEY| unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 };

        Held {
            ctrl: down(VK_CONTROL),
            left_alt: down(VK_LMENU),
            right_alt: down(VK_RMENU),
        }
    }

    /// What a key types, given the modifiers held, written into `out`.
    ///
    /// `ToUnicode` rather than a table, so a non-US layout types what its
    /// user expects: on a German keyboard the key marked Z produces `z`, and
    /// a hand-written map would say `y`.
    ///
    /// Ctrl and Alt are put into the state when they are AltGr, because that
    /// pair is exactly how `ToUnicode` is told to translate a third level: with
    /// them absent it answers for the unshifted key and `@` on a German layout
    /// would come back as `q`.
    fn character_for(vk: u32, held: Held, out: &mut [char; PRODUCED_MAX]) -> usize {
        let mut state = [0u8; 256];

        // SAFETY: every call takes an owned buffer of the documented size.
        unsafe {
            if (GetAsyncKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 {
                state[VK_SHIFT.0 as usize] = 0x80;
            }
            // The toggle rather than whether it is held down, and
            // `GetAsyncKeyState` has no answer for that, so this one stays.
            if (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 {
                state[VK_CAPITAL.0 as usize] = 0x01;
            }
            if held.alt_gr() {
                state[VK_CONTROL.0 as usize] = 0x80;
                state[VK_MENU.0 as usize] = 0x80;
            }

            let scan = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
            let mut buffer = [0u16; PRODUCED_MAX];
            // Flag 4 asks not to disturb the keyboard state, which matters
            // for dead keys: without it, typing an accent would consume it
            // here and the user's next character would come out wrong.
            let written = ToUnicode(vk, scan, Some(&state), &mut buffer, 4);

            // Zero is no translation and a negative one is a dead key, which
            // has put nothing in the field yet.
            if written <= 0 {
                return 0;
            }

            produced(&buffer[..(written as usize).min(PRODUCED_MAX)], out)
        }
    }

    /// Deletes the keyword and types the replacement.
    ///
    /// Called from the app, not the hook: it sends input, which a hook
    /// callback must not sit and wait on.
    pub fn replace(expander: &Expander, backspaces: usize, text: &str, html: &str) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE,
        };

        expander.inner.replacing.store(true, Ordering::SeqCst);

        let mut events: Vec<INPUT> = Vec::with_capacity(backspaces * 2 + text.len() * 2);

        for _ in 0..backspaces {
            events.push(key_event(VK_BACK.0, false));
            events.push(key_event(VK_BACK.0, true));
        }

        let paste = crate::snippets::store::wants_pasting(text, html);

        if !paste {
            for unit in text.encode_utf16() {
                events.push(unicode_event(unit, false));
                events.push(unicode_event(unit, true));
            }
        }

        if !events.is_empty() {
            // SAFETY: a live array of correctly sized INPUT records, with the
            // size taken from the type.
            unsafe {
                SendInput(&events, std::mem::size_of::<INPUT>() as i32);
            }
        }

        if paste {
            paste_text(text, html);
        }

        expander.inner.replacing.store(false, Ordering::SeqCst);

        fn key_event(vk: u16, up: bool) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(vk),
                        wScan: 0,
                        dwFlags: if up {
                            KEYEVENTF_KEYUP
                        } else {
                            Default::default()
                        },
                        time: 0,
                        dwExtraInfo: crate::synthetic::SILL_SYNTHETIC,
                    },
                },
            }
        }

        fn unicode_event(unit: u16, up: bool) -> INPUT {
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: if up {
                            KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                        } else {
                            KEYEVENTF_UNICODE
                        },
                        time: 0,
                        dwExtraInfo: crate::synthetic::SILL_SYNTHETIC,
                    },
                },
            }
        }
    }

    /// Puts `text` on the clipboard, pastes it, and puts back what was there.
    ///
    /// The restore is the part that matters: a snippet that silently emptied
    /// the clipboard would be a poor trade, and this path only exists for
    /// text too long to type.
    /// Puts a snippet on the clipboard, pastes it, and gives the clipboard
    /// back.
    ///
    /// `html` empty means plain text. When it is not, both formats are written
    /// in one go and the target takes whichever it understands, so a plain
    /// field still receives sensible text rather than markup as characters.
    ///
    /// Borrowed through [`crate::selection::Held`] rather than by hand, which
    /// is a fix as well as a tidying: written by hand, the write and the
    /// restore were **two clipboard changes the history recorded**, so pasting
    /// a long snippet left the snippet and then the user's own older entry
    /// sitting at the top of the history as though they had just copied both.
    /// `Held` suspends the history for the whole borrow instead of trying to
    /// count the changes.
    ///
    /// What goes back is text. An image on the clipboard does not survive,
    /// which is true of every borrow in Sill and is written down here because
    /// this is the one somebody triggers by typing.
    fn paste_text(text: &str, html: &str) {
        let Some(app) = APP.get() else {
            crate::say!("no app handle, so a snippet cannot be pasted");
            return;
        };

        let held = crate::selection::Held::take(app);

        let wrote = arboard::Clipboard::new().ok().is_some_and(|mut board| {
            if html.is_empty() {
                board.set_text(text.to_string()).is_ok()
            } else {
                board
                    .set()
                    .html(html.to_string(), Some(text.to_string()))
                    .is_ok()
            }
        });

        if !wrote {
            crate::say!("could not put a snippet on the clipboard");
            held.give_back();
            return;
        }

        // The same settle every paste in Sill needs: writing and immediately
        // pasting races the target application's read of the clipboard.
        std::thread::sleep(std::time::Duration::from_millis(60));
        crate::dictation::paste::chord();

        // After the paste has been read, not before. Giving it back
        // immediately would hand the target the old contents instead of the
        // snippet.
        std::thread::sleep(std::time::Duration::from_millis(120));
        held.give_back();
    }

    /// Moves the caret back `count` characters, for a `{cursor}` placeholder.
    pub fn move_caret_back(count: usize) {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_LEFT,
        };

        if count == 0 {
            return;
        }

        let mut events = Vec::with_capacity(count * 2);
        for _ in 0..count {
            for up in [false, true] {
                events.push(INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VK_LEFT,
                            wScan: 0,
                            dwFlags: if up {
                                KEYEVENTF_KEYUP
                            } else {
                                Default::default()
                            },
                            time: 0,
                            dwExtraInfo: crate::synthetic::SILL_SYNTHETIC,
                        },
                    },
                });
            }
        }

        // SAFETY: as above.
        unsafe {
            SendInput(&events, std::mem::size_of::<INPUT>() as i32);
        }
    }
}

#[cfg(not(windows))]
pub fn watch(_app: &tauri::AppHandle, _expander: &Expander) {}

#[cfg(not(windows))]
pub fn stop(_expander: &Expander) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_keeps_only_the_recent_tail() {
        let mut typed = Typed::new();
        for c in "abcdefghij".repeat(20).chars() {
            typed.push(c);
        }
        assert_eq!(typed.as_str().chars().count(), BUFFER_CHARS);
    }

    #[test]
    fn trimming_the_buffer_never_splits_a_character() {
        // A byte-wise drain would leave half of a multi-byte character and
        // the next push would produce a broken string.
        let mut typed = Typed::new();
        for c in "é".repeat(BUFFER_CHARS + 20).chars() {
            typed.push(c);
        }
        assert_eq!(typed.as_str().chars().count(), BUFFER_CHARS);
        assert!(typed.as_str().chars().all(|c| c == 'é'));
    }

    #[test]
    fn backspace_removes_the_last_character() {
        let mut typed = Typed::new();
        for c in "abc".chars() {
            typed.push(c);
        }
        typed.backspace();
        assert_eq!(typed.as_str(), "ab");
    }

    #[test]
    fn backspace_on_an_empty_buffer_is_harmless() {
        let mut typed = Typed::new();
        typed.backspace();
        typed.backspace();
        assert_eq!(typed.as_str(), "");
    }

    #[test]
    fn consuming_drops_exactly_the_keyword() {
        // After a match the keyword is gone from the field, so it has to go
        // from the buffer too or the next character re-matches it.
        let mut typed = Typed::new();
        for c in "hello ;sig".chars() {
            typed.push(c);
        }
        typed.consume(4);
        assert_eq!(typed.as_str(), "hello ");
    }

    #[test]
    fn clearing_forgets_everything() {
        let mut typed = Typed::new();
        for c in "secret".chars() {
            typed.push(c);
        }
        typed.clear();
        assert_eq!(typed.as_str(), "");
    }

    #[test]
    fn a_disabled_expander_forgets_what_it_had_seen() {
        // Turning it off must not leave a buffer of typing in memory.
        let expander = Expander::new();
        expander.set_enabled(true);
        expander.inner.typed.lock().unwrap().push('x');

        expander.set_enabled(false);
        assert_eq!(expander.inner.typed.lock().unwrap().as_str(), "");
        assert!(!expander.is_enabled());
    }

    /* ---------------------------------------------------------------------
     * The three layouts nobody here has.
     *
     * A German keyboard, a Japanese IME and a character outside the basic
     * plane cannot be installed on the machine this is written on, so the
     * decision was made pure and these are the fixtures. What a person with
     * those layouts still has to confirm is written at the end of the item.
     * -------------------------------------------------------------------- */

    /// The key marked Q, which on a German layout is `@` under AltGr.
    const Q: u32 = 0x51;
    /// The key marked A.
    const A: u32 = 0x41;

    /// AltGr, exactly as a keyboard driver delivers it.
    fn alt_gr() -> Held {
        Held {
            ctrl: true,
            right_alt: true,
            left_alt: false,
        }
    }

    /// A genuine Ctrl+Alt chord, which is Ctrl with the Alt beside the space
    /// bar.
    fn ctrl_alt() -> Held {
        Held {
            ctrl: true,
            left_alt: true,
            right_alt: false,
        }
    }

    fn nothing_held() -> Held {
        Held::default()
    }

    /// A German layout types `@` with AltGr+Q, and it has to reach the buffer.
    ///
    /// This is the first half of the item. `@` is in more keywords than any
    /// other punctuation, and on every layout with an AltGr key it could not be
    /// typed at all, because Ctrl was down and Ctrl down meant shortcut.
    #[test]
    fn an_altgr_character_is_typing() {
        assert_eq!(judge_key(Q, alt_gr(), &['@']), Effect::Type(&['@']));
    }

    /// And a real Ctrl+Alt chord still throws the buffer away.
    ///
    /// The trap in the whole fix. AltGr is Ctrl plus **right** Alt and a real
    /// chord is Ctrl plus **left** Alt, and letting go of the difference in
    /// either direction is a bug: keep resetting on right Alt and German
    /// keywords stay impossible, stop resetting on left Alt and every Ctrl+Alt
    /// shortcut in every application leaks a character into the snippet buffer.
    #[test]
    fn a_real_ctrl_alt_chord_still_forgets_everything() {
        assert!(ctrl_alt().chord(), "Ctrl with the left Alt is a shortcut");
        assert!(!ctrl_alt().alt_gr(), "the left Alt is not AltGr");

        assert_eq!(judge_key(A, ctrl_alt(), &['a']), Effect::Clear);
    }

    /// Ctrl and right Alt on a layout with no AltGr is a shortcut too.
    ///
    /// A US keyboard has no third level, so Ctrl plus right Alt is a plain
    /// Ctrl+Alt shortcut that happens to arrive in exactly the shape AltGr
    /// arrives in. Nothing in the key state separates them. Windows does: the
    /// layout translates it to nothing, and nothing is the answer that says it
    /// was a shortcut after all.
    #[test]
    fn ctrl_and_right_alt_that_type_nothing_are_a_shortcut() {
        assert_eq!(judge_key(A, alt_gr(), &[]), Effect::Clear);
    }

    /// Right Alt on its own is Alt, not AltGr.
    ///
    /// AltGr always brings the synthetic Ctrl with it, so right Alt without
    /// Ctrl is somebody holding the Alt on the right of their space bar.
    #[test]
    fn right_alt_without_ctrl_is_an_alt_shortcut() {
        let right_alt_only = Held {
            ctrl: false,
            left_alt: false,
            right_alt: true,
        };

        assert!(right_alt_only.chord());
        assert_eq!(judge_key(A, right_alt_only, &['a']), Effect::Clear);
    }

    /// A key an IME took changes the buffer to nothing at all.
    ///
    /// Both halves matter. No character is invented for `VK_PROCESSKEY`, which
    /// types nothing, and **what was in the buffer is dropped**, because a
    /// composition is text landing in the field that Sill cannot see. Keeping
    /// it would leave the buffer holding characters that are no longer
    /// immediately behind the caret, and the next keyword to match would delete
    /// its own length out of the middle of somebody's Japanese.
    #[test]
    fn a_key_an_ime_took_leaves_nothing_behind() {
        assert_eq!(judge_key(PROCESS_KEY, nothing_held(), &[]), Effect::Clear);

        let mut typed = Typed::new();
        typed.push_all(&['s', 'i', 'g']);

        if let Effect::Clear = judge_key(PROCESS_KEY, nothing_held(), &[]) {
            typed.clear();
        }

        assert_eq!(
            typed.as_str(),
            "",
            "a composition left earlier typing in the buffer, so it no longer sits behind \
             the caret and a match would delete the wrong characters"
        );
    }

    /// A committed composition arrives as characters and lands whole.
    ///
    /// One key event producing several characters is the same defect as the
    /// surrogate pair below: the old code took the first UTF-16 unit and only
    /// when there was exactly one, so anything longer became nothing.
    #[test]
    fn committed_characters_all_reach_the_buffer() {
        let commit = ['に', 'ほ', 'ん'];
        assert_eq!(judge_key(A, nothing_held(), &commit), Effect::Type(&commit));

        let mut typed = Typed::new();
        typed.push_all(&commit);
        assert_eq!(typed.as_str(), "にほん");
    }

    /// A character outside the basic plane survives whole.
    ///
    /// Two UTF-16 units are one character. Accepting a single unit meant every
    /// emoji and a good deal of CJK either vanished or, worse, arrived as half
    /// of itself.
    #[test]
    fn a_surrogate_pair_is_one_character() {
        let mut out = ['\0'; PRODUCED_MAX];

        // U+1F600, as `ToUnicode` writes it.
        let count = produced(&[0xD83D, 0xDE00], &mut out);
        assert_eq!(count, 1, "a surrogate pair decoded to {count} characters");
        assert_eq!(out[0], '😀');

        assert_eq!(
            judge_key(A, nothing_held(), &out[..count]),
            Effect::Type(&['😀'])
        );
    }

    /// Half a character is not a character.
    ///
    /// A lone surrogate decodes to nothing rather than to a replacement
    /// character, because a `U+FFFD` in the buffer is a character the user
    /// never typed and every keyword comparison after it would be against text
    /// that is not in the field.
    #[test]
    fn a_lone_surrogate_reaches_nothing() {
        let mut out = ['\0'; PRODUCED_MAX];
        assert_eq!(produced(&[0xD83D], &mut out), 0);
    }

    /// Ctrl+Backspace deleted a word, so one character is the wrong answer.
    #[test]
    fn ctrl_backspace_forgets_everything_rather_than_one_character() {
        let ctrl = Held {
            ctrl: true,
            left_alt: false,
            right_alt: false,
        };

        assert_eq!(judge_key(BACKSPACE, ctrl, &[]), Effect::Clear);
        assert_eq!(
            judge_key(BACKSPACE, nothing_held(), &[]),
            Effect::Backspace,
            "a plain backspace still removes exactly one character"
        );
    }

    /// The cost gate and the decision cannot drift apart.
    ///
    /// `could_type` exists only to keep `MapVirtualKeyW` and `ToUnicode` off
    /// the path for keys that cannot type anything, which is most keystrokes on
    /// the machine. That is safe exactly while every case it refuses is one
    /// `judge_key` answers without reading a character, and nothing but this
    /// holds the two lists together.
    #[test]
    fn the_cost_gate_only_skips_keys_whose_answer_ignores_the_character() {
        let states = [nothing_held(), ctrl_alt(), alt_gr()];
        let keys = [A, Q, BACKSPACE, PROCESS_KEY, 0x0D, 0x1B, 0x25];

        for vk in keys {
            for held in states {
                if could_type(vk, held) {
                    continue;
                }

                assert_eq!(
                    judge_key(vk, held, &['x']),
                    judge_key(vk, held, &[]),
                    "key {vk:#x} with {held:?} is refused a translation but its answer \
                     depends on one"
                );
            }
        }
    }

    /// Every character of a multi-character key event is remembered.
    #[test]
    fn pushing_several_characters_keeps_all_of_them() {
        let mut typed = Typed::new();
        typed.push_all(&['a', 'b']);
        typed.push_all(&['😀']);
        assert_eq!(typed.as_str(), "ab😀");
    }

    #[cfg(windows)]
    #[test]
    fn caret_movement_resets_the_context_and_typing_does_not() {
        use windows::Win32::UI::Input::KeyboardAndMouse as vk;

        // Anything that moves the caret means what was typed before it is no
        // longer immediately behind it.
        for key in [
            vk::VK_RETURN,
            vk::VK_TAB,
            vk::VK_LEFT,
            vk::VK_HOME,
            vk::VK_ESCAPE,
        ] {
            assert!(resets_context(key.0 as u32), "{key:?} should reset");
        }
        for key in [vk::VK_A, vk::VK_SPACE, vk::VK_OEM_1] {
            assert!(!resets_context(key.0 as u32), "{key:?} should not reset");
        }
    }
}
