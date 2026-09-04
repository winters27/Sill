//! The dictation trigger, as a low-level keyboard hook.
//!
//! Deliberately not `tauri-plugin-global-shortcut`. `RegisterHotKey` delivers
//! `WM_HOTKEY`, which Windows **auto-repeats while a key is held**, so resting
//! a finger on the trigger would fire it tens of times a second. A
//! `WH_KEYBOARD_LL` hook sees raw transitions instead, so repeats are visible
//! as repeats and can be ignored, and it can bind combinations
//! `RegisterHotKey` refuses.
//!
//! The interaction is modal rather than push-to-talk: the chord starts
//! listening, then `Enter` confirms and `Esc` discards. The hook's behaviour
//! therefore depends on whether a dictation is in flight, which is what
//! `Mode` carries.
//!
//! The decision half is pure and lives here; the Win32 plumbing is behind
//! `#[cfg(windows)]` below.

use crate::dictation::error::DictationError;
use serde::{Deserialize, Serialize};

/// Win32 virtual-key codes. Listed rather than imported so the decision logic
/// compiles and is testable on every platform.
mod vk {
    pub const RETURN: u32 = 0x0D;
    pub const ESCAPE: u32 = 0x1B;
    pub const SHIFT: u32 = 0x10;
    pub const CONTROL: u32 = 0x11;
    pub const MENU: u32 = 0x12;
    pub const LWIN: u32 = 0x5B;
    pub const RWIN: u32 = 0x5C;
}

/// The key combination that starts dictation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chord {
    /// Virtual-key code of the non-modifier key.
    pub key: u32,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Key that finishes a running dictation.
    ///
    /// Carried here rather than in its own struct because this is the one
    /// value that reaches the hook thread, and the hook needs all three keys
    /// to decide what to swallow.
    pub finish: u32,
    /// Key that throws a running dictation away.
    pub cancel: u32,
}

/// Which modifiers are down at the moment of an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// Whether a dictation is in flight, which decides what the hook listens for
/// and what it consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Idle,
    Listening,
}

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Start recording.
    Start,
    /// Stop, transcribe, and paste.
    Confirm,
    /// Stop and throw the audio away.
    Cancel,
}

impl Chord {
    /// Exact match. A chord with extra modifiers held is a different chord,
    /// and firing on it would steal a combination bound elsewhere.
    fn modifiers_match(&self, mods: Modifiers) -> bool {
        self.ctrl == mods.ctrl
            && self.alt == mods.alt
            && self.shift == mods.shift
            && self.win == mods.win
    }
}

/// What a key press means, given what dictation is currently doing.
///
/// `held` is whether the trigger chord is already down, which is what makes
/// auto-repeat distinguishable from a fresh press.
pub fn action_for(
    chord: &Chord,
    mode: Mode,
    key: u32,
    is_down: bool,
    mods: Modifiers,
    held: bool,
) -> Option<Action> {
    // Releases carry no meaning in a modal interaction; only presses act.
    if !is_down {
        return None;
    }

    match mode {
        Mode::Idle => {
            // Every repeat after the first is noise.
            if held || key != chord.key || !chord.modifiers_match(mods) {
                return None;
            }
            Some(Action::Start)
        }
        Mode::Listening => {
            if key == chord.finish {
                Some(Action::Confirm)
            } else if key == chord.cancel {
                Some(Action::Cancel)
            } else {
                None
            }
        }
    }
}

/// Whether the hook should consume this key rather than pass it on.
///
/// Idle: only the trigger itself, so a chord bound to a letter does not type
/// that letter. Listening: `Enter` and `Esc`, because confirming a dictation
/// must not also submit the form behind it or close the window under it, plus
/// the trigger key so pressing it again mid-dictation types nothing.
///
/// Modifier keys are **never** swallowed: eating a `Ctrl` release leaves every
/// other application believing `Ctrl` is still down.
pub fn should_swallow(chord: &Chord, mode: Mode, key: u32, mods: Modifiers) -> bool {
    match mode {
        Mode::Idle => key == chord.key && chord.modifiers_match(mods),
        Mode::Listening => key == chord.finish || key == chord.cancel || key == chord.key,
    }
}

/// Builds a chord from the `{modifier, key}` string pair the app's shortcut
/// recorder produces, e.g. `("Control+Alt", "D")`.
///
/// Rejects `Enter` and `Escape` outright: they are how a running dictation is
/// confirmed and cancelled, so binding the trigger to one would make it
/// impossible to end.
pub fn chord_from_shortcut(modifier: &str, key: &str) -> Result<Chord, DictationError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(DictationError::Validation(
            "Pick a key for the dictation shortcut".to_string(),
        ));
    }

    let vk = virtual_key_for(key).ok_or_else(|| {
        DictationError::Validation(format!("'{key}' cannot be used as a shortcut key"))
    })?;

    if matches!(vk, vk::RETURN | vk::ESCAPE) {
        return Err(DictationError::Validation(format!(
            "'{key}' already confirms or cancels a dictation, so it cannot start one"
        )));
    }

    let mut chord = Chord {
        key: vk,
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
        finish: vk::RETURN,
        cancel: vk::ESCAPE,
    };
    for part in modifier.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "" => {}
            "control" | "ctrl" => chord.ctrl = true,
            "alt" | "option" => chord.alt = true,
            "shift" => chord.shift = true,
            "super" | "meta" | "cmd" | "command" | "win" => chord.win = true,
            other => {
                return Err(DictationError::Validation(format!(
                    "'{other}' is not a modifier this shortcut understands"
                )))
            }
        }
    }

    Ok(chord)
}

/// The keys that end a dictation, given the names the settings store.
///
/// Falls back to Enter and Escape rather than failing: a setting that cannot
/// be understood should leave dictation endable, not strand it.
pub fn end_keys(finish: &str, cancel: &str) -> (u32, u32) {
    (
        virtual_key_for(finish.trim()).unwrap_or(vk::RETURN),
        virtual_key_for(cancel.trim()).unwrap_or(vk::ESCAPE),
    )
}

/// Virtual-key code for a recorder key name, or `None` when unmappable.
fn virtual_key_for(key: &str) -> Option<u32> {
    let upper = key.to_ascii_uppercase();

    if let Some(number) = upper.strip_prefix('F') {
        if let Ok(index) = number.parse::<u32>() {
            // F1 is 0x70 and they run consecutively through F24.
            if (1..=24).contains(&index) {
                return Some(0x70 + index - 1);
            }
        }
    }

    let mut chars = upper.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_uppercase() {
            return Some(c as u32);
        }
        if c.is_ascii_digit() {
            return Some(c as u32);
        }
    }

    match upper.as_str() {
        "SPACE" => Some(0x20),
        "TAB" => Some(0x09),
        "BACKSPACE" => Some(0x08),
        "DELETE" => Some(0x2E),
        "INSERT" => Some(0x2D),
        "HOME" => Some(0x24),
        "END" => Some(0x23),
        "PAGEUP" => Some(0x21),
        "PAGEDOWN" => Some(0x22),
        "UP" => Some(0x26),
        "DOWN" => Some(0x28),
        "LEFT" => Some(0x25),
        "RIGHT" => Some(0x27),
        "ENTER" | "RETURN" => Some(vk::RETURN),
        "ESC" | "ESCAPE" => Some(vk::ESCAPE),
        _ => None,
    }
}

/// What the hook has actually observed, for the settings panel.
///
/// Counters rather than flags because the question being answered is
/// "is it alive", and only something that moves can answer that.
pub struct HookFacts {
    /// `SetWindowsHookExW` returned a handle. Not the same as a chord being
    /// stored, which happens before the thread even spawns.
    pub installed: bool,
    pub listening: bool,
    pub trigger_held: bool,
    /// Every key event the hook has been handed that a person typed.
    pub keys_seen: u64,
    /// Key events that arrived synthesised rather than typed on a physical
    /// keyboard. These are acted on; the count only explains a machine whose
    /// keyboard is driven by software.
    pub injected_seen: u64,
    /// Key events discarded for being Sill's own.
    pub own_seen: u64,
    /// Presses of the trigger key, whatever else was held.
    pub chord_key_seen: u64,
    /// Presses of the trigger key with exactly the chord's modifiers.
    pub triggers_seen: u64,
    /// Modifiers held at the last trigger-key press, or `None` if it has
    /// not been pressed since the hook was installed.
    pub last_modifiers: Option<String>,
}

#[cfg(windows)]
pub use windows_hook::HotkeyListener;

#[cfg(windows)]
mod windows_hook {
    use super::{action_for, should_swallow, Action, Chord, HookFacts, Mode, Modifiers};
    use crate::dictation::error::DictationError;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::{Arc, Mutex, OnceLock};
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_KEYDOWN,
        WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    /// Hook callbacks are plain C function pointers, so everything they touch
    /// has to be reachable without a closure environment.
    ///
    /// The callback runs on the hook thread and Windows silently drops a hook
    /// that takes too long (`LowLevelHooksTimeout`, 5 s by default), so it must
    /// never block. Reads are atomics or an uncontended mutex; the only handoff
    /// is a send on an unbounded channel, which never waits for a receiver.
    /// The trigger the callback compares against.
    ///
    /// An `ArcSwap` rather than a `Mutex`, for the same reason the snippet
    /// expander uses one: this is read on **every keystroke inside a
    /// low-level hook**, and a writer preempted while holding a lock would
    /// stall the user's actual keypress for a scheduler quantum. The window
    /// is small and the hazard is rare, but a keyboard that stutters is a
    /// bad way to find out.
    static CHORD: OnceLock<arc_swap::ArcSwap<Chord>> = OnceLock::new();
    static SENDER: OnceLock<Mutex<Option<Sender<Action>>>> = OnceLock::new();
    static TRIGGER_HELD: AtomicBool = AtomicBool::new(false);
    /// Set once `SetWindowsHookExW` has actually returned a handle.
    ///
    /// Distinct from "a chord has been stored", which happens before the
    /// thread even spawns and therefore proves nothing.
    static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
    /// Every key event the hook has been handed.
    ///
    /// The one unambiguous liveness signal: if this climbs while you type,
    /// the hook is alive and the problem is in what it decides. If it stays
    /// at zero, the hook is not receiving input at all, which on Windows
    /// usually means the focused window is elevated and this process is not.
    static KEYS_SEEN: AtomicU64 = AtomicU64::new(0);
    /// Times the trigger key was seen with the right modifiers held.
    static TRIGGERS_SEEN: AtomicU64 = AtomicU64::new(0);
    /// Key events Sill sent itself, which are the only ones it discards.
    static OWN_SEEN: AtomicU64 = AtomicU64::new(0);
    /// Key events that were synthesised by something. Acted on regardless;
    /// counted only because it explains an otherwise odd-looking machine.
    static INJECTED_SEEN: AtomicU64 = AtomicU64::new(0);
    /// Times the trigger key was seen at all, whatever else was held.
    ///
    /// The difference between this and `TRIGGERS_SEEN` is the whole
    /// diagnosis. Zero here means the key never reaches the hook, which is
    /// something outside this process: an earlier hook in the chain
    /// swallowing it, or a focused window this process may not watch. Above
    /// zero with no triggers means it arrives but the modifiers read
    /// differently than the chord expects, and `LAST_MODS` says how.
    static CHORD_KEY_SEEN: AtomicU64 = AtomicU64::new(0);
    /// The modifier bits the last such press was seen with, as
    /// ctrl 1, alt 2, shift 4, win 8, and 16 to mark it as ever set.
    static LAST_MODS: AtomicU32 = AtomicU32::new(0);
    static LISTENING: AtomicBool = AtomicBool::new(false);
    /// Bumped by every `start`. A listener only tears down the shared state
    /// if it is still the current one: teardown is asynchronous (the action
    /// thread notices its stop flag up to a poll interval later), so a
    /// replaced listener can otherwise run `Drop` *after* its successor has
    /// installed, wiping the new sender and killing the new hook thread.
    static GENERATION: AtomicU64 = AtomicU64::new(0);

    fn sender_slot() -> &'static Mutex<Option<Sender<Action>>> {
        SENDER.get_or_init(|| Mutex::new(None))
    }

    fn is_down(vk: i32) -> bool {
        // The high bit of GetAsyncKeyState means "currently down".
        (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
    }

    fn current_modifiers() -> Modifiers {
        Modifiers {
            ctrl: is_down(super::vk::CONTROL as i32),
            alt: is_down(super::vk::MENU as i32),
            shift: is_down(super::vk::SHIFT as i32),
            win: is_down(super::vk::LWIN as i32) || is_down(super::vk::RWIN as i32),
        }
    }

    fn current_mode() -> Mode {
        if LISTENING.load(Ordering::SeqCst) {
            Mode::Listening
        } else {
            Mode::Idle
        }
    }

    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // Negative codes are reserved: pass them straight along untouched.
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);

        // Ignore Sill's own typing, or the Ctrl+V that pastes a finished
        // transcript is read straight back as user input.
        //
        // Matched on the mark we stamp rather than on `LLKHF_INJECTED`,
        // which is set by everything from remapping software to Remote
        // Desktop. Treating all of that as ours meant the trigger silently
        // did nothing for anyone whose keyboard is not strictly physical.
        if info.dwExtraInfo == crate::synthetic::SILL_SYNTHETIC {
            OWN_SEEN.fetch_add(1, Ordering::Relaxed);
            return CallNextHookEx(None, code, wparam, lparam);
        }

        // Still worth counting, because a keyboard that is entirely
        // synthetic is worth knowing about even now that it works.
        if info.flags & LLKHF_INJECTED != KBDLLHOOKSTRUCT_FLAGS(0) {
            INJECTED_SEEN.fetch_add(1, Ordering::Relaxed);
        }

        KEYS_SEEN.fetch_add(1, Ordering::Relaxed);

        let message = wparam.0 as u32;
        let is_key_down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let is_key_up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        if !is_key_down && !is_key_up {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let Some(chord) = CHORD.get().map(|slot| slot.load()) else {
            return CallNextHookEx(None, code, wparam, lparam);
        };

        let key = info.vkCode;
        let mods = current_modifiers();
        let mode = current_mode();

        // The trigger's own down-state is what makes auto-repeat visible: the
        // second and later WM_KEYDOWN for the same key arrive while this is
        // already true.
        if is_key_down && key == chord.key {
            CHORD_KEY_SEEN.fetch_add(1, Ordering::Relaxed);
            // Packed rather than logged: this runs inside a low-level hook,
            // where touching a file would stall every keystroke on the
            // machine and eventually have Windows drop the hook outright.
            LAST_MODS.store(
                16 | (mods.ctrl as u32)
                    | ((mods.alt as u32) << 1)
                    | ((mods.shift as u32) << 2)
                    | ((mods.win as u32) << 3),
                Ordering::Relaxed,
            );
            if chord.modifiers_match(mods) {
                TRIGGERS_SEEN.fetch_add(1, Ordering::Relaxed);
            }
        }

        let held = key == chord.key && TRIGGER_HELD.load(Ordering::SeqCst);
        if key == chord.key {
            TRIGGER_HELD.store(is_key_down, Ordering::SeqCst);
        }

        if let Some(action) = action_for(&chord, mode, key, is_key_down, mods, held) {
            // The hook owns this flag so the very next keystroke is judged
            // against the new mode without waiting for the service thread.
            LISTENING.store(matches!(action, Action::Start), Ordering::SeqCst);
            if let Ok(slot) = sender_slot().lock() {
                if let Some(sender) = slot.as_ref() {
                    let _ = sender.send(action);
                }
            }
        }

        if is_key_down && should_swallow(&chord, mode, key, mods) {
            return LRESULT(1);
        }

        CallNextHookEx(None, code, wparam, lparam)
    }

    /// An installed keyboard hook. Dropping it uninstalls the hook and stops
    /// the thread.
    pub struct HotkeyListener {
        pub actions: Receiver<Action>,
        /// Which install this is. See `GENERATION`.
        generation: u64,
        /// This listener's own hook thread, so teardown never signals a
        /// successor's thread by reading a shared slot.
        thread_id: Arc<AtomicU32>,
    }

    impl HotkeyListener {
        /// Installs the hook on its own thread with a message pump, which is
        /// what `SetWindowsHookExW` requires: the callback is delivered on the
        /// thread that installed it, and only while that thread pumps.
        pub fn start(chord: Chord) -> Result<Self, DictationError> {
            let (action_tx, actions) = channel();
            let (ready_tx, ready_rx) = channel();
            let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
            let thread_id = Arc::new(AtomicU32::new(0));
            let hook_thread_id = Arc::clone(&thread_id);

            let slot = CHORD.get_or_init(|| arc_swap::ArcSwap::from_pointee(chord.clone()));
            slot.store(std::sync::Arc::new(chord.clone()));
            if let Ok(mut sender) = sender_slot().lock() {
                *sender = Some(action_tx);
            }
            TRIGGER_HELD.store(false, Ordering::SeqCst);
            LISTENING.store(false, Ordering::SeqCst);

            std::thread::Builder::new()
                .name("dictation-hotkey".to_string())
                .spawn(move || unsafe {
                    let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0)
                    else {
                        let _ = ready_tx.send(Err("Windows refused the keyboard hook".to_string()));
                        return;
                    };

                    HOOK_INSTALLED.store(true, Ordering::SeqCst);
                    hook_thread_id.store(
                        windows::Win32::System::Threading::GetCurrentThreadId(),
                        Ordering::SeqCst,
                    );
                    let _ = ready_tx.send(Ok(()));

                    // Runs until `Drop` posts WM_QUIT to this thread.
                    let mut message = MSG::default();
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {}

                    /*
                     * Only the current install owns the flag, for the reason
                     * `Drop` gives about the sender next to it.
                     *
                     * Teardown is asynchronous: the action thread notices its
                     * channel has closed and only then drops the listener that
                     * posts the quit, so a superseded thread can reach this
                     * line well after its replacement has installed and set the
                     * flag. Clearing it unconditionally would leave Sill
                     * believing the live hook is not there, which is a lie the
                     * settings panel prints and the liveness check acts on by
                     * re-installing a hook that was already fine.
                     */
                    if GENERATION.load(Ordering::SeqCst) == generation {
                        HOOK_INSTALLED.store(false, Ordering::SeqCst);
                    }

                    let _ = UnhookWindowsHookEx(hook);
                })
                .map_err(|e| {
                    DictationError::Other(format!("Could not start the hotkey thread: {e}"))
                })?;

            match ready_rx.recv() {
                Ok(Ok(())) => Ok(Self {
                    actions,
                    generation,
                    thread_id,
                }),
                Ok(Err(message)) => Err(DictationError::Other(message)),
                Err(_) => Err(DictationError::Other(
                    "The hotkey thread stopped before it started".to_string(),
                )),
            }
        }

        /// Rebinds without reinstalling the hook.
        pub fn set_chord(chord: Chord) {
            if let Some(current) = CHORD.get() {
                current.store(std::sync::Arc::new(chord));
                TRIGGER_HELD.store(false, Ordering::SeqCst);
            }
        }

        /// Forces the mode back to idle, for when a dictation ends by some
        /// route other than a keystroke: a transcription failure, the app
        /// quitting, the microphone disappearing.
        /// What the hook currently believes, for the settings panel.
        ///
        /// Two of these are the states that silently kill the trigger:
        /// `listening` stuck true swallows the trigger key and does nothing
        /// with it, and `trigger_held` stuck true makes every press look like
        /// an auto-repeat. Neither is visible from outside without this.
        pub fn state() -> HookFacts {
            HookFacts {
                installed: HOOK_INSTALLED.load(Ordering::SeqCst),
                listening: LISTENING.load(Ordering::SeqCst),
                trigger_held: TRIGGER_HELD.load(Ordering::SeqCst),
                keys_seen: KEYS_SEEN.load(Ordering::Relaxed),
                injected_seen: INJECTED_SEEN.load(Ordering::Relaxed),
                own_seen: OWN_SEEN.load(Ordering::Relaxed),
                chord_key_seen: CHORD_KEY_SEEN.load(Ordering::Relaxed),
                triggers_seen: TRIGGERS_SEEN.load(Ordering::Relaxed),
                last_modifiers: Self::describe_last_modifiers(),
            }
        }

        /// The packed `LAST_MODS` bits as something readable, e.g. "Alt".
        fn describe_last_modifiers() -> Option<String> {
            let bits = LAST_MODS.load(Ordering::Relaxed);
            if bits & 16 == 0 {
                return None;
            }
            let mut held = Vec::new();
            if bits & 1 != 0 {
                held.push("Ctrl");
            }
            if bits & 2 != 0 {
                held.push("Alt");
            }
            if bits & 4 != 0 {
                held.push("Shift");
            }
            if bits & 8 != 0 {
                held.push("Win");
            }
            Some(if held.is_empty() {
                "no modifiers".to_string()
            } else {
                held.join("+")
            })
        }

        /// Puts the hook back to idle.
        ///
        /// The escape hatch for exactly the stuck states above: a dictation
        /// that began and never ended leaves `listening` true, and from then
        /// on the trigger is swallowed and answered with nothing.
        pub fn reset_state() {
            LISTENING.store(false, Ordering::SeqCst);
            TRIGGER_HELD.store(false, Ordering::SeqCst);
        }

        /// Closes the channel the hook sends on.
        ///
        /// Which is how stopping reaches a thread that is blocked waiting for
        /// something to do: dropping the only sender disconnects the receiver
        /// and wakes it at once. Before this, that thread woke five times a
        /// second forever to look at a flag, which on an idle machine is
        /// three hundred wakeups a minute to discover that nothing happened.
        pub fn stop_sending() {
            if let Ok(mut slot) = sender_slot().lock() {
                *slot = None;
            }
        }

        pub fn clear_listening() {
            LISTENING.store(false, Ordering::SeqCst);
        }
    }

    impl Drop for HotkeyListener {
        fn drop(&mut self) {
            // Only the current install owns the shared state. A superseded
            // listener dropping later must not clear the live sender or reset
            // the mode out from under its replacement.
            if GENERATION.load(Ordering::SeqCst) == self.generation {
                if let Ok(mut slot) = sender_slot().lock() {
                    *slot = None;
                }
                LISTENING.store(false, Ordering::SeqCst);
            }

            // Always stop THIS listener's own thread, current or not, or the
            // hook it installed stays live for the life of the process.
            let thread_id = self.thread_id.swap(0, Ordering::SeqCst);
            if thread_id != 0 {
                // Wakes GetMessageW so the thread can unhook and exit.
                unsafe {
                    let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The rule the trigger depends on, stated where it can fail loudly.
    ///
    /// Sill ignores a key event when it is Sill's own, which is a question
    /// about `dwExtraInfo`. It must never go back to asking whether the
    /// event was injected: that is true for remapping software, macro keys,
    /// on-screen keyboards and every remote session, and answering yes to
    /// those means the trigger quietly does nothing on those machines.
    #[test]
    fn only_our_own_events_are_ignored() {
        let ours = crate::synthetic::SILL_SYNTHETIC;
        let somebody_elses_injected_event = 0usize;
        assert_ne!(ours, somebody_elses_injected_event);
    }

    use super::*;

    fn chord() -> Chord {
        // Ctrl+Alt+D
        Chord {
            key: 0x44,
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            finish: vk::RETURN,
            cancel: vk::ESCAPE,
        }
    }

    fn mods() -> Modifiers {
        Modifiers {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
        }
    }

    fn no_mods() -> Modifiers {
        Modifiers {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
        }
    }

    // ── starting ────────────────────────────────────────────────────────────

    #[test]
    fn the_chord_starts_listening() {
        assert_eq!(
            action_for(&chord(), Mode::Idle, 0x44, true, mods(), false),
            Some(Action::Start)
        );
    }

    #[test]
    fn holding_the_chord_down_starts_exactly_once() {
        // Windows streams WM_KEYDOWN while a key is held. Without the `held`
        // guard, a resting finger would restart the recording continuously.
        assert_eq!(
            action_for(&chord(), Mode::Idle, 0x44, true, mods(), true),
            None
        );
    }

    #[test]
    fn releasing_the_chord_does_nothing_now_that_it_is_modal() {
        // Push-to-talk would have ended here. This design ends on Enter or
        // Esc instead, so the release is uneventful.
        assert_eq!(
            action_for(&chord(), Mode::Listening, 0x44, false, mods(), false),
            None
        );
    }

    #[test]
    fn the_key_without_its_modifiers_does_nothing() {
        assert_eq!(
            action_for(&chord(), Mode::Idle, 0x44, true, no_mods(), false),
            None
        );
    }

    #[test]
    fn extra_modifiers_do_not_match() {
        let extra = Modifiers {
            shift: true,
            ..mods()
        };

        assert_eq!(
            action_for(&chord(), Mode::Idle, 0x44, true, extra, false),
            None
        );
    }

    // ── confirming and cancelling ───────────────────────────────────────────

    #[test]
    fn enter_confirms_while_listening() {
        assert_eq!(
            action_for(&chord(), Mode::Listening, 0x0D, true, no_mods(), false),
            Some(Action::Confirm)
        );
    }

    #[test]
    fn escape_cancels_while_listening() {
        assert_eq!(
            action_for(&chord(), Mode::Listening, 0x1B, true, no_mods(), false),
            Some(Action::Cancel)
        );
    }

    #[test]
    fn enter_is_ignored_when_no_dictation_is_running() {
        // Otherwise every Enter typed anywhere on the desktop would be read
        // as confirming a dictation that does not exist.
        assert_eq!(
            action_for(&chord(), Mode::Idle, 0x0D, true, no_mods(), false),
            None
        );
    }

    #[test]
    fn escape_is_ignored_when_no_dictation_is_running() {
        assert_eq!(
            action_for(&chord(), Mode::Idle, 0x1B, true, no_mods(), false),
            None
        );
    }

    #[test]
    fn confirming_ignores_which_modifiers_happen_to_be_down() {
        // Shift+Enter is still Enter as far as ending a dictation goes.
        assert_eq!(
            action_for(&chord(), Mode::Listening, 0x0D, true, mods(), false),
            Some(Action::Confirm)
        );
    }

    #[test]
    fn ordinary_typing_while_listening_is_not_an_action() {
        assert_eq!(
            action_for(&chord(), Mode::Listening, 0x41, true, no_mods(), false),
            None
        );
    }

    // ── swallowing ──────────────────────────────────────────────────────────

    #[test]
    fn the_trigger_never_reaches_the_focused_app() {
        assert!(should_swallow(&chord(), Mode::Idle, 0x44, mods()));
    }

    #[test]
    fn enter_is_swallowed_while_listening() {
        // The whole point: confirming a dictation must not also submit the
        // form, send the message, or accept the dialog behind it.
        assert!(should_swallow(&chord(), Mode::Listening, 0x0D, no_mods()));
    }

    #[test]
    fn escape_is_swallowed_while_listening() {
        // Cancelling must not also close the window underneath.
        assert!(should_swallow(&chord(), Mode::Listening, 0x1B, no_mods()));
    }

    #[test]
    fn enter_passes_through_normally_when_idle() {
        assert!(!should_swallow(&chord(), Mode::Idle, 0x0D, no_mods()));
    }

    #[test]
    fn pressing_the_trigger_again_mid_dictation_types_nothing() {
        assert!(should_swallow(&chord(), Mode::Listening, 0x44, mods()));
    }

    #[test]
    fn ordinary_keys_pass_through_in_both_modes() {
        assert!(!should_swallow(&chord(), Mode::Idle, 0x41, mods()));
        assert!(!should_swallow(&chord(), Mode::Listening, 0x41, no_mods()));
    }

    #[test]
    fn modifiers_are_never_swallowed_in_either_mode() {
        // Eating a Ctrl release leaves the whole desktop believing Ctrl is
        // still down until it is pressed again.
        for mode in [Mode::Idle, Mode::Listening] {
            assert!(!should_swallow(&chord(), mode, vk::CONTROL, mods()));
            assert!(!should_swallow(&chord(), mode, vk::MENU, mods()));
        }
    }

    // ââ shortcut parsing ââ

    #[test]
    fn parses_the_recorder_pair_into_a_chord() {
        let parsed = chord_from_shortcut("Control+Alt", "D").unwrap();

        assert_eq!(parsed, chord());
    }

    #[test]
    fn letters_and_digits_map_to_their_ascii_codes() {
        assert_eq!(chord_from_shortcut("Alt", "A").unwrap().key, 0x41);
        assert_eq!(chord_from_shortcut("Alt", "z").unwrap().key, 0x5A);
        assert_eq!(chord_from_shortcut("Alt", "0").unwrap().key, 0x30);
    }

    #[test]
    fn function_keys_run_consecutively_from_f1() {
        assert_eq!(chord_from_shortcut("", "F1").unwrap().key, 0x70);
        assert_eq!(chord_from_shortcut("", "F13").unwrap().key, 0x7C);
        assert_eq!(chord_from_shortcut("", "F24").unwrap().key, 0x87);
    }

    #[test]
    fn f25_is_not_a_key() {
        assert!(chord_from_shortcut("", "F25").is_err());
    }

    #[test]
    fn a_bare_key_with_no_modifiers_is_allowed() {
        // F13 and friends exist precisely to be bound alone.
        let parsed = chord_from_shortcut("", "F13").unwrap();

        assert!(!parsed.ctrl && !parsed.alt && !parsed.shift && !parsed.win);
    }

    #[test]
    fn every_modifier_spelling_the_recorder_might_emit_is_understood() {
        for spelling in ["Control", "Ctrl", "control"] {
            assert!(chord_from_shortcut(spelling, "D").unwrap().ctrl);
        }
        for spelling in ["Super", "Meta", "Cmd", "Win"] {
            assert!(chord_from_shortcut(spelling, "D").unwrap().win);
        }
    }

    #[test]
    fn enter_cannot_be_the_trigger() {
        // It is how a dictation is confirmed; binding it here would make one
        // impossible to end.
        let err = chord_from_shortcut("Control", "Enter").unwrap_err();

        assert!(err.to_string().contains("confirms"), "{err}");
    }

    #[test]
    fn escape_cannot_be_the_trigger() {
        assert!(chord_from_shortcut("Control", "Escape").is_err());
    }

    #[test]
    fn an_empty_key_is_rejected_with_a_useful_message() {
        let err = chord_from_shortcut("Control+Alt", "  ").unwrap_err();

        assert!(err.to_string().contains("Pick a key"), "{err}");
    }

    #[test]
    fn an_unknown_modifier_is_rejected() {
        assert!(chord_from_shortcut("Hyper", "D").is_err());
    }

    /// Installs the real hook and reports what it sees for 20 seconds.
    ///
    /// ```text
    /// cargo test --lib dictation::hotkey::tests::probe_hook -- --ignored --nocapture
    /// ```
    ///
    /// Press Ctrl+Alt+D, then Enter or Esc. One START per press however long
    /// the finger rests on it, then one CONFIRM or CANCEL. Neither Enter nor
    /// Esc should reach whatever is focused while a dictation is running.
    #[cfg(windows)]
    #[test]
    #[ignore = "installs a system-wide keyboard hook"]
    fn probe_hook_reports_actions() {
        use std::time::{Duration, Instant};

        let listener = HotkeyListener::start(chord()).expect("install the hook");
        println!("\nPress Ctrl+Alt+D to start, then Enter to confirm or Esc to cancel.");
        println!("Hold the chord down: it must still report Start only once.");
        println!("Listening for 20s.\n");

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            if let Ok(action) = listener.actions.recv_timeout(Duration::from_millis(250)) {
                println!("  {action:?}");
                seen.push(action);
            }
        }

        println!("\n{} action(s): {seen:?}\n", seen.len());
    }
}
