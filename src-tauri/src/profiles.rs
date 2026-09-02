//! Window arrangements, saved and put back.
//!
//! P2.12. A profile is a set of windows and where each one sat: the editor
//! left, the browser right, the terminal in the corner. Arranging that by hand
//! takes a minute every morning, and a launcher is exactly the thing that
//! should be able to do it in a keystroke.
//!
//! ## What a profile stores, and what it deliberately does not
//!
//! **Not window handles.** A handle means nothing once the window closes, and
//! Windows reuses them, so a profile keyed by handle would move a stranger's
//! window a day later. A saved window is named by the program it belongs to
//! and, where there is more than one, by its title.
//!
//! **Not what to launch.** Restoring arranges what is open; it opens nothing.
//! A profile that starts four programs because one was closed is a profile
//! nobody dares run, and "arrange what I have" is the thing wanted several
//! times a day.
//!
//! ## Monitors change
//!
//! A profile saved on two displays and restored on one has to go somewhere.
//! Every rectangle is stored against the work area it was measured in, so it
//! can be rescaled into whatever is attached now rather than dropped off the
//! edge of a screen that is not.

use serde::{Deserialize, Serialize};

use crate::windowing::{Rect, Window};

/// One window's place in an arrangement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    /// The program, as the window list names it.
    pub app: String,
    /// The window's title when it was saved.
    ///
    /// Used to tell four browser windows apart, and only as a preference: a
    /// title changes with the page, so a profile that insisted on it would
    /// stop matching the moment somebody navigated.
    pub title: String,
    pub rect: Rect,
    /// The work area the rectangle was measured in.
    ///
    /// What makes restoring onto a different display sane.
    pub work: Rect,
    pub maximized: bool,
    /// The executable behind the window, so a closed one can be opened again.
    ///
    /// `#[serde(default)]` because arrangements saved before this existed have
    /// no path in the file, and they must keep working: an arrangement that
    /// stopped loading because a field was added would be somebody's saved
    /// desk lost to an upgrade.
    #[serde(default)]
    pub path: String,
}

/// A named arrangement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub windows: Vec<Placed>,
}

/// Takes the arrangement as it stands.
///
/// Minimised windows are left out. Where a minimised window would be if it
/// were restored is not something anybody arranged, and saving it means that
/// restoring later drags a window out of the taskbar that nobody asked for.
pub fn capture(name: &str, open: &[Window], works: &[Rect]) -> Profile {
    Profile {
        name: name.trim().to_string(),
        windows: open
            .iter()
            .filter(|window| !window.minimized)
            .map(|window| Placed {
                app: window.app.clone(),
                title: window.title.clone(),
                rect: window.rect,
                work: works.get(window.monitor).copied().unwrap_or(window.rect),
                maximized: window.maximized,
                path: window.app_path.clone(),
            })
            .collect(),
    }
}

/// What has to be started before anything can be put back.
///
/// One path per program, not one per saved window. Somebody with four browser
/// windows saved and the browser closed wants the browser started, not started
/// four times: a second launch of most programs opens another window, and a
/// second launch of the rest does nothing at all. Starting one and placing
/// whatever appears is the behaviour that is right in both cases.
///
/// A program with any window already open is left alone entirely, because the
/// person is using it and reopening it would put a new window in front of what
/// they were doing.
///
/// Pure, so the awkward cases are testable without a desktop.
pub fn missing(profile: &Profile, open: &[Window]) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();

    for placed in &profile.windows {
        if placed.path.is_empty() {
            continue;
        }

        let running = open
            .iter()
            .any(|window| window.app.eq_ignore_ascii_case(&placed.app));

        let already = wanted
            .iter()
            .any(|path| path.eq_ignore_ascii_case(&placed.path));

        if !running && !already {
            wanted.push(placed.path.clone());
        }
    }

    wanted
}

/// Which open window each saved one means, and where to put it.
///
/// Pure, so the awkward parts are testable without a desktop: two windows of
/// one program, a program that is no longer running, and a profile holding
/// more windows than are open.
///
/// **One open window is used once.** Without that, three saved Explorer
/// windows all match the same open one and it is moved three times, ending up
/// wherever the last instruction put it, which looks like two of them
/// silently failing.
pub fn plan(profile: &Profile, open: &[Window], works: &[Rect]) -> Vec<(isize, Rect, bool)> {
    let mut taken: Vec<isize> = Vec::new();
    let mut out = Vec::new();

    for saved in &profile.windows {
        let mut best: Option<&Window> = None;

        for window in open {
            if taken.contains(&window.id) || window.app != saved.app {
                continue;
            }

            // An exact title wins outright. Otherwise the first window of the
            // right program will do, which is what somebody means when they
            // have one of everything.
            if window.title == saved.title {
                best = Some(window);
                break;
            }

            if best.is_none() {
                best = Some(window);
            }
        }

        let Some(window) = best else { continue };
        taken.push(window.id);

        // Rescaled into the work area the window is on now. Restoring onto the
        // same layout is the common case and rescaling is then the identity,
        // so this costs nothing when nothing has changed.
        let now = works.get(window.monitor).copied().unwrap_or(saved.work);
        let rect = if now == saved.work {
            saved.rect
        } else {
            crate::windowing::rescale(saved.rect, saved.work, now)
        };

        out.push((window.id, rect, saved.maximized));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1040,
        }
    }

    fn window(id: isize, app: &str, title: &str) -> Window {
        Window {
            id,
            title: title.to_string(),
            app: app.to_string(),
            app_path: String::new(),
            pid: 1,
            minimized: false,
            maximized: false,
            rect: Rect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            monitor: 0,
        }
    }

    fn saved(app: &str, title: &str, x: i32) -> Placed {
        Placed {
            app: app.to_string(),
            title: title.to_string(),
            rect: Rect {
                x,
                y: 0,
                width: 800,
                height: 600,
            },
            work: work(),
            maximized: false,
            path: String::new(),
        }
    }

    #[test]
    fn a_minimized_window_is_not_part_of_an_arrangement() {
        let mut down = window(1, "Firefox", "News");
        down.minimized = true;

        let profile = capture("Morning", &[down, window(2, "Code", "sill")], &[work()]);

        assert_eq!(profile.windows.len(), 1);
        assert_eq!(profile.windows[0].app, "Code");
    }

    /// The title decides between two windows of one program.
    #[test]
    fn the_right_window_of_two_is_chosen_by_title() {
        let profile = Profile {
            name: "Work".into(),
            windows: vec![saved("Firefox", "Docs", 100)],
        };

        let open = vec![window(1, "Firefox", "News"), window(2, "Firefox", "Docs")];
        let moves = plan(&profile, &open, &[work()]);

        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].0, 2, "the window whose title matches");
    }

    /// Without this, several saved windows all move the same open one.
    #[test]
    fn one_open_window_is_used_once() {
        let profile = Profile {
            name: "Work".into(),
            windows: vec![
                saved("Explorer", "Downloads", 0),
                saved("Explorer", "Documents", 900),
            ],
        };

        // One Explorer window is open and neither title matches it.
        let open = vec![window(7, "Explorer", "Pictures")];
        let moves = plan(&profile, &open, &[work()]);

        assert_eq!(moves.len(), 1, "one window can only be in one place");
        assert_eq!(moves[0].0, 7);
    }

    #[test]
    fn a_program_that_is_not_running_is_skipped_rather_than_guessed_at() {
        let profile = Profile {
            name: "Work".into(),
            windows: vec![
                saved("Photoshop", "Untitled", 0),
                saved("Code", "sill", 100),
            ],
        };

        let moves = plan(&profile, &[window(3, "Code", "sill")], &[work()]);

        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].0, 3);
    }

    /// Saved on a wide display, restored on a narrower one.
    #[test]
    fn a_rectangle_is_rescaled_onto_the_display_that_is_there_now() {
        let profile = Profile {
            name: "Work".into(),
            // The right-hand half of a 1920 wide desktop.
            windows: vec![Placed {
                app: "Code".into(),
                title: "sill".into(),
                rect: Rect {
                    x: 960,
                    y: 0,
                    width: 960,
                    height: 1040,
                },
                work: work(),
                maximized: false,
                path: String::new(),
            }],
        };

        let narrow = Rect {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        };
        let moves = plan(&profile, &[window(1, "Code", "sill")], &[narrow]);

        let (_, rect, _) = moves[0];
        assert!(
            rect.x >= 600 && rect.x + rect.width <= 1280,
            "still the right-hand half and still on the screen: {rect:?}"
        );
    }

    /// The common case must not be disturbed by the rescaling.
    #[test]
    fn the_same_layout_puts_a_window_back_exactly() {
        let profile = Profile {
            name: "Work".into(),
            windows: vec![saved("Code", "sill", 640)],
        };

        let moves = plan(&profile, &[window(1, "Code", "sill")], &[work()]);

        assert_eq!(moves[0].1.x, 640);
        assert_eq!(moves[0].1.width, 800);
    }

    mod what_has_to_be_started {
        use super::*;

        fn with_path(app: &str, path: &str) -> Placed {
            Placed {
                path: path.to_string(),
                ..saved(app, "a window", 0)
            }
        }

        fn profile_of(windows: Vec<Placed>) -> Profile {
            Profile {
                name: "Desk".to_string(),
                windows,
            }
        }

        /// A program somebody is already using is left alone.
        ///
        /// Reopening it would put a new window in front of what they were doing,
        /// which is the opposite of restoring an arrangement.
        #[test]
        fn a_program_that_is_running_is_not_started_again() {
            let profile = profile_of(vec![with_path("Code", "C:/code.exe")]);
            let open = vec![window(1, "Code", "main.rs")];

            assert!(missing(&profile, &open).is_empty());
        }

        #[test]
        fn a_program_that_is_closed_is_started() {
            let profile = profile_of(vec![with_path("Code", "C:/code.exe")]);

            assert_eq!(missing(&profile, &[]), vec!["C:/code.exe".to_string()]);
        }

        /// Four saved browser windows and no browser means start the browser once.
        ///
        /// A second launch of most programs opens another window and of the rest
        /// does nothing, so starting one and placing whatever appears is the
        /// behaviour that is right either way. Starting four is right in neither.
        #[test]
        fn several_windows_of_one_closed_program_start_it_once() {
            let profile = profile_of(vec![
                with_path("Zen", "C:/zen.exe"),
                with_path("Zen", "C:/zen.exe"),
                with_path("Zen", "C:/zen.exe"),
            ]);

            assert_eq!(missing(&profile, &[]), vec!["C:/zen.exe".to_string()]);
        }

        /// An arrangement saved before paths were recorded still restores.
        ///
        /// Its windows have no path, so nothing can be started for them, and the
        /// ones that happen to be open are still put back. Treating a blank path
        /// as a program to run would try to start the empty string.
        #[test]
        fn an_arrangement_saved_without_paths_starts_nothing() {
            let profile = profile_of(vec![saved("Code", "main.rs", 0)]);

            assert!(missing(&profile, &[]).is_empty());
        }
    }
}
