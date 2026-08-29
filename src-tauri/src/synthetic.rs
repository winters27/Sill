//! The mark Sill puts on keystrokes it sends itself.
//!
//! The dictation hook has to ignore Sill's own typing, or the Ctrl+V that
//! pastes a finished transcript is read straight back as user input and the
//! trigger fires on its own paste.
//!
//! The obvious way to do that is to ignore anything flagged `LLKHF_INJECTED`,
//! and it is wrong. That flag means "not typed on a physical keyboard", which
//! covers a great deal more than us: keyboard software that remaps keys,
//! macro keys, on-screen keyboards, Remote Desktop and every other remote
//! session, and accessibility tools. Ignoring all of it means the trigger
//! silently does nothing for anyone using any of those, with no error to
//! explain why.
//!
//! So mark our own instead. `dwExtraInfo` rides along with every synthetic
//! key event and comes back untouched in the hook, which makes "ours" a
//! question with an exact answer.

/// Stamped into `dwExtraInfo` on every key event Sill synthesises.
///
/// The bytes spell `SILL`. The value only has to be one no other program is
/// likely to pick, since a collision would mean ignoring somebody else's
/// keystroke rather than anything worse.
pub const SILL_SYNTHETIC: usize = 0x5349_4C4C;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_is_not_zero() {
        // Zero is what `dwExtraInfo` is when nobody sets it, which is most
        // synthetic input in the world. A zero mark would therefore mean
        // "ignore almost every key anyone else sends", which is the exact
        // bug this constant exists to fix.
        assert_ne!(SILL_SYNTHETIC, 0);
    }
}
