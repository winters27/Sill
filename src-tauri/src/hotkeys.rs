//! Global hotkeys through the low-level keyboard hook.
//!
//! ## Why the hook and not only `RegisterHotKey`
//!
//! Windows' registration is first come, first served and says nothing about
//! who came first: a key another program holds is refused with no name
//! attached, and the shortcut crate cannot even name some keys (the Menu key
//! has no entry in its table, so it fails as if the key were taken). Sill
//! already keeps a `WH_KEYBOARD_LL` hook on the machine for snippet expansion,
//! the hyper key and the double-tap, and that hook sees every keystroke before
//! any registration does. A chord matched here is swallowed, so nothing behind
//! Sill sees it: Sill is the first layer, whatever else wanted the key.
//!
//! ## Why the registrations stay
//!
//! A hook cannot prove it is alive. Windows removes a slow low-level hook
//! silently, and from the inside everything still looks armed (`hooks.rs`).
//! The registrations are kept as the backstop: while the hook is alive it
//! swallows the key and they never fire; if the hook has died they still
//! summon, and the summon is the moment `hooks::check` puts the hook back. A
//! registration Windows refuses is therefore not a dead key any more, only a
//! missing backstop, and the settings row says exactly that.
//!
//! ## What it costs
//!
//! One virtual-key comparison per keystroke against a list of a few entries,
//! inside a callback that already runs for the features above. The modifier
//! state is only read when the key itself matches. The hook is armed whenever
//! any hotkey is set, which is always, because the summon key is one; that
//! was already true on any machine with snippet expansion on.

use tauri::AppHandle;

/// A chord as the hook sees it: which modifiers must be down, and the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub vk: u32,
}

/// What a chord does. The same functions the registrations call.
#[derive(Debug, Clone)]
pub enum Target {
    Summon,
    Switcher,
    Capture,
    CaptureScreen,
    Binding(crate::bindings::Binding),
}

#[derive(Debug, Clone)]
pub struct Hotkey {
    pub accelerator: String,
    pub chord: Chord,
    pub target: Target,
}

/// The modifiers down at a keystroke.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// Every hotkey the preferences name, in the order they are checked.
pub fn from_prefs(prefs: &crate::preferences::Preferences) -> Vec<Hotkey> {
    let mut out = Vec::new();
    push(&mut out, &prefs.hotkey.summon, Target::Summon);
    push(&mut out, &prefs.hotkey.switcher, Target::Switcher);
    push(&mut out, &prefs.hotkey.capture, Target::Capture);
    push(&mut out, &prefs.hotkey.capture_screen, Target::CaptureScreen);
    for binding in &prefs.bindings {
        push(&mut out, &binding.accelerator, Target::Binding(binding.clone()));
    }
    out
}

fn push(out: &mut Vec<Hotkey>, accelerator: &str, target: Target) {
    if let Some(chord) = parse(accelerator) {
        out.push(Hotkey {
            accelerator: accelerator.to_string(),
            chord,
            target,
        });
    }
}

/// The first hotkey this keystroke is, if it is one.
///
/// Exact on the modifiers: Ctrl+K is not Ctrl+Shift+K, and a chord with no
/// modifiers matches only a bare press.
pub fn hit<'a>(hotkeys: &'a [Hotkey], vk: u32, held: Held) -> Option<&'a Hotkey> {
    hotkeys.iter().find(|one| {
        one.chord.vk == vk
            && one.chord.ctrl == held.ctrl
            && one.chord.alt == held.alt
            && one.chord.shift == held.shift
            && one.chord.win == held.win
    })
}

/// Runs what a chord does, off the hook thread.
///
/// A low-level hook callback is something Windows expects back promptly, so
/// nothing here waits: windows are shown on a thread of their own, the way
/// the double-tap already does it, and the async work goes to the runtime.
pub fn dispatch(app: &AppHandle, target: Target) {
    let app = app.clone();
    match target {
        Target::Summon => {
            std::thread::spawn(move || crate::summon::toggle_main(&app));
        }
        Target::Switcher => {
            std::thread::spawn(move || crate::summon::show_switcher(&app));
        }
        Target::Capture => {
            tauri::async_runtime::spawn(async move {
                if let Err(reason) = crate::commands::system::begin_capture(app).await {
                    crate::say!("capture key: {reason}");
                }
            });
        }
        Target::CaptureScreen => {
            tauri::async_runtime::spawn(async move {
                if let Err(reason) = crate::commands::system::capture_screen(app).await {
                    crate::say!("capture key: {reason}");
                }
            });
        }
        Target::Binding(binding) => {
            tauri::async_runtime::spawn(async move { crate::bindings::fire(&app, &binding).await });
        }
    }
}

/// An accelerator, as the settings recorder writes it, as a chord.
///
/// The recorder writes the browser's names for keys (`ContextMenu`, `Pause`,
/// `PageUp`) and its own for a few (`Up`, `Space`, `Escape`), and the
/// modifiers in any order. A single character is looked up on the keyboard
/// layout, so `;` and `[` are the keys marked that way on this machine; a
/// letter or digit is its own virtual key on every layout.
pub fn parse(accelerator: &str) -> Option<Chord> {
    let mut parts: Vec<&str> = accelerator.split('+').map(str::trim).collect();
    let key = parts.pop().filter(|k| !k.is_empty())?;

    let mut chord = Chord {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
        vk: 0,
    };
    for part in parts {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => chord.ctrl = true,
            "alt" | "opt" | "option" => chord.alt = true,
            "shift" => chord.shift = true,
            "super" | "win" | "meta" | "cmd" | "command" => chord.win = true,
            _ => return None,
        }
    }

    chord.vk = vk_of(key)?;
    Some(chord)
}

/// The virtual key a name stands for.
fn vk_of(key: &str) -> Option<u32> {
    let lower = key.to_ascii_lowercase();
    let named = match lower.as_str() {
        "space" => 0x20,
        "up" | "arrowup" => 0x26,
        "down" | "arrowdown" => 0x28,
        "left" | "arrowleft" => 0x25,
        "right" | "arrowright" => 0x27,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "escape" | "esc" => 0x1B,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "capslock" => 0x14,
        "scrolllock" => 0x91,
        "numlock" => 0x90,
        "printscreen" => 0x2C,
        "pause" => 0x13,
        // The key between right Alt and right Ctrl, which opens the context
        // menu. The browser calls it ContextMenu; keyboards print a menu.
        "contextmenu" | "apps" | "menu" => 0x5D,
        // The rest of what a keyboard can send and a browser can name. The
        // names are the browser's own (KeyboardEvent.key), because that is
        // what the recorder writes; the numbers are Windows' virtual keys.
        "clear" => 0x0C,
        "cancel" => 0x03,
        "help" => 0x2F,
        "select" => 0x29,
        "print" => 0x2A,
        "execute" => 0x2B,
        "sleep" | "standby" => 0x5F,
        "audiovolumeup" => 0xAF,
        "audiovolumedown" => 0xAE,
        "audiovolumemute" => 0xAD,
        "mediaplaypause" => 0xB3,
        "mediaplay" => 0xFA,
        "mediapause" => 0xB3,
        "mediastop" => 0xB2,
        "mediatracknext" => 0xB0,
        "mediatrackprevious" => 0xB1,
        "browserhome" => 0xAC,
        "browserback" => 0xA6,
        "browserforward" => 0xA7,
        "browserrefresh" => 0xA8,
        "browserstop" => 0xA9,
        "browsersearch" => 0xAA,
        "browserfavorites" => 0xAB,
        "launchmail" => 0xB4,
        "launchmediaplayer" | "selectmedia" => 0xB5,
        "launchapplication1" | "launchapp1" | "launchmycomputer" => 0xB6,
        "launchapplication2" | "launchapp2" | "launchcalculator" => 0xB7,
        "numpadadd" | "add" => 0x6B,
        "numpadsubtract" | "subtract" => 0x6D,
        "numpadmultiply" | "multiply" => 0x6A,
        "numpaddivide" | "divide" => 0x6F,
        "numpaddecimal" | "decimal" => 0x6E,
        "separator" => 0x6C,
        "attn" => 0xF6,
        "crsel" => 0xF7,
        "exsel" => 0xF8,
        "eraseeof" => 0xF9,
        "zoom" => 0xFB,
        "play" => 0xFA,
        _ => 0,
    };
    if named != 0 {
        return Some(named);
    }

    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&number) {
            return Some(0x70 + number - 1);
        }
    }
    if let Some(digit) = lower.strip_prefix("numpad").and_then(|n| n.parse::<u32>().ok()) {
        if digit <= 9 {
            return Some(0x60 + digit);
        }
    }

    let mut chars = key.chars();
    let (first, rest) = (chars.next()?, chars.next());
    if rest.is_some() {
        return None;
    }
    if first.is_ascii_alphanumeric() {
        return Some(first.to_ascii_uppercase() as u32);
    }
    layout_vk(first)
}

/// The virtual key that types a character on this keyboard layout.
#[cfg(windows)]
fn layout_vk(c: char) -> Option<u32> {
    use windows::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW;

    let mut units = [0u16; 2];
    let encoded = c.encode_utf16(&mut units);
    if encoded.len() != 1 {
        return None;
    }
    // SAFETY: takes a UTF-16 unit and returns a plain value.
    let scan = unsafe { VkKeyScanW(encoded[0]) };
    if scan == -1 {
        return None;
    }
    Some((scan as u16 & 0xFF) as u32)
}

#[cfg(not(windows))]
fn layout_vk(_c: char) -> Option<u32> {
    None
}

/// The modifiers down right now, asked of Windows at the keystroke.
#[cfg(windows)]
pub fn held_now() -> Held {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };

    // SAFETY: takes a virtual key and returns a plain value.
    let down = |vk: i32| unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 };

    Held {
        ctrl: down(VK_CONTROL.0 as i32),
        alt: down(VK_MENU.0 as i32),
        shift: down(VK_SHIFT.0 as i32),
        win: down(VK_LWIN.0 as i32) || down(VK_RWIN.0 as i32),
    }
}

#[cfg(not(windows))]
pub fn held_now() -> Held {
    Held::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(ctrl: bool, alt: bool, shift: bool, win: bool, vk: u32) -> Chord {
        Chord {
            ctrl,
            alt,
            shift,
            win,
            vk,
        }
    }

    #[test]
    fn a_key_on_its_own_is_a_chord() {
        assert_eq!(parse("F12"), Some(chord(false, false, false, false, 0x7B)));
        assert_eq!(parse("Pause"), Some(chord(false, false, false, false, 0x13)));
    }

    /// The key that started this: the shortcut crate cannot name it at all.
    #[test]
    fn the_menu_key_is_a_chord_under_every_name_it_goes_by() {
        for name in ["ContextMenu", "Menu", "Apps", "contextmenu"] {
            assert_eq!(parse(name), Some(chord(false, false, false, false, 0x5D)), "{name}");
        }
    }

    #[test]
    fn modifiers_are_read_in_any_order_and_any_spelling() {
        let wanted = Some(chord(true, false, true, false, b'K' as u32));
        assert_eq!(parse("Ctrl+Shift+K"), wanted);
        assert_eq!(parse("Shift+Control+k"), wanted);
        assert_eq!(parse("Super+Space"), Some(chord(false, false, false, true, 0x20)));
        assert_eq!(parse("Win+Space"), Some(chord(false, false, false, true, 0x20)));
        assert_eq!(parse("Alt+Space"), Some(chord(false, true, false, false, 0x20)));
    }

    /// The names a browser hands the recorder for keys nobody prints on a
    /// keycap: media, browser and launch keys, and the numpad operators.
    #[test]
    fn the_keys_a_browser_can_name_are_all_chords() {
        for (name, vk) in [
            ("AudioVolumeMute", 0xAD),
            ("MediaPlayPause", 0xB3),
            ("BrowserBack", 0xA6),
            ("LaunchApplication2", 0xB7),
            ("NumpadAdd", 0x6B),
            ("Numpad5", 0x65),
            ("F24", 0x87),
            ("Sleep", 0x5F),
            ("PrintScreen", 0x2C),
            ("Insert", 0x2D),
        ] {
            assert_eq!(parse(name), Some(chord(false, false, false, false, vk)), "{name}");
        }
    }

    #[test]
    fn what_is_not_a_chord_is_refused_rather_than_guessed() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("Ctrl+"), None);
        assert_eq!(parse("Hyper+K"), None);
        assert_eq!(parse("Ctrl+Bogus"), None);
        assert_eq!(parse("F25"), None);
    }

    #[test]
    fn a_hit_is_exact_about_its_modifiers() {
        let hotkeys = vec![
            Hotkey {
                accelerator: "Ctrl+K".into(),
                chord: parse("Ctrl+K").unwrap(),
                target: Target::Summon,
            },
            Hotkey {
                accelerator: "K".into(),
                chord: parse("K").unwrap(),
                target: Target::Switcher,
            },
        ];
        let held = |ctrl: bool, shift: bool| Held {
            ctrl,
            alt: false,
            shift,
            win: false,
        };

        assert!(matches!(
            hit(&hotkeys, b'K' as u32, held(true, false)).map(|h| &h.target),
            Some(Target::Summon)
        ));
        assert!(matches!(
            hit(&hotkeys, b'K' as u32, held(false, false)).map(|h| &h.target),
            Some(Target::Switcher)
        ));
        assert!(hit(&hotkeys, b'K' as u32, held(true, true)).is_none(), "Ctrl+Shift+K is not Ctrl+K");
        assert!(hit(&hotkeys, b'J' as u32, held(true, false)).is_none());
    }
}
