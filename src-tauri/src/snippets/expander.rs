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
    /// Set while the replacement is being typed, so the synthetic keystrokes
    /// Sill sends are not read back in as more typing.
    replacing: AtomicBool,
}

impl Expander {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_snippets(&self, snippets: Vec<Snippet>) {
        self.inner.snippets.store(Arc::new(snippets));
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
pub use windows_impl::{move_caret_back, replace, watch};

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::sync::OnceLock;
    use tauri::{AppHandle, Emitter};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyState, MapVirtualKeyW, ToUnicode, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_BACK, VK_CAPITAL,
        VK_CONTROL, VK_MENU, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
        LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
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
        if expander.inner.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let _ = EXPANDER.set(expander.clone());
        let _ = APP.set(app.clone());

        std::thread::Builder::new()
            .name("snippet-hook".to_string())
            .spawn(|| {
                // SAFETY: the hook is installed and pumped on this thread,
                // which is what `SetWindowsHookExW` requires, and released
                // when the pump ends.
                unsafe {
                    let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
                    else {
                        crate::say!("could not install the snippet hook");
                        return;
                    };

                    let mut message = MSG::default();
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {}

                    let _ = UnhookWindowsHookEx(hook);
                }
            })
            .ok();
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // SAFETY: Windows guarantees the pointer for `code >= 0`, and the
        // chain is always continued.
        let next = || unsafe { CallNextHookEx(None, code, wparam, lparam) };

        if code < 0 {
            return next();
        }

        let Some(expander) = EXPANDER.get() else {
            return next();
        };
        if !expander.is_enabled() || expander.inner.replacing.load(Ordering::SeqCst) {
            return next();
        }

        let message = wparam.0 as u32;
        if message != WM_KEYDOWN && message != WM_SYSKEYDOWN {
            return next();
        }

        // SAFETY: for `code >= 0` lparam is a KBDLLHOOKSTRUCT.
        let event = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };

        // Sill's own synthetic keys must never feed the buffer, or the
        // replacement would be read back as more typing.
        if event.flags & LLKHF_INJECTED != KBDLLHOOKSTRUCT_FLAGS_NONE {
            return next();
        }

        let vk = event.vkCode;
        handle_key(expander, vk);
        next()
    }

    const KBDLLHOOKSTRUCT_FLAGS_NONE: windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT_FLAGS =
        windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT_FLAGS(0);

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

            crate::snippets::store::match_keyword(&expander.snippets(), typed.as_str())
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
        if let Some(app) = APP.get() {
            let _ = app.emit("snippets:expand", (id, keyword_len));
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

    /// Past this many characters, typing is abandoned for a paste.
    ///
    /// `SendInput` costs two records per character, so a 600-character
    /// snippet is 1,200 events. That is visibly slow to watch appear, and
    /// several classes of application (Electron, terminals, remote desktop)
    /// drop synthetic input arriving that fast. Every mature expander has the
    /// same threshold for the same reason.
    ///
    /// Below it, typing is strictly better: it leaves the clipboard alone.
    pub const TYPE_LIMIT: usize = 200;

    /// Deletes the keyword and types the replacement.
    ///
    /// Called from the app, not the hook: it sends input, which a hook
    /// callback must not sit and wait on.
    pub fn replace(expander: &Expander, backspaces: usize, text: &str) {
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

        let long = text.chars().count() > TYPE_LIMIT;

        // Short text is typed as Unicode, which leaves the clipboard exactly
        // as the user had it. Long text is pasted, because typing it is both
        // slow to watch and unreliable in applications that drop rapid
        // synthetic input.
        if !long {
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

        if long {
            paste_text(text);
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
    fn paste_text(text: &str) {
        let Ok(mut board) = arboard::Clipboard::new() else {
            crate::say!("could not open the clipboard to paste a snippet");
            return;
        };

        let previous = board.get_text().ok();

        if board.set_text(text.to_string()).is_err() {
            return;
        }

        // The same settle every paste in Sill needs: writing and immediately
        // pasting races the target application's read of the clipboard.
        std::thread::sleep(std::time::Duration::from_millis(60));
        crate::dictation::paste::chord();

        // After the paste has been read, not before. Restoring immediately
        // would hand the target the old contents instead of the snippet.
        if let Some(previous) = previous {
            std::thread::sleep(std::time::Duration::from_millis(120));
            let _ = board.set_text(previous);
        }
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
        for key in [vk::VK_RETURN, vk::VK_TAB, vk::VK_LEFT, vk::VK_HOME, vk::VK_ESCAPE] {
            assert!(resets_context(key.0 as u32), "{key:?} should reset");
        }
        for key in [vk::VK_A, vk::VK_SPACE, vk::VK_OEM_1] {
            assert!(!resets_context(key.0 as u32), "{key:?} should not reset");
        }
    }
}
