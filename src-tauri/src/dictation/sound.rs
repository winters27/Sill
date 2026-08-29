//! Audio cues for the start and end of a dictation.
//!
//! Two WAVs ship with the app under `resources/sounds/`, so cues work out of
//! the box for everyone. The settings can point at different files, in which
//! case the bundled ones are simply not used.
//!
//! Files rather than `include_bytes!`: a bundled resource can be swapped
//! without a rebuild, and the binary stays free of ~60 KB of audio.
//!
//! Missing, unreadable, or malformed files are ignored rather than reported.
//! A cue is a nicety, and failing a dictation over one would be absurd.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// Which cue to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Start,
    Stop,
}

impl Cue {
    /// Bundled file name for this cue.
    fn resource(self) -> &'static str {
        match self {
            Cue::Start => "resources/sounds/dictation_start.wav",
            Cue::Stop => "resources/sounds/dictation_stop.wav",
        }
    }
}

/// Plays `cue`.
pub fn play(app: &AppHandle, cue: Cue) {
    let Some(path) = resolve(app, cue) else {
        return;
    };
    play_file(&path);
}

/// The bundled file for this cue.
fn resolve(app: &AppHandle, cue: Cue) -> Option<PathBuf> {
    let bundled = app
        .path()
        .resolve(cue.resource(), tauri::path::BaseDirectory::Resource)
        .ok()?;
    bundled.is_file().then_some(bundled)
}

#[cfg(windows)]
fn play_file(path: &std::path::Path) {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Media::Audio::{
        PlaySoundW, SND_ASYNC, SND_FILENAME, SND_NODEFAULT, SND_NOWAIT,
    };

    // SND_ASYNC so a cue never delays opening the microphone, SND_NODEFAULT
    // so an unplayable file is silent rather than the Windows ding, and
    // SND_NOWAIT so a busy audio device drops the cue instead of blocking.
    let wide = HSTRING::from(path.as_os_str());
    unsafe {
        let _ = PlaySoundW(
            PCWSTR(wide.as_ptr()),
            None,
            SND_FILENAME | SND_ASYNC | SND_NODEFAULT | SND_NOWAIT,
        );
    }
}

#[cfg(not(windows))]
fn play_file(_path: &std::path::Path) {
    // Deliberately silent elsewhere, for the same reason the keyboard hook is
    // Windows-only so far.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_cue_names_a_distinct_bundled_file() {
        // A copy-paste slip here would make start and stop sound identical,
        // which is subtle enough to ship unnoticed.
        assert_ne!(Cue::Start.resource(), Cue::Stop.resource());
        assert!(Cue::Start.resource().ends_with("dictation_start.wav"));
        assert!(Cue::Stop.resource().ends_with("dictation_stop.wav"));
    }

    #[test]
    fn bundled_paths_are_relative_so_they_resolve_against_the_resource_dir() {
        for cue in [Cue::Start, Cue::Stop] {
            let path = std::path::Path::new(cue.resource());
            assert!(path.is_relative(), "{cue:?} must not be absolute");
        }
    }
}
