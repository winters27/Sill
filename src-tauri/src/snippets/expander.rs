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

    /// Drops the last `count` characters, after they have been replaced.
    pub fn consume(&mut self, count: usize) {
        for _ in 0..count {
            self.buffer.pop();
        }
    }
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
        self.is_enabled() || self.tap_binding().is_some() || self.hyper_on()
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
pub use windows_impl::{arm, armed, facts, move_caret_back, replace, stop, watch};

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::OnceLock;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, MapVirtualKeyW, ToUnicode, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_BACK, VK_CAPITAL,
        VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
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
        if vk == VK_BACK.0 as u32 {
            if let Ok(mut typed) = expander.inner.typed.lock() {
                typed.backspace();
            }
            return;
        }

        if resets_context(vk) {
            expander.reset();
            return;
        }

        // A modifier held with a letter is a shortcut, not typing.
        // SAFETY: GetKeyState takes a virtual key and returns a plain value.
        let modified = unsafe {
            (GetKeyState(VK_CONTROL.0 as i32) as u16 & 0x8000) != 0
                || (GetKeyState(VK_MENU.0 as i32) as u16 & 0x8000) != 0
        };
        if modified {
            expander.reset();
            return;
        }

        let Some(c) = character_for(vk) else {
            return;
        };

        let matched = {
            let Ok(mut typed) = expander.inner.typed.lock() else {
                return;
            };
            typed.push(c);

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

    /// The character a key would type, given the modifiers held right now.
    ///
    /// `ToUnicode` rather than a table, so a non-US layout types what its
    /// user expects: on a German keyboard the key marked Z produces `z`, and
    /// a hand-written map would say `y`.
    fn character_for(vk: u32) -> Option<char> {
        let mut state = [0u8; 256];

        // SAFETY: every call takes an owned buffer of the documented size.
        unsafe {
            if (GetKeyState(VK_SHIFT.0 as i32) as u16 & 0x8000) != 0 {
                state[VK_SHIFT.0 as usize] = 0x80;
            }
            if (GetKeyState(VK_CAPITAL.0 as i32) as u16 & 0x0001) != 0 {
                state[VK_CAPITAL.0 as usize] = 0x01;
            }

            let scan = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
            let mut buffer = [0u16; 8];
            // Flag 4 asks not to disturb the keyboard state, which matters
            // for dead keys: without it, typing an accent would consume it
            // here and the user's next character would come out wrong.
            let written = ToUnicode(vk, scan, Some(&state), &mut buffer, 4);

            (written == 1).then(|| char::from_u32(buffer[0] as u32))?
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
                board.set().html(html.to_string(), Some(text.to_string())).is_ok()
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
