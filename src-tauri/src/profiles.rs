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
    /**
    A named position, when the arrangement was made resolution independent.

    A captured rectangle says "1213 pixels wide", which is a fact about the
    display it was measured on. Rescaling carries that onto a different one
    and it lands close, but "close" is what leaves a two pixel strip of
    desktop between two windows that were flush.

    A slot says "the left half", which is true on any display, so an
    arrangement holding slots is the same arrangement on a laptop screen, a
    dock and a television. That is what [`nearest_slot`] converts a captured
    one into, and it wins over `rect` when it is set.

    `Option`, not a default slot, because an arrangement somebody captured
    deliberately holds rectangles nothing tiles: a window two thirds up the
    screen and slightly off centre is a place, and forcing it to the nearest
    named one would be Sill deciding it knew better.
    */
    #[serde(default)]
    pub slot: Option<crate::windowing::Slot>,
    /// Which display, counted from zero, when the arrangement names one.
    ///
    /// `None` means the display the window is already on, which is what a
    /// captured arrangement means on the machine it was captured on. Named
    /// only when somebody wants a window to move screens, and ignored when
    /// that display is not attached: an arrangement that puts half its
    /// windows nowhere because a monitor is unplugged is worse than one that
    /// keeps them where they are.
    #[serde(default)]
    pub display: Option<usize>,
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
                // Captured arrangements hold rectangles. `to_slots` is what
                // turns one into a set of named positions, deliberately as a
                // separate step somebody asks for.
                slot: None,
                display: None,
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

        // The display the arrangement names, when it names one and that
        // display is attached. Falling back rather than skipping: an
        // arrangement that puts half its windows nowhere because a monitor is
        // unplugged is worse than one that keeps them where they are.
        let onto = saved
            .display
            .and_then(|index| works.get(index))
            .or_else(|| works.get(window.monitor))
            .copied()
            .unwrap_or(saved.work);

        let rect = match saved.slot {
            // A named position is true on any display, so there is nothing to
            // rescale and nothing to lose in the arithmetic.
            Some(slot) => crate::windowing::slot_rect(slot, onto),
            // Rescaled into the work area the window is on now. Restoring onto
            // the same layout is the common case and rescaling is then the
            // identity, so this costs nothing when nothing has changed.
            None if onto == saved.work => saved.rect,
            None => crate::windowing::rescale(saved.rect, saved.work, onto),
        };

        out.push((window.id, rect, saved.maximized));
    }

    out
}

/// The named position a rectangle is closest to, if it is close to one.
///
/// **How much of the slot the window covers, not how near its corners are.**
/// A window three pixels narrower than the left half and a window filling the
/// top left quarter have corners about equally far from "left half"; only one
/// of them is what somebody meant by it. Overlap answers that directly.
///
/// `None` below the threshold, which is the whole point. An arrangement
/// somebody captured deliberately holds rectangles nothing tiles, and a
/// window two thirds up the screen and slightly off centre is a place rather
/// than a bad attempt at one. Converting it anyway would be Sill deciding it
/// knew better than the arrangement.
///
/// Pure, so every awkward case is testable without a desktop.
pub fn nearest_slot(
    rect: crate::windowing::Rect,
    work: crate::windowing::Rect,
) -> Option<crate::windowing::Slot> {
    /// How much of the slot and of the window must be shared before a
    /// rectangle counts as being in that slot, as hundredths.
    ///
    /// Both directions matter. Covering the slot is not enough on its own,
    /// because a full screen window covers the left half completely; being
    /// covered by it is not enough either, because a tiny window in the corner
    /// sits entirely inside it.
    const ENOUGH: i64 = 90;

    let area = |r: crate::windowing::Rect| (r.width as i64) * (r.height as i64);

    if area(rect) <= 0 || area(work) <= 0 {
        return None;
    }

    let mut best: Option<(i64, crate::windowing::Slot)> = None;

    for slot in crate::windowing::Slot::ALL {
        let candidate = crate::windowing::slot_rect(slot, work);
        let shared = rect.overlap(&candidate);

        if shared * 100 < area(candidate) * ENOUGH || shared * 100 < area(rect) * ENOUGH {
            continue;
        }

        // Ties go to the slot listed first, which is the order a person reads
        // them in: halves, then quarters, then thirds. `Fill` is last, so a
        // window that is genuinely both never quietly becomes the vaguer one.
        if best.is_none_or(|(most, _)| shared > most) {
            best = Some((shared, slot));
        }
    }

    best.map(|(_, slot)| slot)
}

/// Rewrites an arrangement as named positions, where they fit.
///
/// The point of the conversion: an arrangement captured on this desk becomes
/// one that means the same thing on a laptop screen. A window that fits no
/// slot keeps its rectangle, so the arrangement is never made worse by being
/// converted, only more portable where it can be.
///
/// The display each window is on is recorded at the same time, because a
/// two screen arrangement whose windows all say "left half" and nothing else
/// would pile every one of them onto the same screen.
pub fn to_slots(profile: &Profile, works: &[crate::windowing::Rect]) -> Profile {
    Profile {
        name: profile.name.clone(),
        windows: profile
            .windows
            .iter()
            .map(|saved| {
                let display = works.iter().position(|work| *work == saved.work);

                Placed {
                    slot: nearest_slot(saved.rect, saved.work),
                    display,
                    ..saved.clone()
                }
            })
            .collect(),
    }
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
            slot: None,
            display: None,
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
                slot: None,
                display: None,
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

    /// The conversion that makes an arrangement portable.
    #[test]
    fn a_captured_half_becomes_the_slot_that_means_the_same_thing() {
        let left = Rect {
            x: 0,
            y: 0,
            width: 960,
            height: 1040,
        };

        assert_eq!(
            nearest_slot(left, work()),
            Some(crate::windowing::Slot::Left)
        );
    }

    /// The reason it is overlap and not corner distance.
    ///
    /// A window filling the top left quarter has corners about as far from
    /// "left half" as a window three pixels narrower than it does, and only
    /// one of those is what somebody meant by the left half.
    #[test]
    fn a_quarter_is_its_own_quarter_rather_than_a_poor_half() {
        let top_left = Rect {
            x: 0,
            y: 0,
            width: 960,
            height: 520,
        };

        assert_eq!(
            nearest_slot(top_left, work()),
            Some(crate::windowing::Slot::TopLeft)
        );
    }

    /// A place nothing tiles keeps its rectangle.
    ///
    /// The whole reason this answers `None`. An arrangement somebody made by
    /// hand holds positions that are not any named one, and converting them
    /// anyway would be Sill deciding it knew better than the arrangement.
    #[test]
    fn a_window_that_fits_no_slot_is_left_as_a_rectangle() {
        let awkward = Rect {
            x: 300,
            y: 120,
            width: 700,
            height: 500,
        };

        assert_eq!(nearest_slot(awkward, work()), None);
    }

    /// A size somebody chose between two named ones is neither of them.
    ///
    /// This is the case the second half of the threshold exists for, and the
    /// only one that distinguishes it. A window 1500 wide on a 1920 desktop
    /// covers the first two thirds completely, so asking only "is the slot
    /// covered" answers "the first two thirds". It is 78% of the width, which
    /// is not that slot and not the left half either: it is a width the
    /// person dragged to, and the arrangement should keep it.
    #[test]
    fn a_width_between_two_named_ones_is_neither_of_them() {
        let between = Rect {
            x: 0,
            y: 0,
            width: 1500,
            height: 1040,
        };

        assert_eq!(nearest_slot(between, work()), None);
    }

    /// A window covering the whole screen is not "the left half".
    ///
    /// It covers the left half completely, so a test that only asked whether
    /// the slot was covered would say it was.
    #[test]
    fn a_full_screen_window_is_fill_rather_than_every_slot_it_covers() {
        assert_eq!(
            nearest_slot(work(), work()),
            Some(crate::windowing::Slot::Fill)
        );
    }

    /// And a tiny window in the corner is not the quarter it sits inside.
    #[test]
    fn a_small_window_is_not_the_slot_it_happens_to_sit_within() {
        let small = Rect {
            x: 10,
            y: 10,
            width: 200,
            height: 150,
        };

        assert_eq!(nearest_slot(small, work()), None);
    }

    /// A converted arrangement puts a window on a slot rather than a
    /// rectangle, and lands exactly on a display of a different size.
    #[test]
    fn a_converted_arrangement_is_exact_on_a_display_it_was_never_captured_on() {
        let captured = Profile {
            name: "Desk".into(),
            windows: vec![Placed {
                app: "Code".into(),
                title: "sill".into(),
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 960,
                    height: 1040,
                },
                work: work(),
                maximized: false,
                path: String::new(),
                slot: None,
                display: None,
            }],
        };

        let portable = to_slots(&captured, &[work()]);
        assert_eq!(portable.windows[0].slot, Some(crate::windowing::Slot::Left));
        assert_eq!(portable.windows[0].display, Some(0));

        // A laptop screen, which is not a scaled copy of the desk.
        let laptop = Rect {
            x: 0,
            y: 0,
            width: 1512,
            height: 903,
        };

        let open = vec![crate::windowing::Window {
            id: 1,
            title: "sill".into(),
            app: "Code".into(),
            app_path: String::new(),
            pid: 1,
            minimized: false,
            maximized: false,
            rect: laptop,
            monitor: 0,
        }];

        let planned = plan(&portable, &open, &[laptop]);
        assert_eq!(planned.len(), 1);

        // Exactly half, with no strip of desktop left down the middle. The
        // rescaled rectangle would have been close and not this.
        assert_eq!(planned[0].1.width, 756);
        assert_eq!(planned[0].1.height, 903);
        assert_eq!(planned[0].1.x, 0);
    }

    /// An arrangement naming a display it cannot find keeps the window where
    /// it is rather than putting it nowhere.
    #[test]
    fn a_display_that_is_unplugged_leaves_the_window_where_it_is() {
        let profile = Profile {
            name: "Two screens".into(),
            windows: vec![Placed {
                app: "Code".into(),
                title: "sill".into(),
                rect: Rect {
                    x: 0,
                    y: 0,
                    width: 960,
                    height: 1040,
                },
                work: work(),
                maximized: false,
                path: String::new(),
                slot: Some(crate::windowing::Slot::Left),
                // A second display that is not attached any more.
                display: Some(1),
            }],
        };

        // The one display still attached is deliberately not the one the
        // arrangement was measured on, so falling back to the saved work area
        // and falling back to the attached one give different answers.
        let attached = Rect {
            x: 0,
            y: 0,
            width: 1512,
            height: 903,
        };

        let open = vec![crate::windowing::Window {
            id: 1,
            title: "sill".into(),
            app: "Code".into(),
            app_path: String::new(),
            pid: 1,
            minimized: false,
            maximized: false,
            rect: attached,
            monitor: 0,
        }];

        let planned = plan(&profile, &open, &[attached]);
        assert_eq!(planned.len(), 1, "the window was dropped");
        assert_eq!(
            planned[0].1,
            crate::windowing::slot_rect(crate::windowing::Slot::Left, attached),
            "it landed on the display it was saved from rather than the one that is here"
        );
    }
}
