//! What private mode actually stops.
//!
//! Not that a flag was set. Each of the three things it claims to pause has a
//! gate of its own, and each gate is asked here with private mode on and off:
//!
//! | claim | the gate | what asks it |
//! | --- | --- | --- |
//! | the clipboard history stops recording | `monitor::records` | `capture_with`, before the clipboard is opened |
//! | dictation stops listening | `DictationService::may_listen` | `start`, before the microphone |
//! | the screen stops being photographed | `privacy::allow` | every caller of `capture::region` and `capture::window`, by the type system |
//!
//! Two of those cannot be got wrong later, and that is deliberate. A capture
//! takes a `privacy::Allowed` and nothing outside `privacy` can make one, so a
//! new way of photographing the screen does not compile until it has asked;
//! and `verify-source` refuses a `monitor::Rules` built anywhere but
//! `privacy.rs`, so a second place applying settings cannot spell the seven
//! fields out and quietly leave private mode off.

use crate::clipboard::monitor;
use crate::dictation::service::DictationService;
use crate::preferences::Preferences;
use crate::privacy;

/// Everything switched on, which is what makes the assertions below mean
/// something: a test against a default Sill would pass with private mode doing
/// nothing at all, because the clipboard and the trigger are both off there.
fn recording() -> Preferences {
    let mut prefs = Preferences::default();
    prefs.clipboard.enabled = true;
    prefs.dictation.enabled = true;
    prefs
}

/// The clipboard's own gate, asked with private mode both ways.
///
/// `records` is what `capture_with` asks before it opens the clipboard, so a
/// `false` here is a copy that is never read, let alone written.
#[test]
fn a_copy_is_not_read_while_private_mode_is_on() {
    let mut prefs = recording();

    let open = monitor::records(&privacy::clipboard_rules(&prefs), Some("notepad.exe"));
    assert!(
        open,
        "the history was not recording, so this proves nothing"
    );

    prefs.privacy.paused = true;

    for source in [None, Some("notepad.exe"), Some("chrome.exe"), Some("")] {
        assert!(
            !monitor::records(&privacy::clipboard_rules(&prefs), source),
            "a copy from {source:?} would still be recorded in private mode"
        );
    }
}

/// And switching it off starts recording again, rather than leaving the
/// history off for good.
#[test]
fn turning_private_mode_off_starts_the_history_again() {
    let mut prefs = recording();
    prefs.privacy.paused = true;
    assert!(!monitor::records(
        &privacy::clipboard_rules(&prefs),
        Some("notepad.exe")
    ));

    prefs.privacy.paused = false;
    assert!(monitor::records(
        &privacy::clipboard_rules(&prefs),
        Some("notepad.exe")
    ));
}

/// It pauses recording. It does not switch recording on.
#[test]
fn leaving_private_mode_does_not_start_a_history_nobody_asked_for() {
    let mut prefs = Preferences::default();
    prefs.clipboard.enabled = false;
    prefs.privacy.paused = true;

    assert!(!monitor::records(&privacy::clipboard_rules(&prefs), None));

    prefs.privacy.paused = false;
    assert!(
        !monitor::records(&privacy::clipboard_rules(&prefs), None),
        "leaving private mode switched the clipboard history on"
    );
}

/// The watcher is stopped rather than left running and declining.
///
/// The thread owns a hidden window and is woken by every copy on the machine.
/// Both places that apply settings decide whether to keep it by asking
/// `privacy::clipboard_rules(..).enabled`, which is the same value asserted
/// here; `verify-source` is what stops a third place answering differently.
#[test]
fn the_watcher_is_told_to_stop_and_not_merely_to_ignore() {
    let mut prefs = recording();
    prefs.privacy.paused = true;

    assert!(
        !privacy::clipboard_rules(&prefs).enabled,
        "the watcher would be left running to decline every copy on the machine"
    );
}

/// Dictation's own gate, which is not the trigger setting.
///
/// The microphone is opened by three things that never touch the keyboard
/// hook: the "Dictate" row, the deep link, and the hook thread itself. All
/// three go through `start`, and `start` asks this first.
#[test]
fn the_microphone_is_refused_while_private_mode_is_on() {
    let service = DictationService::new();
    assert!(
        service.may_listen().is_ok(),
        "a fresh service refuses to listen, so this proves nothing"
    );

    service.set_paused(true);
    let refused = service
        .may_listen()
        .expect_err("the microphone was allowed to open in private mode");
    assert_eq!(refused.to_string(), privacy::REFUSED_LISTENING);

    service.set_paused(false);
    assert!(service.may_listen().is_ok(), "it never came back");
}

/// The regression this design exists to avoid.
///
/// `enabled` is the keyboard trigger, it is off by default, and the "Dictate"
/// row's whole purpose is starting a dictation without it. A private mode that
/// refused whenever the trigger was off would have broken that row on every
/// machine that has never installed the hook.
#[test]
fn switching_the_trigger_off_does_not_refuse_to_listen() {
    let service = DictationService::new();
    service.set_settings(crate::dictation::models::DictationSettings {
        enabled: false,
        ..Default::default()
    });

    assert!(
        service.may_listen().is_ok(),
        "the Dictate row would refuse on every machine with the trigger off"
    );
}

/// The hook is removed rather than armed and ignored.
#[test]
fn the_keyboard_hook_is_taken_away() {
    let mut prefs = recording();
    prefs.privacy.paused = true;

    assert!(
        !privacy::dictation_settings(&prefs).enabled,
        "the low-level keyboard hook would stay installed in private mode"
    );
}

/// The screen, which is the one the type system enforces.
#[test]
fn the_screen_cannot_be_photographed_while_private_mode_is_on() {
    let privacy_state = privacy::Privacy::default();
    assert!(privacy::allow(&privacy_state).is_ok());

    privacy_state.set(true);
    assert_eq!(
        privacy::allow(&privacy_state).expect_err("a capture was allowed"),
        privacy::REFUSED
    );
}

/// The row exists, and it is the row all three files mean.
///
/// `registry::PRIVATE_MODE` names it in three places that never see each
/// other: the row is built in `registry.rs`, the search fills its state in from
/// `commands/search.rs`, and the action that flips it dispatches on it in
/// `actions/mod.rs`. A fourth spelling would compile, and the symptom would be
/// a row that draws and does nothing, or one that never shows which way it is
/// set.
#[test]
fn the_row_is_the_one_every_file_means() {
    let id = crate::registry::builtin_id(crate::registry::PRIVATE_MODE);

    let row = crate::registry::builtins()
        .into_iter()
        .find(|one| one.id == id)
        .expect("the private mode row is in the root list");

    assert_eq!(row.mode, "builtin", "it is not run through `RunBuiltin`");
    assert_eq!(
        row.entrypoint,
        crate::registry::PRIVATE_MODE,
        "`RunBuiltin` matches on the entrypoint, so this is what it dispatches on"
    );
    assert!(
        row.icon.is_some(),
        "it wears the shell's padlock rather than Sill's gear"
    );
}

/// One state, three answers, and they agree.
///
/// The thing that would actually hurt somebody is private mode being on for
/// two of the three. Asserted together rather than only in three separate
/// tests, because three passing tests about three subsystems do not say that
/// one switch reaches all of them.
#[test]
fn one_switch_reaches_all_three() {
    let mut prefs = recording();
    let service = DictationService::new();
    let screen = privacy::Privacy::default();

    for paused in [false, true, false] {
        prefs.privacy.paused = paused;
        service.set_paused(paused);
        screen.set(paused);

        let recording = monitor::records(&privacy::clipboard_rules(&prefs), Some("notepad.exe"));
        let listening = service.may_listen().is_ok();
        let watching = privacy::allow(&screen).is_ok();

        assert_eq!(
            (recording, listening, watching),
            (!paused, !paused, !paused),
            "with private mode {}, the clipboard says {recording}, dictation says \
             {listening} and capture says {watching}",
            if paused { "on" } else { "off" }
        );
    }
}
