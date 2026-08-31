//! Double-tapping a modifier key.
//!
//! Ported from AuraKey's `DoubleTapTracker`, which had the two-phase shape
//! right: idle, then waiting for a second press, with a second press inside
//! the window confirming and one outside it becoming a new first press.
//!
//! Three things are new here, and all three are because a **modifier** is
//! being watched rather than an ordinary key.
//!
//! **A held key is one press.** Windows repeats a held key, so a modifier
//! leant on for half a second arrives as a stream of presses that would
//! confirm a double-tap nobody made. A press only counts when the key was up.
//!
//! **Anything else cancels.** `Ctrl`, `C`, `Ctrl` is somebody copying and then
//! reaching for another shortcut. It is not a double-tap, and firing on it
//! would make the feature go off constantly while somebody works.
//!
//! **The other side of the pair does not count.** Left and right Control are
//! different keys to Windows and the same key to a person, so tapping one then
//! the other is a double-tap. Tracking them separately would mean the feature
//! quietly not working for anybody who does that.
//!
//! There is no `tick`. AuraKey's version expired a stale first press from a
//! loop it already had; Sill has no such loop and is not going to grow one for
//! this. Expiry is decided when the next press arrives, which is the only
//! moment anything looks.

use serde::{Deserialize, Serialize};

/// A modifier, as a person thinks of it rather than as Windows sends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Control,
    Alt,
    Shift,
    /// Either Windows key.
    Win,
}

impl Modifier {
    /// Which modifier a virtual key is, if it is one.
    ///
    /// Both sides map to the same answer. Left and right Control are one key
    /// to the person pressing them.
    pub fn of(vk: u32) -> Option<Self> {
        match vk {
            // VK_CONTROL, VK_LCONTROL, VK_RCONTROL
            0x11 | 0xA2 | 0xA3 => Some(Self::Control),
            // VK_MENU, VK_LMENU, VK_RMENU
            0x12 | 0xA4 | 0xA5 => Some(Self::Alt),
            // VK_SHIFT, VK_LSHIFT, VK_RSHIFT
            0x10 | 0xA0 | 0xA1 => Some(Self::Shift),
            // VK_LWIN, VK_RWIN
            0x5B | 0x5C => Some(Self::Win),
            _ => None,
        }
    }

    /// What to call it in the settings window.
    pub fn name(self) -> &'static str {
        match self {
            Self::Control => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Win => "Win",
        }
    }
}

/// How long the second tap has to arrive, in milliseconds.
///
/// Long enough to be comfortable and short enough that two deliberate,
/// separate presses are not read as one gesture. Every implementation of this
/// lands between three hundred and four hundred; this is the middle of that.
pub const WINDOW_MS: u64 = 350;

/// Watches for a modifier being tapped twice.
///
/// Fed every key the machine sees, so it does the least work it can: one
/// comparison for the overwhelming majority of keys, which are not modifiers
/// and only ever clear a flag.
#[derive(Debug, Default)]
pub struct Taps {
    /// The first tap of a possible pair, and when it landed.
    armed: Option<(Modifier, u64)>,
    /// Which modifiers are physically down, so a repeat is not a press.
    ///
    /// A bitmask rather than a set: there are four of them and this is touched
    /// on every keystroke on the machine.
    down: u8,
}

impl Taps {
    pub fn new() -> Self {
        Self::default()
    }

    /// A key went down. Returns the modifier if this completed a double-tap.
    ///
    /// `now_ms` is a monotonic millisecond count. Passed in rather than read
    /// here so this is a pure function of its inputs and the tests do not have
    /// to sleep to describe a gesture.
    pub fn press(&mut self, vk: u32, now_ms: u64, window_ms: u64) -> Option<Modifier> {
        let Some(modifier) = Modifier::of(vk) else {
            // Somebody is typing, or using a shortcut. Either way the tap that
            // was waiting was the start of that rather than of a pair.
            self.armed = None;
            return None;
        };

        let bit = mask(modifier);

        // Windows repeats a held key. A modifier leant on would otherwise
        // arrive as a stream of presses and confirm a tap nobody made.
        if self.down & bit != 0 {
            return None;
        }

        self.down |= bit;

        match self.armed {
            Some((first, at)) if first == modifier && now_ms.saturating_sub(at) < window_ms => {
                self.armed = None;
                Some(modifier)
            }
            // A different modifier, or the same one too late. Either way this
            // press is the first of a new pair rather than nothing at all.
            _ => {
                self.armed = Some((modifier, now_ms));
                None
            }
        }
    }

    /// A key came up.
    ///
    /// Only modifiers are tracked, and only so the next press of one can be
    /// told from a repeat. The arming is deliberately left alone: a tap is a
    /// press and a release, and the release is the middle of the gesture.
    pub fn release(&mut self, vk: u32) {
        if let Some(modifier) = Modifier::of(vk) {
            self.down &= !mask(modifier);
        }
    }

    /// Forgets everything.
    ///
    /// For the moments when what the machine did is no longer a guide to what
    /// the keyboard is doing: the hook being reinstalled, or a session lock
    /// swallowing the release of a key that is now up.
    pub fn reset(&mut self) {
        self.armed = None;
        self.down = 0;
    }
}

fn mask(modifier: Modifier) -> u8 {
    match modifier {
        Modifier::Control => 1,
        Modifier::Alt => 2,
        Modifier::Shift => 4,
        Modifier::Win => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: u32 = 0xA2;
    const RIGHT_CTRL: u32 = 0xA3;
    const ALT: u32 = 0xA4;
    const SHIFT: u32 = 0xA0;
    const C: u32 = 0x43;

    /// A tap is a press and a release, so a pair is press, release, press.
    fn tap(taps: &mut Taps, vk: u32, at: u64) -> Option<Modifier> {
        let fired = taps.press(vk, at, WINDOW_MS);
        taps.release(vk);
        fired
    }

    #[test]
    fn two_taps_inside_the_window_fire() {
        let mut taps = Taps::new();

        assert_eq!(tap(&mut taps, CTRL, 0), None);
        assert_eq!(tap(&mut taps, CTRL, 200), Some(Modifier::Control));
    }

    #[test]
    fn two_taps_too_far_apart_do_not() {
        let mut taps = Taps::new();

        assert_eq!(tap(&mut taps, CTRL, 0), None);
        assert_eq!(tap(&mut taps, CTRL, WINDOW_MS + 1), None);
    }

    /// The second one becomes the first of a new pair rather than nothing, so
    /// a slow double-tap followed by a quick one still works.
    #[test]
    fn a_late_second_tap_starts_a_new_pair() {
        let mut taps = Taps::new();

        tap(&mut taps, CTRL, 0);
        assert_eq!(tap(&mut taps, CTRL, 1000), None);
        assert_eq!(tap(&mut taps, CTRL, 1100), Some(Modifier::Control));
    }

    /// Windows repeats a held key. Leaning on Ctrl is not a double-tap.
    #[test]
    fn a_held_key_is_one_press_however_many_windows_sends() {
        let mut taps = Taps::new();

        assert_eq!(taps.press(CTRL, 0, WINDOW_MS), None);
        for at in 1..20 {
            assert_eq!(
                taps.press(CTRL, at * 30, WINDOW_MS),
                None,
                "a repeat at {at} was read as a press",
            );
        }
    }

    /// `Ctrl`, `C`, `Ctrl` is somebody copying and then reaching for another
    /// shortcut. Firing on it would make this go off constantly.
    #[test]
    fn a_key_pressed_in_between_cancels_the_pair() {
        let mut taps = Taps::new();

        taps.press(CTRL, 0, WINDOW_MS);
        taps.press(C, 20, WINDOW_MS);
        taps.release(C);
        taps.release(CTRL);

        assert_eq!(tap(&mut taps, CTRL, 60), None);
    }

    /// Sill's own summon is Alt+Space, and it must not arm anything.
    #[test]
    fn a_shortcut_using_the_modifier_does_not_arm_it() {
        let mut taps = Taps::new();
        const SPACE: u32 = 0x20;

        taps.press(ALT, 0, WINDOW_MS);
        taps.press(SPACE, 10, WINDOW_MS);
        taps.release(SPACE);
        taps.release(ALT);

        assert_eq!(tap(&mut taps, ALT, 50), None);
    }

    /// Left and right are different keys to Windows and one key to a person.
    #[test]
    fn either_side_of_the_pair_counts_as_the_same_key() {
        let mut taps = Taps::new();

        assert_eq!(tap(&mut taps, CTRL, 0), None);
        assert_eq!(tap(&mut taps, RIGHT_CTRL, 100), Some(Modifier::Control));
    }

    /// Ctrl then Alt is two different keys, not a double-tap of either.
    #[test]
    fn two_different_modifiers_are_not_a_pair() {
        let mut taps = Taps::new();

        assert_eq!(tap(&mut taps, CTRL, 0), None);
        assert_eq!(tap(&mut taps, ALT, 100), None);
        // And the Alt is now the first of its own pair.
        assert_eq!(tap(&mut taps, ALT, 200), Some(Modifier::Alt));
    }

    #[test]
    fn every_modifier_can_be_the_one() {
        for (vk, wanted) in [
            (CTRL, Modifier::Control),
            (ALT, Modifier::Alt),
            (SHIFT, Modifier::Shift),
            (0x5B, Modifier::Win),
        ] {
            let mut taps = Taps::new();

            assert_eq!(tap(&mut taps, vk, 0), None);
            assert_eq!(tap(&mut taps, vk, 100), Some(wanted));
        }
    }

    /// After the hook is reinstalled the machine's state is no guide to what
    /// the keyboard is doing, and a key believed down would never press again.
    #[test]
    fn resetting_forgets_a_key_it_thought_was_held() {
        let mut taps = Taps::new();

        taps.press(CTRL, 0, WINDOW_MS);
        taps.reset();

        assert_eq!(tap(&mut taps, CTRL, 10), None, "the reset left it armed");
        assert_eq!(tap(&mut taps, CTRL, 60), Some(Modifier::Control));
    }

    /// Firing clears the pair, so three taps are one gesture and not two.
    #[test]
    fn a_third_tap_does_not_fire_again_on_its_own() {
        let mut taps = Taps::new();

        tap(&mut taps, CTRL, 0);
        assert_eq!(tap(&mut taps, CTRL, 100), Some(Modifier::Control));
        assert_eq!(tap(&mut taps, CTRL, 200), None);
        assert_eq!(tap(&mut taps, CTRL, 300), Some(Modifier::Control));
    }
}
