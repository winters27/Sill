//! Private mode: one switch that stops Sill recording anything.
//!
//! Three subsystems of the launcher watch the machine on the person's behalf.
//! The clipboard history records everything copied. Dictation listens to a
//! microphone. Capture photographs the screen, for a screenshot, for the
//! switcher's previews, for reading text out of a picture, and for handing the
//! screen to a model. Every one of them is wanted almost all of the time and
//! none of them is wanted while somebody is typing a password into a shared
//! screen.
//!
//! ## The property this has to have
//!
//! **Somebody who believes private mode is on and is wrong is worse off than
//! somebody who never turned it on.** That shapes three decisions.
//!
//! **It is a preference, so it survives a restart.** A mode that quietly
//! switched itself back on the next time Sill started would be the exact
//! failure above: everything they turned off comes back and nothing says so.
//! The other way round costs somebody an eventual "why is my clipboard history
//! empty", and the row says why.
//!
//! **Nothing that could record asks whether it may.** Three call sites, each
//! remembering to check a flag, is the shape that has already lost icons,
//! subtitles and actions in this codebase, and here it would lose somebody's
//! privacy. So the check is at the one place each thing has to pass through:
//! [`crate::capture::region`] and [`crate::capture::window`] take an [`Allowed`]
//! that only [`allow`] can make, so a screenshot cannot be taken without one
//! and a new caller does not compile until it has asked.
//!
//! **The watchers are stopped, not told to ignore.** The clipboard listener
//! owns a thread and a hidden window, and dictation owns a low-level keyboard
//! hook. Leaving either running in order to decline what it sees is both a
//! cost rule 23 refuses and a thing that only has to be got wrong once.
//!
//! ## What it does not stop
//!
//! Sill still remembers what was launched, because ranking is what makes the
//! launcher work and a list of programs is not a recording of the person. It
//! is named here so that nobody has to infer it.

/// Whether Sill is currently recording anything.
///
/// A mirror of the preference, in a form the parts that cannot reach the
/// preference store can read. The clipboard's `Rules` are cached on the
/// watcher's thread for the same reason and with the same warning attached:
/// preferences are authoritative and this is written from the one place that
/// applies them, [`crate::commands::settings::apply_settings`] and the startup
/// that shares its shape.
#[derive(Debug, Default)]
pub struct Privacy {
    paused: std::sync::atomic::AtomicBool,
}

impl Privacy {
    /// Reads the switch. A relaxed atomic load, which is why it is free enough
    /// to sit in front of every screenshot.
    pub fn paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Points the mirror at what the preferences say.
    pub fn set(&self, paused: bool) {
        self.paused
            .store(paused, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Permission to photograph the screen, which only [`allow`] can grant.
///
/// The point is the private field: nothing outside this module can build one,
/// so a capture cannot happen without having asked. A new way of taking a
/// picture does not compile until it has, which is the difference between a
/// rule and a rule somebody has to remember.
#[derive(Debug)]
pub struct Allowed(());

/// What a refused capture says.
///
/// One sentence, in one place, because it is shown by five different commands
/// and a person who sees two different wordings for one state has to work out
/// whether they mean the same thing.
pub const REFUSED: &str = "Private mode is on, so Sill is not capturing the screen.";

/// What a refused dictation says.
pub const REFUSED_LISTENING: &str = "Private mode is on, so Sill is not listening.";

/// Permission to photograph the screen, or the reason there is none.
pub fn allow(privacy: &Privacy) -> Result<Allowed, String> {
    if privacy.paused() {
        return Err(REFUSED.to_string());
    }

    Ok(Allowed(()))
}

/// Permission where there is nobody to ask.
///
/// For the tests and the diagnostic probes, which run without an application
/// and therefore without preferences or managed state. Named so that it reads
/// as a decision at the call site rather than as an omission, and deliberately
/// clumsy to type: it is the one way past the gate, and the only places it may
/// appear are ones with no person whose screen it could be.
pub fn allowed_regardless() -> Allowed {
    Allowed(())
}

/// What the clipboard watcher is allowed to do, given the preferences.
///
/// **The only way `Rules` are built from preferences.** Both places that apply
/// settings call this, so private mode cannot be honoured by one of them and
/// forgotten by the other, which is what would have happened if each had
/// carried on filling the struct in by hand.
pub fn clipboard_rules(
    prefs: &crate::preferences::Preferences,
) -> crate::clipboard::monitor::Rules {
    crate::clipboard::monitor::Rules {
        // The whole of private mode's effect on the clipboard. Everything
        // downstream of `enabled` already exists: the watcher's thread is torn
        // down rather than left running, because that is what the setting for
        // switching recording off does, and this is the same fact arriving a
        // different way.
        enabled: prefs.clipboard.enabled && !prefs.privacy.paused,
        keep_images: prefs.clipboard.keep_images,
        ignored_apps: prefs.clipboard.ignored_apps.clone(),
        secrets: prefs.clipboard.secrets,
        retain_days: prefs.clipboard.retain_days,
        max_entries: prefs.clipboard.max_entries,
        encrypt_images: prefs.clipboard.encrypt_images,
    }
}

/// What dictation is allowed to do, given the preferences.
///
/// Same reasoning as [`clipboard_rules`], and the same consequence: `enabled`
/// off means the low-level keyboard hook is removed rather than armed and
/// ignored, which is what `apply_dictation` already does with it.
///
/// **This is only half of dictation's answer.** `enabled` is the trigger, and
/// the trigger is off by default: the "Dictate" row exists to start a
/// dictation without it. So private mode is also pushed onto the service as a
/// flag of its own, and that is what refuses to open the microphone however
/// the dictation was asked for. Folding the two together here would have
/// silently broken that row on every machine that never turned the hook on.
pub fn dictation_settings(
    prefs: &crate::preferences::Preferences,
) -> crate::dictation::models::DictationSettings {
    crate::dictation::models::DictationSettings {
        enabled: prefs.dictation.enabled && !prefs.privacy.paused,
        ..prefs.dictation.clone()
    }
}

/// The report the status surface carries while private mode is on.
///
/// Keyed, so turning it on twice is one entry, and resolved when it goes off.
/// It exists because the row that switches this on is seen once and the state
/// lasts until somebody switches it off: without a standing sign, "is it still
/// on" has no answer except turning it off and seeing what changes.
pub const REPORT: &str = "privacy";

/// What that report says.
pub fn report(app: &tauri::AppHandle, paused: bool) {
    if paused {
        crate::status::report(
            app,
            REPORT,
            "Private mode is on. The clipboard history, dictation and screen \
             capture are all paused until it is switched off."
                .to_string(),
            Some("general"),
        );
    } else {
        crate::status::resolved(app, REPORT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Preferences;

    /// Everything private mode claims to stop, as one test over the
    /// derivations that stop them.
    #[test]
    fn private_mode_switches_off_recording_and_listening() {
        let mut prefs = Preferences::default();
        prefs.clipboard.enabled = true;
        prefs.dictation.enabled = true;

        assert!(
            clipboard_rules(&prefs).enabled,
            "the clipboard was already off, so this test proves nothing"
        );
        assert!(
            dictation_settings(&prefs).enabled,
            "dictation was already off, so this test proves nothing"
        );

        prefs.privacy.paused = true;

        assert!(
            !clipboard_rules(&prefs).enabled,
            "private mode left the clipboard history recording"
        );
        assert!(
            !dictation_settings(&prefs).enabled,
            "private mode left dictation armed"
        );
    }

    /// Private mode switches things off. It does not switch anything on.
    #[test]
    fn leaving_private_mode_does_not_turn_on_what_was_already_off() {
        let mut prefs = Preferences::default();
        prefs.clipboard.enabled = false;
        prefs.dictation.enabled = false;
        prefs.privacy.paused = false;

        assert!(!clipboard_rules(&prefs).enabled);
        assert!(!dictation_settings(&prefs).enabled);
    }

    /// It changes one thing about each, and nothing else.
    #[test]
    fn nothing_else_about_the_settings_changes() {
        let mut prefs = Preferences::default();
        prefs.clipboard.enabled = true;
        prefs.clipboard.retain_days = 30;
        prefs.clipboard.max_entries = 500;
        prefs.clipboard.keep_images = true;
        prefs.dictation.enabled = true;
        prefs.dictation.shortcut_key = "F9".to_string();

        let open = clipboard_rules(&prefs);
        let spoken = dictation_settings(&prefs);

        prefs.privacy.paused = true;
        let shut = clipboard_rules(&prefs);
        let silent = dictation_settings(&prefs);

        assert_eq!(shut.retain_days, open.retain_days);
        assert_eq!(shut.max_entries, open.max_entries);
        assert_eq!(shut.keep_images, open.keep_images);
        assert_eq!(shut.ignored_apps, open.ignored_apps);
        assert_eq!(silent.shortcut_key, spoken.shortcut_key);
    }

    /// A capture cannot be taken without permission, and permission is refused
    /// with words rather than with a silent empty picture.
    #[test]
    fn permission_to_photograph_the_screen_is_refused_while_it_is_on() {
        let privacy = Privacy::default();
        assert!(allow(&privacy).is_ok(), "a fresh Sill is not private");

        privacy.set(true);
        let refused = allow(&privacy).expect_err("a capture was allowed in private mode");
        assert_eq!(refused, REFUSED);

        privacy.set(false);
        assert!(
            allow(&privacy).is_ok(),
            "switching it off did not switch off"
        );
    }
}
