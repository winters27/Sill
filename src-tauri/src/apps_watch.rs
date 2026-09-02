//! Noticing that an application was installed or removed.
//!
//! ## Why this exists
//!
//! The command index was built at startup and never again unless somebody ran
//! "Reload Sill Index" by hand. So an application installed while Sill was
//! running was invisible, and the way to find out was to fail to find it,
//! guess that the index was stale, and know a command exists for that. That is
//! three things a person should not have to know.
//!
//! ## Why a watcher rather than a timer
//!
//! Installing something is a thing that happens rarely and then all at once.
//! A timer would either be slow enough to be useless or frequent enough to
//! walk the Start Menu for nothing, which is the sort of idle cost rule 23
//! exists to refuse. A watcher costs one handle per folder and reports only
//! when something actually changed.
//!
//! ## What it watches
//!
//! The same shortcut roots the scan already reads: both Start Menus, the
//! pinned taskbar folder, and both Desktops. Practically every Windows
//! installer writes a Start Menu shortcut, so this catches installs and
//! uninstalls alike without reading the registry.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tauri::AppHandle;

/// How long to wait for an installer to stop writing.
///
/// An install writes a folder, a shortcut and often an uninstaller entry, over
/// a second or two. Rebuilding for each of those would be three scans for one
/// event.
const SETTLE: Duration = Duration::from_secs(2);

/// The least time between two rebuilds.
///
/// A scan is a PowerShell round trip and thousands of filesystem calls, so it
/// is not something to do on a hair trigger. Short enough that an application
/// is findable within a few seconds of finishing its install, which is the
/// point of the whole thing.
const QUIET: Duration = Duration::from_secs(15);

/// Files that can add or remove a command.
///
/// A shortcut, an internet shortcut, and an executable dropped straight onto
/// the Desktop. Anything else under these folders is an installer's working
/// file and does not change what Sill can launch.
const LAUNCHABLE: &[&str] = &["lnk", "url", "exe"];

/// Watches for applications appearing and disappearing.
///
/// Held as managed state so it lives as long as the application does. Dropping
/// it stops the watching, which is what should happen.
pub(crate) struct AppWatcher {
    // Behind a mutex because managed state is shared across threads and a
    // watcher is only `Send`. Nothing ever locks it: the field exists to keep
    // the watcher alive.
    _watcher: std::sync::Mutex<Box<dyn notify::Watcher + Send>>,
}

impl AppWatcher {
    /// Starts watching, and reindexes once things settle.
    ///
    /// Failing to watch is not fatal. The index is still built at startup and
    /// can still be rebuilt by hand; all that is lost is noticing on its own.
    pub(crate) fn start(app: AppHandle, roots: Vec<PathBuf>) -> Option<Self> {
        use notify::{RecursiveMode, Watcher};

        if roots.is_empty() {
            return None;
        }

        let (tx, rx) = std::sync::mpsc::channel();

        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };

                // Writing to a file cannot add or remove a command. The same
                // rule the file index uses, and for the same reason: writes
                // are nearly everything a watcher reports.
                if !crate::catalog::changes_the_index(&event.kind) {
                    return;
                }

                if !event.paths.iter().any(|path| worth_reindexing(path)) {
                    return;
                }

                let _ = tx.send(());
            })
            .ok()?;

        let mut watching = 0;
        for root in &roots {
            match watcher.watch(root, RecursiveMode::Recursive) {
                Ok(()) => watching += 1,
                // A missing folder is ordinary: not every machine has a
                // public Desktop or a pinned taskbar folder.
                Err(err) => crate::say!("apps: cannot watch {}: {err}", root.display()),
            }
        }

        if watching == 0 {
            return None;
        }

        // Written to the log rather than only to a console nobody sees in a
        // release build. "Sill did not notice my new application" is a
        // complaint that has to be answerable afterwards.
        crate::say!("watching {watching} folders for installed applications");

        std::thread::spawn(move || {
            // Starts as if a rebuild just happened, because one did: startup
            // scans once. Without this the first event to arrive would scan
            // again immediately, and an ordinary launch would scan twice.
            let mut last = Instant::now();

            while rx.recv().is_ok() {
                // Drain whatever else the installer is still writing.
                while rx.recv_timeout(SETTLE).is_ok() {}

                // Waited out rather than dropped. Skipping the rebuild inside
                // the quiet period would throw the change away with it, and
                // the application installed in the first fifteen seconds after
                // a launch would stay invisible until something else changed.
                if let Some(wait) = QUIET.checked_sub(last.elapsed()) {
                    std::thread::sleep(wait);
                    while rx.try_recv().is_ok() {}
                }

                last = Instant::now();
                crate::say!("applications changed, reindexing");
                crate::reload_index(&app);
            }
        });

        Some(Self {
            _watcher: std::sync::Mutex::new(Box::new(watcher)),
        })
    }
}

/// Whether this path appearing or going away changes what Sill can launch.
///
/// Its own function so the rule can be tested without a filesystem, a watcher
/// or a two second wait. A directory has no extension and always counts: a
/// whole program group appearing is exactly the event worth reacting to, and
/// the events for the shortcuts inside it may arrive before the watch reaches
/// that far down.
pub(crate) fn worth_reindexing(path: &Path) -> bool {
    match path.extension() {
        Some(extension) => {
            let extension = extension.to_string_lossy().to_ascii_lowercase();
            LAUNCHABLE.contains(&extension.as_str())
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::worth_reindexing;
    use std::path::Path;

    #[test]
    fn a_shortcut_appearing_is_worth_a_scan() {
        for name in [
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Figma.lnk",
            r"C:\Users\x\Desktop\Steam.url",
            r"C:\Users\x\Desktop\portable-thing.exe",
            // Capitalisation is whatever the installer felt like.
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Thing.LNK",
        ] {
            assert!(
                worth_reindexing(Path::new(name)),
                "{name} does not count as a change"
            );
        }
    }

    #[test]
    fn a_new_program_group_is_worth_a_scan() {
        // No extension, so it is a folder: a whole group of shortcuts about to
        // appear, and the events for what is inside it may not reach a watch
        // that has not descended there yet.
        assert!(worth_reindexing(Path::new(
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Some Company"
        )));
    }

    /// The half that matters, because a scan is expensive.
    ///
    /// Installers leave temporary files, logs and icon caches under these
    /// folders, and every one of them would otherwise cost a PowerShell round
    /// trip and a walk of the Start Menu.
    #[test]
    fn an_installers_working_files_are_not() {
        for name in [
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\desktop.ini",
            r"C:\Users\x\Desktop\notes.txt",
            r"C:\Users\x\Desktop\install.log",
            r"C:\Users\x\Desktop\screenshot.png",
            r"C:\Users\x\Desktop\thing.tmp",
        ] {
            assert!(
                !worth_reindexing(Path::new(name)),
                "{name} would trigger a scan"
            );
        }
    }
}
