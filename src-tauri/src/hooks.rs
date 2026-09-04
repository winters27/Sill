//! Whether the keyboard hooks are still receiving keys.
//!
//! Sill puts two `WH_KEYBOARD_LL` hooks on the machine: one for snippet
//! expansion, the hyper key and the double-tap gesture, and one for the
//! dictation chord. **Windows silently removes a low-level hook whose callback
//! takes too long** and tells nobody. The thread stays parked in `GetMessageW`,
//! the handle stays valid, everything Sill can ask about the hook still answers
//! yes, and every keyword, the hyper key and the double-tap stop working at the
//! same moment for no reason the user can see.
//!
//! Resume from sleep is where this happens. So this is the part that notices
//! and puts the hook back.
//!
//! ## Why there is no timer
//!
//! The obvious shape is a thread that compares a counter every few seconds.
//! `P2-02` had just finished deleting two threads of exactly that shape, and
//! rule 23 is a product requirement rather than a preference: nothing may cost
//! anything while nobody is using it. So the question is asked at moments that
//! are already happening, and never in between.
//!
//! Two moments ask it. The machine coming back from sleep or from a locked
//! session, because that is when Windows takes hooks away, and somebody
//! pressing the summon key, because that is a keystroke this process knows
//! about which the hook must also have seen.
//!
//! ## What the counter can and cannot say
//!
//! Being honest about this is the whole design.
//!
//! At a keystroke the counter is decisive: the user demonstrably pressed a key
//! a moment ago, so a hook whose count has not moved since the last time
//! anybody looked is not being called.
//!
//! **On waking the counter says nothing.** Nothing was typed while the machine
//! was asleep, so of course the count did not move, and a hook that is perfectly
//! healthy looks exactly like one Windows took away. There is no reading that
//! tells them apart, and waiting for the next keystroke to find out means the
//! user's next keyword is the one that silently does nothing. So waking
//! re-installs unconditionally, and the counter is not consulted.
//!
//! That is only reasonable because re-installing is cheap and cannot be wrong:
//! it is one thread teardown and one `SetWindowsHookExW`, at a moment where the
//! machine is already restoring every device it owns, and a hook that did
//! survive is simply put back the way it was.

use std::time::Duration;

/// What is known about one hook at one moment.
///
/// `installed` is what Sill believes; `keys_seen` is what actually happened.
/// The gap between those two is the failure this module exists for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// Whether anything wants the hook installed at all. A hook nobody wants
    /// is correctly absent, and re-installing it would put Sill back in the
    /// path of every keystroke on the machine to do nothing with them.
    pub wanted: bool,
    /// Whether `SetWindowsHookExW` returned a handle and the thread that owns
    /// it is still pumping.
    pub installed: bool,
    /// Every key event the hook has been handed since Sill started.
    pub keys_seen: u64,
}

/// What the last check saw, and how long ago it saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Last {
    pub keys_seen: u64,
    pub ago: Duration,
}

/// Why the question is being asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The machine came back from sleep, or the session was unlocked. Nothing
    /// was typed in between, so the counter has nothing to say.
    Woke,
    /// Somebody pressed a key Sill is registered for, so the keyboard was
    /// demonstrably in use a moment ago and the hook should have seen it.
    Typed,
}

/// What to do about a hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing wants it, so its absence is correct.
    Idle,
    /// Wanted and not there. Install it; there is nothing to take out first.
    Install,
    /// Wanted, believed installed, and not receiving keys. Take it out and put
    /// it back, which is the only way to recover a hook Windows removed: the
    /// thread is still pumping and every flag still says yes, so arming again
    /// on its own would do nothing.
    Reinstall,
    /// Nothing to do.
    Sound,
}

/// How old the last check has to be before its counter means anything.
///
/// Two check points can land on top of each other: a session unlock is
/// routinely followed by a summon within the second. Without this the second
/// one would read a counter that had had no time to move and call a healthy
/// hook dead.
pub const SETTLED: Duration = Duration::from_secs(2);

/// Whether a hook looks dead, and what to do if it does.
///
/// Pure, and that is deliberate. The failure this decides about only happens on
/// a real machine coming back from real sleep, which no test can arrange, so
/// the decision is separated from everything that needs Windows and is the part
/// that gets tested.
pub fn judge(now: Reading, last: Option<Last>, cause: Cause) -> Verdict {
    if !now.wanted {
        return Verdict::Idle;
    }

    if !now.installed {
        return Verdict::Install;
    }

    match cause {
        // See the module note. On waking, a healthy hook and a removed one
        // read identically, so the only safe answer is to put it back.
        Cause::Woke => Verdict::Reinstall,
        Cause::Typed => match last {
            // A count that has not moved, since a moment long enough ago that
            // it had every chance to, while somebody was demonstrably typing.
            //
            // Strictly equal rather than "no greater": a counter that went
            // backwards has been reset by something, and a reset is not
            // evidence of a dead hook.
            Some(last) if last.ago >= SETTLED && now.keys_seen == last.keys_seen => {
                Verdict::Reinstall
            }
            _ => Verdict::Sound,
        },
    }
}

#[cfg(windows)]
pub use windows_impl::{check, Watch};

#[cfg(windows)]
mod windows_impl {
    use super::{judge, Cause, Last, Reading, Verdict};
    use std::sync::Mutex;
    use std::time::Instant;
    use tauri::{AppHandle, Manager};

    /// What each hook looked like the last time anybody asked.
    ///
    /// Managed state rather than a static, which is what rule 2 refuses, and
    /// there is nothing here a hook callback needs to reach: the callbacks only
    /// ever increment their own counter.
    #[derive(Default)]
    pub struct Watch {
        snippets: Mutex<Option<(u64, Instant)>>,
        dictation: Mutex<Option<(u64, Instant)>>,
    }

    impl Watch {
        /// Reads what was seen last and records what is seen now, in one step.
        ///
        /// One step because the two must not be able to drift: a caller that
        /// read without writing would compare the same stale count forever, and
        /// one that wrote without reading would never notice anything.
        fn turn(slot: &Mutex<Option<(u64, Instant)>>, keys_seen: u64) -> Option<Last> {
            let mut held = slot.lock().unwrap_or_else(|e| e.into_inner());

            let previous = held.map(|(seen, at)| Last {
                keys_seen: seen,
                ago: at.elapsed(),
            });

            *held = Some((keys_seen, Instant::now()));
            previous
        }
    }

    /// The one snippet-hook trouble, named once so the report and the
    /// withdrawal cannot disagree about which failure they mean.
    const SNIPPET_HOOK_TROUBLE: &str = "snippet-hook";
    /// The same for dictation's.
    const DICTATION_HOOK_TROUBLE: &str = "dictation-hook";

    /// Looks at both hooks and puts back whichever is not there.
    ///
    /// Cheap enough to sit on the summon path: two atomic loads per hook and a
    /// comparison. Anything that is not cheap happens on a thread of its own,
    /// because re-installing waits for the old hook's thread to let go and this
    /// may be called from the window procedure of the launcher.
    pub fn check(app: &AppHandle, cause: Cause) {
        let Some(watch) = app.try_state::<Watch>() else {
            return;
        };

        snippet_hook(app, &watch, cause);
        dictation_hook(app, &watch, cause);
    }

    fn snippet_hook(app: &AppHandle, watch: &Watch, cause: Cause) {
        use crate::snippets::expander;

        let Some(expander) = app.try_state::<expander::Expander>() else {
            return;
        };

        let (installed, keys_seen) = expander::facts(&expander);
        let last = Watch::turn(&watch.snippets, keys_seen);

        let now = Reading {
            wanted: expander.wanted(),
            installed,
            keys_seen,
        };

        let verdict = judge(now, last, cause);
        if matches!(verdict, Verdict::Idle | Verdict::Sound) {
            return;
        }

        crate::say!("snippet hook {verdict:?} after {cause:?}, keys seen {keys_seen}");

        let app = app.clone();
        let expander = expander.inner().clone();

        // Off this thread: `Reinstall` waits for the old hook's thread to
        // finish, and the two callers are the launcher's window procedure and
        // the summon key, neither of which may sit and wait for anything.
        std::thread::spawn(move || {
            let back = match verdict {
                Verdict::Reinstall => expander::rearm(&expander),
                // `watch` rather than `arm`, because it also publishes the
                // app handle the callback needs, which was never set if
                // nothing wanted the hook when Sill started.
                _ => {
                    expander::watch(&app, &expander);
                    expander::settled(&expander)
                }
            };

            if back {
                crate::status::resolved(&app, SNIPPET_HOOK_TROUBLE);
                return;
            }

            crate::status::report(
                &app,
                SNIPPET_HOOK_TROUBLE,
                "Windows will not let Sill watch the keyboard, so snippet keywords, the \
                 hyper key and the double-tap shortcut do nothing. Restarting Sill usually \
                 fixes it.",
                Some("snippets"),
            );
        });
    }

    fn dictation_hook(app: &AppHandle, watch: &Watch, cause: Cause) {
        use crate::dictation::hotkey::HotkeyListener;
        use crate::dictation::service::DictationService;

        let Some(service) = app.try_state::<DictationService>() else {
            return;
        };

        let settings = service.settings();
        let facts = HotkeyListener::state();
        let last = Watch::turn(&watch.dictation, facts.keys_seen);

        let now = Reading {
            wanted: settings.enabled,
            installed: facts.installed,
            keys_seen: facts.keys_seen,
        };

        let verdict = judge(now, last, cause);
        if matches!(verdict, Verdict::Idle | Verdict::Sound) {
            return;
        }

        crate::say!(
            "dictation hook {verdict:?} after {cause:?}, keys seen {}",
            facts.keys_seen
        );

        let app = app.clone();

        std::thread::spawn(move || {
            /*
             * The one place that arms this hook, rather than a second route
             * beside it.
             *
             * It already tears down whatever is there before installing, so it
             * is the re-install as well as the install, and a separate path
             * here would be a second thing to keep in step with the chord, the
             * finish key and the cancel key.
             */
            crate::apply_dictation(&app, &settings);

            if HotkeyListener::state().installed {
                crate::status::resolved(&app, DICTATION_HOOK_TROUBLE);
                return;
            }

            crate::status::report(
                &app,
                DICTATION_HOOK_TROUBLE,
                "Windows will not let Sill watch the keyboard, so the dictation shortcut \
                 does nothing. Restarting Sill usually fixes it.",
                Some("dictation"),
            );
        });
    }

    #[cfg(test)]
    mod tests {
        use super::Watch;
        use std::sync::Mutex;

        /// Every check leaves behind what the next one compares against.
        ///
        /// The whole liveness idea rests on this one step. A `turn` that read
        /// without recording would hand every later check the very first count
        /// this process ever saw, so once anybody typed nothing would ever look
        /// silent again and the check would be permanently switched off while
        /// still appearing to run.
        #[test]
        fn a_check_leaves_behind_what_the_next_one_reads() {
            let slot: Mutex<Option<(u64, std::time::Instant)>> = Mutex::new(None);

            assert_eq!(
                Watch::turn(&slot, 10),
                None,
                "the first check compared against something"
            );

            let second = Watch::turn(&slot, 25).expect("the first check recorded nothing");
            assert_eq!(second.keys_seen, 10);

            let third = Watch::turn(&slot, 40).expect("the second check recorded nothing");
            assert_eq!(
                third.keys_seen, 25,
                "a check read a count older than the one before it, so the comparison is \
                 against a reading that never moves"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{judge, Cause, Last, Reading, Verdict, SETTLED};
    use std::time::Duration;

    /// A reading of a hook that is installed and has seen some typing.
    fn alive(keys_seen: u64) -> Reading {
        Reading {
            wanted: true,
            installed: true,
            keys_seen,
        }
    }

    /// A hook nobody wants is correctly absent.
    ///
    /// It matters that this comes first. Re-installing a hook nothing asked for
    /// would put Sill back in the path of every keystroke on the machine in
    /// order to do nothing with them, which is the exact cost rule 23 refuses,
    /// and switching snippets off would stop meaning anything.
    #[test]
    fn a_hook_nobody_wants_is_left_alone() {
        let unwanted = Reading {
            wanted: false,
            installed: false,
            keys_seen: 0,
        };

        assert_eq!(judge(unwanted, None, Cause::Woke), Verdict::Idle);
        assert_eq!(judge(unwanted, None, Cause::Typed), Verdict::Idle);
    }

    /// A hook that is wanted and simply is not there gets installed.
    #[test]
    fn a_wanted_hook_that_is_missing_is_installed() {
        let missing = Reading {
            wanted: true,
            installed: false,
            keys_seen: 0,
        };

        assert_eq!(judge(missing, None, Cause::Typed), Verdict::Install);
    }

    /// Waking puts the hook back whatever the counter says.
    ///
    /// This is the case the whole item is about, and the reason it ignores the
    /// counter is worth stating where it can be read: nothing is typed while a
    /// machine is asleep, so a hook Windows removed and a hook that survived
    /// both come back with an unchanged count. There is no reading that tells
    /// them apart, and the alternative to re-installing is finding out when the
    /// user's next keyword silently does nothing.
    #[test]
    fn waking_puts_the_hook_back_even_though_it_looks_fine() {
        let seen = Some(Last {
            keys_seen: 900,
            ago: Duration::from_secs(3600),
        });

        assert_eq!(judge(alive(900), seen, Cause::Woke), Verdict::Reinstall);
    }

    /// A key was pressed, time has passed, and the hook counted nothing.
    ///
    /// The summon key reaches this process, so somebody definitely typed. A
    /// low-level hook sees every keystroke on the machine, so it must have been
    /// handed that one. A count that did not move is the signature of a hook
    /// Windows has taken away without saying so.
    #[test]
    fn a_hook_that_missed_a_keystroke_somebody_definitely_made_is_dead() {
        let seen = Some(Last {
            keys_seen: 4_200,
            ago: SETTLED,
        });

        assert_eq!(judge(alive(4_200), seen, Cause::Typed), Verdict::Reinstall);
    }

    /// A count that moved is a hook doing its job.
    #[test]
    fn a_hook_that_is_counting_keys_is_left_alone() {
        let seen = Some(Last {
            keys_seen: 4_200,
            ago: Duration::from_secs(60),
        });

        assert_eq!(judge(alive(4_201), seen, Cause::Typed), Verdict::Sound);
    }

    /// Two check points on top of each other prove nothing.
    ///
    /// A session unlock is routinely followed by a summon inside the same
    /// second, and holding the summon key repeats it. Without the settling
    /// period the second check would read a counter that had had no chance to
    /// move and tear down a hook that was working.
    #[test]
    fn a_check_a_moment_after_the_last_one_does_not_condemn_anything() {
        let just_now = Some(Last {
            keys_seen: 4_200,
            ago: SETTLED - Duration::from_millis(1),
        });

        assert_eq!(judge(alive(4_200), just_now, Cause::Typed), Verdict::Sound);
    }

    /// The first check of a session has nothing to compare against.
    #[test]
    fn the_first_check_never_condemns_a_hook() {
        assert_eq!(judge(alive(0), None, Cause::Typed), Verdict::Sound);
    }

    /// A counter that went backwards was reset, not starved.
    ///
    /// Settings has a button that clears the dictation hook's counters. A
    /// reading taken after it must not read as silence, or pressing it would
    /// tear the hook down.
    #[test]
    fn a_counter_that_was_reset_is_not_a_dead_hook() {
        let before_the_reset = Some(Last {
            keys_seen: 9_000,
            ago: Duration::from_secs(60),
        });

        assert_eq!(
            judge(alive(3), before_the_reset, Cause::Typed),
            Verdict::Sound
        );
    }
}
