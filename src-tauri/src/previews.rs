//! Pictures of the windows you are switching between.
//!
//! A list of window titles tells you which application, and often not which
//! window: four browser windows are four rows reading almost the same. A
//! picture answers in one glance what a title cannot answer at all.
//!
//! ## What this costs, and when
//!
//! Nothing at rest, and nothing at all unless the switcher is open. A window
//! is captured when it is the one selected, never in a batch: opening the
//! switcher on twenty windows must not photograph twenty windows.
//!
//! Each picture is made small **before** it is encoded, because encoding is
//! the expensive half and a full-size window is four million pixels nobody is
//! going to look at.
//!
//! ## Why they are kept, and why not for long
//!
//! Arrowing down a list and back up again asks for the same window twice a
//! second, and capturing it twice is work for a picture that has not changed.
//! So a handful are kept, and the whole lot is dropped when the switcher
//! closes: a preview is a picture of a moment, and the moment ends.

use std::collections::HashMap;
use std::sync::Mutex;

/// How many pictures are kept while the switcher is open.
///
/// Enough that arrowing up and down a list does not re-photograph anything,
/// few enough that the memory is a few megabytes at most. Each is a small
/// PNG, not a window's worth of pixels.
const KEPT: usize = 12;

/// The longer side of a preview, in pixels.
///
/// The strip it is drawn in is a few hundred pixels wide and this is drawn
/// into it, so there is room for a display that is not at 100% without there
/// being room for a megabyte of picture nobody sees.
const LONGEST_SIDE: i32 = 480;

/// Pictures of windows, for as long as the switcher is open.
#[derive(Default)]
pub struct Previews {
    inner: Mutex<HashMap<isize, String>>,
}

impl Previews {
    pub fn new() -> Self {
        Self::default()
    }

    /// A picture of one window, as a data URI, taking one if there is none.
    ///
    /// `None` when the window has closed or refuses to be photographed, which
    /// is not an error: a switcher with no picture is a switcher, and a
    /// message about it would be about Sill rather than about the window.
    pub fn of(&self, id: isize) -> Option<String> {
        if let Ok(kept) = self.inner.lock() {
            if let Some(already) = kept.get(&id) {
                return Some(already.clone());
            }
        }

        let taken = take(id)?;

        if let Ok(mut kept) = self.inner.lock() {
            // Bounded, and crudely. There is no order worth keeping here: the
            // switcher closes in seconds and takes the whole lot with it, so
            // an eviction policy would be machinery for a case that does not
            // arise.
            if kept.len() >= KEPT {
                kept.clear();
            }

            kept.insert(id, taken.clone());
        }

        Some(taken)
    }

    /// Drops every picture.
    ///
    /// Called when the switcher closes. A preview is a picture of a moment,
    /// and keeping them would mean showing somebody a window as it was the
    /// last time they looked rather than as it is.
    pub fn forget(&self) {
        if let Ok(mut kept) = self.inner.lock() {
            kept.clear();
        }
    }
}

/// Photographs one window, small, as a data URI.
fn take(id: isize) -> Option<String> {
    // Revalidated rather than trusted. A handle can be reused by a different
    // window once the first one closes, and photographing a stranger is worse
    // than showing nothing.
    let window = crate::windowing::find(id)?;

    // A minimized window has nothing on screen to photograph and `PrintWindow`
    // on one gives back an empty rectangle or the desktop behind it. Saying
    // there is no picture is honest; showing a grey box is not.
    if window.minimized {
        return None;
    }

    let shot = crate::capture::window(
        id,
        (
            window.rect.x,
            window.rect.y,
            window.rect.width,
            window.rect.height,
        ),
    )
    .ok()?;

    let png = crate::capture::thumbnail(&shot, LONGEST_SIDE)
        .to_png()
        .ok()?;

    Some(format!("data:image/png;base64,{}", base64_of(&png),))
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
