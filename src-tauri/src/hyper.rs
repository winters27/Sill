//! One key standing in for four modifiers at once.
//!
//! ## What it is for
//!
//! Every useful chord on Windows is taken. A hyper key is a key nobody uses
//! for anything, usually Caps Lock, that turns every other key into a shortcut
//! nothing else in the system has claimed: Hyper+T can be yours because
//! Ctrl+Alt+Shift+Win+T is nobody's.
//!
//! ## A whole chord per keystroke, not modifiers held down
//!
//! The obvious way is to press the four modifiers when the hyper key goes down
//! and release them when it comes up. That is the version that leaves somebody
//! with Ctrl stuck on.
//!
//! There are several ways to never see the key-up: the process ends, the hook
//! is removed, a remote session takes the keyboard, a lock screen takes focus.
//! Any one of them leaves four modifiers held with nothing left running to
//! release them, and the machine is unusable until they are pressed and
//! released by hand. It is not recoverable by quitting Sill, because quitting
//! Sill is one of the ways it happens.
//!
//! So nothing is held. Every key pressed while the hyper key is down produces
//! **one complete chord**: modifiers down, the key, modifiers up, in a single
//! batch. Nothing can be left behind because nothing outlives the keystroke.
//!
//! ## Why the ups matter as much as the downs
//!
//! Swallowing a key's press and letting its release through sends a program a
//! release for something it never saw pressed. Most ignore it; the ones that do
//! not, do something strange once and never again, which is the worst kind of
//! bug to be told about.
//!
//! Only the releases of keys this actually chorded are swallowed. A key that
//! was already down before the hyper key went down belongs to whatever it was
//! doing, and stealing its release would strand it.

use std::collections::BTreeSet;

/// What should happen to one key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing to do with us.
    Pass,
    /// Never reaches anything. The hyper key itself, or the release of a key
    /// whose press was turned into a chord.
    Swallow,
    /// Send this key with all four modifiers, and swallow the original.
    Chord(u32),
}

/// The state of one hyper key.
#[derive(Debug, Default)]
pub struct Hyper {
    /// The key that does the standing in, or `None` when this is off.
    key: Option<u32>,
    held: bool,
    /// Keys whose press became a chord, and whose release must be swallowed.
    ///
    /// Kept past the hyper key being released, because a key pressed while it
    /// was down is often let go afterwards, and its release has to be
    /// swallowed then too.
    chorded: BTreeSet<u32>,
}

impl Hyper {
    pub fn new() -> Self {
        Self::default()
    }

    /// Turns it on for one key, or off.
    ///
    /// Clears everything else: a hyper key changed while the old one was held
    /// would otherwise leave `held` set for a key nothing is watching, and
    /// every keystroke afterwards would be a chord.
    pub fn set(&mut self, key: Option<u32>) {
        self.key = key;
        self.held = false;
        self.chorded.clear();
    }

    pub fn on(&self) -> bool {
        self.key.is_some()
    }

    /// Whether the hyper key is down right now.
    pub fn holding(&self) -> bool {
        self.held
    }

    /// What to do about one key going down or coming up.
    pub fn saw(&mut self, vk: u32, down: bool) -> Verdict {
        let Some(key) = self.key else {
            return Verdict::Pass;
        };

        if vk == key {
            // A held key repeats, and each repeat arrives as another press.
            // Setting `held` again is harmless; clearing `chorded` here would
            // not be, because the keys it holds may still be down.
            self.held = down;
            return Verdict::Swallow;
        }

        if down {
            if self.held {
                self.chorded.insert(vk);
                return Verdict::Chord(vk);
            }

            return Verdict::Pass;
        }

        // A release. Swallowed only if this is the key whose press was taken,
        // whether or not the hyper key is still down.
        if self.chorded.remove(&vk) {
            return Verdict::Swallow;
        }

        Verdict::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAPS: u32 = 0x14;
    const T: u32 = 0x54;
    const K: u32 = 0x4B;

    fn on() -> Hyper {
        let mut hyper = Hyper::new();
        hyper.set(Some(CAPS));
        hyper
    }

    #[test]
    fn off_by_default_and_nothing_is_touched() {
        let mut hyper = Hyper::new();

        assert!(!hyper.on());
        assert_eq!(hyper.saw(CAPS, true), Verdict::Pass);
        assert_eq!(hyper.saw(T, true), Verdict::Pass);
    }

    /// The hyper key never reaches anything, pressed or released.
    ///
    /// Caps Lock that still toggled Caps Lock would be a key doing two jobs,
    /// and the second one arrives in the middle of somebody's sentence.
    #[test]
    fn the_hyper_key_itself_never_gets_through() {
        let mut hyper = on();

        assert_eq!(hyper.saw(CAPS, true), Verdict::Swallow);
        assert_eq!(hyper.saw(CAPS, false), Verdict::Swallow);
    }

    #[test]
    fn a_key_pressed_while_it_is_held_becomes_a_chord() {
        let mut hyper = on();
        hyper.saw(CAPS, true);

        assert_eq!(hyper.saw(T, true), Verdict::Chord(T));
    }

    #[test]
    fn a_key_pressed_when_it_is_not_held_is_left_alone() {
        let mut hyper = on();

        assert_eq!(hyper.saw(T, true), Verdict::Pass);
    }

    /// The release of a chorded key is swallowed too.
    ///
    /// Letting it through sends a program a release for something it never saw
    /// pressed, which most ignore and a few act on once, strangely.
    #[test]
    fn the_release_of_a_chorded_key_is_swallowed() {
        let mut hyper = on();
        hyper.saw(CAPS, true);
        hyper.saw(T, true);

        assert_eq!(hyper.saw(T, false), Verdict::Swallow);
    }

    /// And only that one.
    ///
    /// A key already down before the hyper key went down belongs to whatever
    /// it was doing, and stealing its release would strand it: an application
    /// that saw the press and never the release believes it is still held.
    #[test]
    fn a_key_that_was_already_down_keeps_its_release() {
        let mut hyper = on();

        hyper.saw(K, true); // pressed first, passed through
        hyper.saw(CAPS, true);

        assert_eq!(hyper.saw(K, false), Verdict::Pass);
    }

    /// Let go after the hyper key, and it is still swallowed.
    ///
    /// Somebody holds Caps, presses T, lets Caps go, then lets T go. The
    /// release still belongs to a press nothing ever saw.
    #[test]
    fn a_chorded_key_released_after_the_hyper_key_is_still_swallowed() {
        let mut hyper = on();
        hyper.saw(CAPS, true);
        hyper.saw(T, true);
        hyper.saw(CAPS, false);

        assert_eq!(hyper.saw(T, false), Verdict::Swallow);
    }

    /// Holding a key repeats it, and every repeat is a chord.
    #[test]
    fn a_held_key_chords_on_every_repeat() {
        let mut hyper = on();
        hyper.saw(CAPS, true);

        assert_eq!(hyper.saw(T, true), Verdict::Chord(T));
        assert_eq!(hyper.saw(T, true), Verdict::Chord(T));
    }

    /// The hyper key repeating does not lose what is already chorded.
    #[test]
    fn the_hyper_key_repeating_changes_nothing() {
        let mut hyper = on();
        hyper.saw(CAPS, true);
        hyper.saw(T, true);
        hyper.saw(CAPS, true); // the repeat

        assert_eq!(hyper.saw(T, false), Verdict::Swallow);
    }

    /// Turning it off, or changing the key, forgets that it was held.
    ///
    /// Otherwise a hyper key changed while it was down leaves `held` set for a
    /// key nothing is watching any more, and every keystroke after that is a
    /// chord nobody asked for. There is no key-up coming to clear it, because
    /// the key that would send it is no longer the hyper key.
    #[test]
    fn changing_the_key_while_it_is_held_does_not_strand_it() {
        let mut hyper = on();
        hyper.saw(CAPS, true);
        assert!(hyper.holding());

        hyper.set(Some(K));

        assert!(!hyper.holding());
        assert_eq!(hyper.saw(T, true), Verdict::Pass);
    }

    #[test]
    fn turning_it_off_stops_everything() {
        let mut hyper = on();
        hyper.saw(CAPS, true);

        hyper.set(None);

        assert!(!hyper.on());
        assert_eq!(hyper.saw(CAPS, true), Verdict::Pass);
        assert_eq!(hyper.saw(T, true), Verdict::Pass);
    }
}
