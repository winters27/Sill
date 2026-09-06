//! What Sill leaves on a machine, and what removing it costs.
//!
//! ## Why this is a list rather than a paragraph in the uninstaller
//!
//! An uninstaller's idea of what a program wrote is a second copy of a fact
//! the program already knows, and this codebase has been bitten four times by
//! two lists that had to agree with nothing making them agree. The
//! consequence here is worse than usual in both directions: a list that has
//! gone stale leaves somebody's clipboard history and their sealed keys on a
//! machine they thought they had cleaned, or it deletes a folder Sill never
//! owned.
//!
//! So the list lives here, in one place, and `verify:source` holds the NSIS
//! script against it. Adding somewhere Sill writes means adding a line here,
//! and the installer stops passing until it names it too.
//!
//! ## What is deliberately not here
//!
//! **Everything Sill only reads.** The Start Menu, the uninstall hives, App
//! Paths, browser profiles, Steam's library and the terminal's settings are
//! all read and never written, so none of them is Sill's to tidy up.
//!
//! **The install directory.** The installer already owns that.

/// One thing Sill put on the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leaving {
    /// Where it is, written the way the installer has to write it.
    ///
    /// NSIS constants rather than resolved paths, because the uninstaller runs
    /// as whoever is uninstalling and a path resolved on this machine would be
    /// wrong on theirs.
    pub where_it_is: &'static str,
    /// What it is, in the words somebody uninstalling would use.
    pub what_it_is: &'static str,
    /// Whether it holds something a person would miss.
    ///
    /// The difference between "remove this quietly" and "ask first". A cache
    /// is Sill's own business; a clipboard history is not.
    pub theirs: bool,
}

/// Everything Sill writes outside the folder it is installed into.
///
/// Ordered so the uninstaller reads it top to bottom: the registry entry that
/// would otherwise keep starting a program that is gone, then the data.
pub const LEAVINGS: &[Leaving] = &[
    Leaving {
        /*
         * The firewall rule that lets extensions hear the network.
         *
         * Windows Firewall rules are per program. An extension that listens,
         * LocalSend waiting for the devices on the network to answer its
         * call, hears nothing unless the program doing the listening has an
         * inbound rule, and the program is Sill's bundled Node runtime. The
         * installer adds one rule for that one file, private and domain
         * networks only, and the host already refuses a listening socket to
         * any extension not granted the network, so the rule opens nothing
         * the person did not grant. Removed on uninstall, because a rule for
         * a program that is gone is a rule nobody asked to keep.
         */
        where_it_is: r"Windows Firewall\Sill extension runtime",
        what_it_is: "the firewall rule that lets extensions hear the network",
        theirs: false,
    },
    Leaving {
        // The autostart plugin writes this, and it is the one that matters
        // most: left behind, Windows keeps trying to start a program that is
        // not there any more, and the only sign is a delay at login.
        where_it_is: r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Sill",
        what_it_is: "the entry that starts Sill when you sign in",
        theirs: false,
    },
    Leaving {
        // Preferences, the clipboard history, the file index, saved
        // arrangements, quicklinks, snippets, installed extensions and the
        // dictation models. On the machine this was written on, the models
        // alone are the largest thing in it.
        where_it_is: r"$APPDATA\app.winters.sill",
        what_it_is:
            "your settings, clipboard history, snippets, quicklinks and installed extensions",
        theirs: true,
    },
    Leaving {
        /*
         * The Task Scheduler folder automations and timers are registered in.
         *
         * This is the one leaving that is not a file and not a registry value
         * Sill wrote for itself: it is work Windows is holding **on Sill's
         * behalf**, and it outlives the process, the reboot and the uninstall.
         * A one-off timer deletes itself once it has fired, so those do not
         * pile up, but a pending one and every daily trigger survive, and what
         * they run is `sill.exe`, which by then is not there.
         *
         * Removed without asking, because a scheduled task that starts a
         * program nobody has any more is not something anybody would choose to
         * keep. What it would do instead is fail at three in the morning,
         * forever, in a log nobody reads.
         */
        where_it_is: r"Task Scheduler\Sill",
        what_it_is: "the automations and reminders Sill asked Windows to run",
        theirs: false,
    },
    Leaving {
        // Tauri's WebView2 user data folder, which is Chromium's profile for
        // the windows Sill draws. Nothing of the person's is in it that is not
        // already in the folder above, so it goes without asking.
        where_it_is: r"$LOCALAPPDATA\app.winters.sill",
        what_it_is: "the browser engine's cache for Sill's own windows",
        theirs: false,
    },
];

/// What the uninstaller may remove without asking.
pub fn removed_quietly() -> impl Iterator<Item = &'static Leaving> {
    LEAVINGS.iter().filter(|one| !one.theirs)
}

/// What it has to ask about first.
pub fn worth_asking_about() -> impl Iterator<Item = &'static Leaving> {
    LEAVINGS.iter().filter(|one| one.theirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The autostart entry is the one that outlives the program visibly.
    ///
    /// Left behind, Windows keeps trying to start something that is gone at
    /// every sign-in, and nothing on screen says why the machine is slower.
    #[test]
    fn the_entry_that_starts_sill_is_always_removed() {
        let run = LEAVINGS
            .iter()
            .find(|one| one.where_it_is.contains("CurrentVersion\\Run"))
            .expect("the autostart entry is not listed at all");

        assert!(
            !run.theirs,
            "the autostart entry would be left behind unless somebody said yes"
        );
    }

    /// Anything holding a person's own work is asked about, never assumed.
    ///
    /// Somebody reinstalling to fix something does not expect their clipboard
    /// history and their snippets to go with it, and an uninstaller that took
    /// them silently would be right once and wrong every other time.
    #[test]
    fn the_folder_with_their_work_in_it_is_asked_about() {
        let data = LEAVINGS
            .iter()
            .find(|one| one.where_it_is.contains("$APPDATA"))
            .expect("the data folder is not listed at all");

        assert!(data.theirs, "somebody's settings would be deleted silently");
    }

    /// Every path is written in a form the uninstaller can use.
    ///
    /// A path resolved on a developer's machine is wrong on everybody else's,
    /// and NSIS would take it literally rather than complain.
    #[test]
    fn nothing_names_one_particular_machine() {
        for one in LEAVINGS {
            assert!(
                one.where_it_is.starts_with('$')
                    || one.where_it_is.starts_with("HKCU\\")
                    || one.where_it_is.starts_with("Task Scheduler\\")
                    // A firewall rule, which the installer names rather than
                    // paths to, the same way it names a scheduled task.
                    || one.where_it_is.starts_with("Windows Firewall\\"),
                "{} is not a constant the uninstaller can resolve",
                one.where_it_is
            );
            assert!(
                !one.where_it_is.contains("C:\\") && !one.where_it_is.contains("Brandon"),
                "{} names one particular machine",
                one.where_it_is
            );
        }
    }

    /// Every entry says something a person could act on.
    #[test]
    fn each_one_is_described_in_words_somebody_would_use() {
        for one in LEAVINGS {
            assert!(!one.what_it_is.trim().is_empty(), "{one:?} says nothing");
            assert!(
                one.what_it_is
                    .starts_with(|c: char| c.is_lowercase() || c == '$'),
                "{} reads as a heading rather than as part of a sentence",
                one.what_it_is
            );
        }
    }

    /// The two halves cover the list between them and do not overlap.
    #[test]
    fn everything_is_either_asked_about_or_not() {
        assert_eq!(
            removed_quietly().count() + worth_asking_about().count(),
            LEAVINGS.len()
        );
    }

    /// The scheduled tasks go, and nobody is asked about them.
    ///
    /// They are the one leaving that is work Windows holds on Sill's behalf
    /// rather than a file Sill wrote, and they outlive the uninstall. What a
    /// kept one would do is start a program that is not there any more, at
    /// three in the morning, forever.
    #[test]
    fn the_scheduled_tasks_go_without_asking() {
        let tasks = LEAVINGS
            .iter()
            .find(|one| one.where_it_is.contains("Task Scheduler"))
            .expect("the scheduled tasks are not listed at all");

        assert!(!tasks.theirs, "somebody would be asked to keep a dead task");
        assert!(
            tasks.where_it_is.ends_with(crate::automation::FOLDER),
            "the leaving names a different folder from the one tasks are written to"
        );
    }
}
