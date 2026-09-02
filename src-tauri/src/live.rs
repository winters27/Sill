//! Rows whose subtitle changes while somebody is looking at them.
//!
//! ## What makes a row live
//!
//! Nearly every row in the launcher says the same thing every time it is
//! drawn, because what it describes does not change: a snippet is its text and
//! an application is its name. A few describe something that is different a
//! second later, and for those the subtitle is the answer rather than a
//! description of where the answer lives. "The clock, the weather, and what
//! this machine is doing" is a sentence about a screen; "CPU 12%, RAM 41%" is
//! the thing somebody opened the launcher to find out.
//!
//! ## Measured only while it can be seen, and that is enforced here
//!
//! The window asks for these on a timer while it is open, and the honest way
//! to write that would be for the window to stop its own timer when it hides.
//! It is not written that way. **This refuses to measure when the launcher is
//! not visible**, and answers with nothing, which the window takes as its
//! signal to stop asking.
//!
//! The difference matters because there are several ways the launcher can go
//! away and only one of them is the window deciding to. A timer that outlived
//! a dismissal would be a launcher that costs a reading a second forever,
//! which is the exact thing this project claims it does not do, and it would
//! be invisible: nothing on screen, no error, just a number in Task Manager.
//! Put here, being wrong is impossible rather than unlikely.

use serde::Serialize;
use tauri::Manager;

/// One row, and what it should say now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    /// The record's id, so the window can find the row it belongs to.
    pub id: String,
    pub subtitle: String,
}

/// Rounded to whole percents, because a subtitle that changes in the first
/// decimal every tick reads as noise rather than as a measurement.
fn percent(part: u64, whole: u64) -> u32 {
    if whole == 0 {
        return 0;
    }

    ((part as f64 / whole as f64) * 100.0).round() as u32
}

/// What the machine row says right now.
pub fn machine(reading: &crate::meter::Reading) -> String {
    format!(
        "CPU {}%  ·  RAM {}%  ·  Sill {} MB",
        reading.cpu.round() as u32,
        percent(reading.memory_used, reading.memory_total),
        reading.sill / (1024 * 1024),
    )
}

/// The id of the row the machine reading belongs to.
///
/// Taken from the registry rather than written twice: this and the builtin
/// have to name the same row, and a subtitle that updates a row nobody can
/// find is the failure nothing would report.
pub fn machine_row() -> String {
    crate::registry::builtin_id("widgets")
}

/// Whether anybody can see the launcher.
///
/// The one question this module exists to ask. A hidden window is not
/// measured for, and asking Windows costs less than the reading would.
fn on_screen(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

/// Every live row, or nothing at all when the launcher cannot be seen.
///
/// Nothing is the window's signal to stop asking, so a launcher that was
/// dismissed by any route, the hotkey, a click elsewhere, an action that put
/// it away, stops costing anything within one tick without either side having
/// to know how it was dismissed.
pub fn rows(app: &tauri::AppHandle) -> Vec<Live> {
    if !on_screen(app) {
        // The samples are worthless now anyway: a rate worked out against a
        // reading from before somebody went to lunch is not "right now".
        app.state::<crate::meter::Meter>().forget();
        return Vec::new();
    }

    let reading = app.state::<crate::meter::Meter>().read();

    vec![Live {
        id: machine_row(),
        subtitle: machine(&reading),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meter::Reading;

    fn reading(cpu: f32, used: u64, total: u64, sill: u64) -> Reading {
        Reading {
            cpu,
            memory_used: used,
            memory_total: total,
            sill,
            ..Default::default()
        }
    }

    #[test]
    fn it_says_the_three_numbers_somebody_opened_it_for() {
        let said = machine(&reading(
            12.4,
            8 * 1024 * 1024 * 1024,
            16 * 1024 * 1024 * 1024,
            827 * 1024 * 1024,
        ));

        assert!(said.contains("CPU 12%"), "{said}");
        assert!(said.contains("RAM 50%"), "{said}");
        assert!(said.contains("Sill 827 MB"), "{said}");
    }

    /// A machine reporting no memory at all is a reading that failed, and
    /// dividing by it would put "RAM NaN%" in front of somebody.
    #[test]
    fn no_total_is_zero_rather_than_a_division() {
        assert_eq!(percent(1, 0), 0);
        assert!(machine(&reading(0.0, 0, 0, 0)).contains("RAM 0%"));
    }

    /// Rounded, because a subtitle whose last digit changes every tick reads
    /// as noise rather than as a measurement.
    #[test]
    fn it_is_rounded_rather_than_precise() {
        assert!(machine(&reading(12.4, 0, 0, 0)).contains("CPU 12%"));
        assert!(machine(&reading(12.6, 0, 0, 0)).contains("CPU 13%"));
    }

    /// The subtitle has to land on the row the builtin actually is.
    #[test]
    fn the_row_it_updates_is_one_the_registry_has() {
        let id = machine_row();

        assert!(
            crate::registry::builtins()
                .iter()
                .any(|record| record.id == id),
            "the machine row updates {id}, which is not in the registry",
        );
    }
}
