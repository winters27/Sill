//! What is playing, and the three keys people press at it.
//!
//! `Windows.Media.Control` is the same thing a keyboard's media keys talk to
//! and the same thing the volume flyout draws a little card for. Every player
//! that behaves publishes a session to it, so one row reaches Spotify, a video
//! in a browser tab and whatever else is making noise, without knowing
//! anything about any of them.
//!
//! ## Why nothing here runs unless it is asked for
//!
//! `GlobalSystemMediaTransportControlsSessionManager::RequestAsync` is a call
//! out of this process and into Windows, and reading a track's title is a
//! second one. That is not work to do because somebody typed a letter.
//!
//! The same decision App Volume and the process list already made, and made
//! for the same reason, except that those two sit behind a row of their own
//! and this is a row in the ordinary list. So the gate is [`asked`], and
//! [`matched`] is the only way the search reaches a reading: it takes the
//! reader as an argument and **does not call it** unless the query asked. A
//! keystroke that is not one of the words below costs a `trim`, an ASCII
//! lowercase and a lookup in a list of nine, and nothing else at all.
//!
//! ## Nothing playing is not a failure
//!
//! A machine with no media session answers `None`, which draws no row. Not an
//! empty row, not a row saying nothing is playing, and not an error: somebody
//! who typed "pause" with nothing playing has learned what they wanted to know
//! from the row not being there.

use serde::Serialize;

/// What is playing right now, as far as Windows is concerned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// What the track is called. Often empty for a video in a browser tab.
    pub title: String,
    /// Who it is by, when the player says.
    pub artist: String,
    /// Windows' own name for the program that owns the session.
    ///
    /// An app user model id: `Spotify.exe` for a desktop program, a long
    /// opaque string for a packaged one. Kept because it is the only thing
    /// naming the player, and a row for a track with no title has to say
    /// something.
    pub source: String,
    pub playing: bool,
    /// Whether the player says it can be skipped forward.
    ///
    /// A podcast at the end of a queue and a single video both say no, and an
    /// action offered where it will do nothing is worse than no action.
    pub can_next: bool,
}

/*
 * The words that ask.
 *
 * The whole line, exactly, the way `utilities` requires a bare generator to be
 * the whole line. A prefix rule would fire on the way to typing "player",
 * "playnite" and "media player", and a row that appears while somebody is
 * still typing something else is the launcher arguing with them.
 *
 * These nine are all imperatives or names for the thing itself. None of them
 * is a word somebody types to find a file, which is the test `utilities` had
 * to apply to `json` and failed to pass without the gate.
 */
const ASKED_BY: &[&str] = &[
    "media",
    "now playing",
    "nowplaying",
    "play",
    "pause",
    "resume",
    "next",
    "next track",
    "skip",
];

/// Whether this query is asking about what is playing.
///
/// Pure, and the whole of the cost claim. Everything expensive is behind it.
pub fn asked(query: &str) -> bool {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return false;
    }

    // Two spaces between "now" and "playing" is still somebody typing "now
    // playing", and a launcher that refused it would be being pedantic about
    // a space bar.
    let lowered = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    let lowered = lowered.to_ascii_lowercase();

    ASKED_BY.contains(&lowered.as_str())
}

/// What is playing, if the query asked and something is.
///
/// `read` is the reading of the machine, and it is **not called** unless
/// [`asked`] says so. Taking it as an argument rather than calling it inside
/// is what lets a test prove that without a media session existing at all: the
/// test passes a reader that counts, and asserts the count stayed at nought.
pub fn matched(query: &str, read: impl FnOnce() -> Option<NowPlaying>) -> Option<NowPlaying> {
    if !asked(query) {
        return None;
    }

    read()
}

/// What to call the row.
///
/// The track, then the player, then the last resort. A video in a browser tab
/// routinely publishes a session with no title at all, and a row called ""
/// is a row nobody can read.
pub fn title_for(now: &NowPlaying) -> String {
    for candidate in [now.title.trim(), player_name(&now.source)] {
        if !candidate.is_empty() {
            return candidate.to_string();
        }
    }

    "Now Playing".to_string()
}

/// What goes underneath it.
///
/// The state is always said, in both directions, because the state is the
/// thing Enter changes. A subtitle that named the artist while playing and
/// said "Paused" while paused would make pressing Enter look like it had
/// replaced the artist with a word.
pub fn subtitle_for(now: &NowPlaying) -> String {
    let state = if now.playing { "Playing" } else { "Paused" };
    let artist = now.artist.trim();

    if artist.is_empty() {
        state.to_string()
    } else {
        format!("{state} · {artist}")
    }
}

/// What to call the player, on the right of the row.
///
/// The row's title is the track, so this is the only place the program that is
/// playing it gets named. "Media" when Windows gave nothing a person would
/// read, which is a true thing to say about a row whose player will not say
/// what it is called.
pub fn player_for(now: &NowPlaying) -> String {
    match player_name(&now.source) {
        "" => "Media".to_string(),
        name => name.to_string(),
    }
}

/// A program's name out of its app user model id.
///
/// `Spotify.exe` is a name once the extension is off it. A packaged app's id
/// is an opaque pair like `AppleInc.AppleMusicWin_nzyj5cx40ttqa!App` and no
/// part of it is worth showing somebody, so that answers nothing and the
/// caller falls through.
fn player_name(source: &str) -> &str {
    let source = source.trim();

    match source.strip_suffix(".exe").or_else(|| {
        source
            .strip_suffix(".EXE")
            .or_else(|| source.strip_suffix(".Exe"))
    }) {
        Some(stem) => stem,
        // Anything that is not plainly a program's filename. An id with a `!`
        // or a `_` in it is a package identity rather than a name.
        None if source.contains('!') || source.contains('_') => "",
        None => source,
    }
}

/// How long a reading is reused for.
///
/// The same second the switches and the audio sessions use. It matters less
/// here than it does for either of those, because the gate means one keystroke
/// in a query takes a reading rather than all of them, but a search can be run
/// more than once for one keystroke and this is the shape that already exists.
pub const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(1);

/// What is playing, read at most once a second.
pub fn now(playing: &crate::state::Fresh<Option<NowPlaying>>) -> Option<NowPlaying> {
    playing.get(platform::now_playing)
}

/// Throws the reading away, so the next one is taken fresh.
///
/// Called after pressing play, pause or next. Without it the row would show
/// what was playing a moment ago for up to a second, which is exactly the
/// moment somebody is looking at the row they just pressed.
pub fn forget(playing: &crate::state::Fresh<Option<NowPlaying>>) {
    playing.forget();
}

#[cfg(windows)]
mod platform {
    use super::NowPlaying;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session,
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };

    /// The session Windows says is the current one.
    ///
    /// **Which one that is, is Windows' decision and deliberately not ours.**
    /// A machine playing music and a video at once has two sessions, and the
    /// current one is the one the system would send a media key to: the most
    /// recently started or interacted with. So Sill's row does what the key on
    /// the keyboard does, which is the only answer that will not surprise
    /// somebody. Picking a different one would mean the launcher and the
    /// keyboard disagreed about what "pause" means.
    fn current() -> Option<Session> {
        Manager::RequestAsync()
            .ok()?
            .join()
            .ok()?
            .GetCurrentSession()
            .ok()
    }

    /// What is playing, or nothing at all.
    ///
    /// Every step answers `None` rather than an error. No session is the
    /// ordinary state of a machine nobody is playing anything on, and a
    /// player that refuses to say what it is playing is a player with nothing
    /// worth drawing a row for.
    pub fn now_playing() -> Option<NowPlaying> {
        let session = current()?;

        let info = session.GetPlaybackInfo().ok()?;
        let status = info.PlaybackStatus().ok()?;

        /*
         * Closed, opened and stopped are not "paused".
         *
         * A player that has been opened and told nothing, or has finished and
         * stopped, publishes a session with no track in it. Drawing a row for
         * one would put "Paused" on screen for something nobody started, and
         * pressing Enter on it would start playing whatever that program
         * happened to have loaded.
         */
        let playing = match status {
            Status::Playing => true,
            Status::Paused => false,
            _ => return None,
        };

        // Reading the properties is a second call across the boundary, and it
        // is the one that can be slow: the player answers it, not Windows.
        let properties = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;

        let can_next = info
            .Controls()
            .and_then(|controls| controls.IsNextEnabled())
            .unwrap_or(false);

        Some(NowPlaying {
            title: properties
                .Title()
                .map(|t| t.to_string())
                .unwrap_or_default(),
            artist: properties
                .Artist()
                .map(|a| a.to_string())
                .unwrap_or_default(),
            source: session
                .SourceAppUserModelId()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            playing,
            can_next,
        })
    }

    /// Plays what is paused, or pauses what is playing.
    ///
    /// Reads the state and then asks for the opposite explicitly rather than
    /// calling `TryTogglePlayPauseAsync`, for one reason: the toggle does not
    /// say which way it went, so nothing could report what happened without
    /// reading again and racing the player's own update. Asking for a
    /// direction means the answer is known before the call returns.
    pub fn play_pause() -> Result<bool, String> {
        let session = current().ok_or_else(|| "nothing is playing".to_string())?;

        let playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .map(|status| status == Status::Playing)
            .map_err(|err| format!("could not read what is playing: {err}"))?;

        let asked = if playing {
            session.TryPauseAsync()
        } else {
            session.TryPlayAsync()
        }
        .and_then(|task| task.join())
        .map_err(|err| format!("the player refused: {err}"))?;

        // The player answered "no". Spotify does this for a track it is still
        // buffering, and a browser does it for a tab that has been closed
        // since the row was drawn.
        if !asked {
            return Err(format!(
                "the player would not {}",
                if playing { "pause" } else { "play" }
            ));
        }

        Ok(!playing)
    }

    /// Moves to the next track.
    pub fn next() -> Result<(), String> {
        let session = current().ok_or_else(|| "nothing is playing".to_string())?;

        let moved = session
            .TrySkipNextAsync()
            .and_then(|task| task.join())
            .map_err(|err| format!("the player refused: {err}"))?;

        if !moved {
            return Err("there is nothing after this one".to_string());
        }

        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    use super::NowPlaying;

    pub fn now_playing() -> Option<NowPlaying> {
        None
    }

    pub fn play_pause() -> Result<bool, String> {
        Err("media controls need Windows".to_string())
    }

    pub fn next() -> Result<(), String> {
        Err("media controls need Windows".to_string())
    }
}

pub use platform::{next, play_pause};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn track(title: &str, artist: &str, playing: bool) -> NowPlaying {
        NowPlaying {
            title: title.to_string(),
            artist: artist.to_string(),
            source: "Spotify.exe".to_string(),
            playing,
            can_next: true,
        }
    }

    /// The item's whole "done when", as a test.
    ///
    /// Every one of these is a query somebody types at a launcher in the
    /// ordinary course of using it. Not one of them may reach Windows.
    #[test]
    fn a_query_that_is_not_asking_never_takes_a_reading() {
        let taken = Cell::new(0);

        let read = || {
            taken.set(taken.get() + 1);
            Some(track("Anything", "Anyone", true))
        };

        for query in [
            "",
            "   ",
            "chrome",
            "p",
            "pl",
            "pla",
            "playnite",
            "player",
            "play store",
            "media player",
            "pauses",
            "next.js",
            "skip forward",
            "spotify",
            "2+2",
            "sha256 hello",
        ] {
            assert_eq!(
                matched(query, read),
                None,
                "{query:?} produced a row when it is not asking for one"
            );
        }

        assert_eq!(
            taken.get(),
            0,
            "the machine was read {} time(s) for queries that asked nothing",
            taken.get()
        );
    }

    #[test]
    fn the_words_that_ask_take_exactly_one_reading_each() {
        for query in ASKED_BY {
            let taken = Cell::new(0);

            let read = || {
                taken.set(taken.get() + 1);
                Some(track("Weightless", "Marconi Union", true))
            };

            assert!(
                matched(query, read).is_some(),
                "{query:?} is in the list that asks and produced no row"
            );
            assert_eq!(
                taken.get(),
                1,
                "{query:?} read the machine {} times",
                taken.get()
            );
        }
    }

    /// Case and stray whitespace are still somebody asking.
    #[test]
    fn asking_is_not_a_matter_of_typing_it_neatly() {
        assert!(asked("  Pause  "));
        assert!(asked("NEXT"));
        assert!(asked("Now  Playing"));
    }

    /// A machine with nothing playing draws no row, and that is not an error.
    #[test]
    fn nothing_playing_is_no_row_rather_than_an_empty_one() {
        assert_eq!(matched("pause", || None), None);
    }

    #[test]
    fn a_row_says_which_way_it_is_before_it_says_who_by() {
        assert_eq!(
            subtitle_for(&track("Weightless", "Marconi Union", true)),
            "Playing · Marconi Union"
        );
        assert_eq!(
            subtitle_for(&track("Weightless", "Marconi Union", false)),
            "Paused · Marconi Union"
        );
    }

    /// A video in a browser tab has no artist and often no title.
    #[test]
    fn a_track_with_nothing_said_about_it_still_reads() {
        let mut bare = track("", "", true);
        assert_eq!(subtitle_for(&bare), "Playing");
        assert_eq!(title_for(&bare), "Spotify");

        bare.source = "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App".to_string();
        assert_eq!(title_for(&bare), "Now Playing");
    }

    #[test]
    fn the_track_is_the_title_when_there_is_one() {
        assert_eq!(
            title_for(&track("Weightless", "Marconi Union", true)),
            "Weightless"
        );
    }
}
