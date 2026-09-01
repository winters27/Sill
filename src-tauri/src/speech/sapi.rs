//! The voice Windows already has.
//!
//! SAPI's `ISpVoice` synthesises and plays in one call, has been in Windows
//! since XP, and needs nothing installed, which is what makes it the fallback
//! rather than the choice: David and Zira are concatenative voices from a
//! decade ago and they sound like it. Windows 11 does ship neural voices, and
//! they are registered under `Speech_OneCore` where only Narrator and Edge can
//! reach them, so there is no better local voice to be had for free.
//!
//! Everything better is a provider. See [`super`].
//!
//! ## Why a thread of its own
//!
//! A COM object belongs to the apartment that created it, so the voice cannot
//! be handed between threads and cannot be parked in Tauri state. It stays on
//! one thread and is reached by a channel, which is the same shape the
//! clipboard watcher uses.
//!
//! The thread is started by the first thing said and not before. Nothing is
//! read aloud on a machine where nobody asks for it, so there is nothing to
//! start at launch and nothing running at rest.

use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;

/// What the speaking thread is asked to do.
enum Say {
    Aloud(String),
    Stop,
}

/// The voice, and the one thread allowed to touch it.
#[derive(Default)]
pub struct Sapi {
    /// `None` until the first thing is said.
    to_voice: Mutex<Option<Sender<Say>>>,
}

impl Sapi {
    /// Says something, interrupting whatever was already being said.
    ///
    /// Interrupting is the right default for a launcher: reading is asked for
    /// on one thing at a time, and queueing would mean a second request is
    /// heard after a paragraph nobody is still waiting for.
    pub fn aloud(&self, text: &str) -> Result<(), String> {
        let text = text.trim();

        if text.is_empty() {
            return Err("There is nothing to read".to_string());
        }

        self.send(Say::Aloud(text.to_string()))
    }

    /// Stops mid-sentence.
    ///
    /// Not an error when nothing is speaking and nothing has ever spoken: the
    /// thread is only started by something being said, and asking for silence
    /// from a voice that never spoke is already true.
    pub fn stop(&self) -> Result<(), String> {
        let started = self.to_voice.lock().map_err(|_| POISONED)?.is_some();

        if !started {
            return Ok(());
        }

        self.send(Say::Stop)
    }

    /// Hands one instruction to the thread, starting it if this is the first.
    fn send(&self, what: Say) -> Result<(), String> {
        let mut held = self.to_voice.lock().map_err(|_| POISONED)?;

        if held.is_none() {
            *held = Some(start()?);
        }

        let Some(sender) = held.as_ref() else {
            return Err("the voice could not be started".to_string());
        };

        if sender.send(what).is_err() {
            // The thread is gone, so the handle is stale. Dropping it means
            // the next attempt starts a fresh one rather than failing forever
            // against a channel nobody is reading.
            *held = None;
            return Err("the voice stopped answering".to_string());
        }

        Ok(())
    }
}

const POISONED: &str = "the voice is in an unknown state";

/// Starts the thread that owns the voice.
#[cfg(windows)]
fn start() -> Result<Sender<Say>, String> {
    use windows::core::HSTRING;
    use windows::Win32::Media::Speech::{ISpVoice, SpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    let (tx, rx) = channel::<Say>();
    let (ready, started) = channel::<Result<(), String>>();

    std::thread::Builder::new()
        .name("sill-speech".into())
        .spawn(move || {
            // SAFETY: this thread initialises its own apartment, creates the
            // voice in it, and never lets the interface leave.
            let made = unsafe {
                // Apartment threaded, because that is what an object with an
                // audio output and its own message pump expects.
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                CoCreateInstance::<_, ISpVoice>(&SpVoice, None, CLSCTX_ALL)
            };

            let voice = match made {
                Ok(voice) => {
                    let _ = ready.send(Ok(()));
                    voice
                }
                Err(err) => {
                    let _ = ready.send(Err(format!("no voice on this machine: {err}")));
                    return;
                }
            };

            // Every instruction purges what is already being said. A launcher
            // asks about one thing at a time.
            let interrupting = (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_NOT_XML.0) as u32;

            while let Ok(what) = rx.recv() {
                let said = match what {
                    // SPF_IS_NOT_XML is not optional. SAPI reads its input as
                    // markup by default, so any text containing a `<` is
                    // parsed rather than spoken, and a clipboard full of HTML
                    // reads as silence or as an error nobody asked about.
                    Say::Aloud(text) => {
                        let wide = HSTRING::from(text);
                        // SAFETY: the string outlives the call, and the voice
                        // was created on this thread.
                        unsafe { voice.Speak(&wide, interrupting, None) }
                    }
                    // Nothing to say, purging first: SAPI's own way to fall
                    // silent immediately.
                    Say::Stop => {
                        let nothing = HSTRING::new();
                        // SAFETY: as above.
                        unsafe {
                            voice.Speak(
                                &nothing,
                                (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32,
                                None,
                            )
                        }
                    }
                };

                if let Err(err) = said {
                    crate::say!("speech: {err}");
                }
            }
        })
        .map_err(|err| format!("could not start the voice: {err}"))?;

    // Waiting for the voice to exist rather than assuming it does, so a
    // machine with no speech engine says so at the moment somebody asks
    // instead of falling silent and looking like a broken action.
    started
        .recv()
        .map_err(|_| "the voice did not start".to_string())??;

    Ok(tx)
}

#[cfg(not(windows))]
fn start() -> Result<Sender<Say>, String> {
    Err("reading aloud needs Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whitespace is not something to read, and asking says so rather than
    /// starting a voice to say nothing.
    #[test]
    fn there_is_nothing_to_read_in_blank_text() {
        let speech = Sapi::default();

        for blank in ["", "   ", "\n\t "] {
            assert!(
                speech.aloud(blank).is_err(),
                "{blank:?} should not be worth speaking"
            );
        }
    }

    /// Stopping before anything has been said must not start a voice.
    ///
    /// The thread exists only because something was read aloud, so silence is
    /// already the state being asked for, and starting a speech engine to
    /// deliver it would be work in answer to a request for none.
    #[test]
    fn stopping_when_nothing_ever_spoke_starts_nothing() {
        let speech = Sapi::default();

        assert!(speech.stop().is_ok());
        assert!(
            speech.to_voice.lock().unwrap().is_none(),
            "asking for silence started a voice"
        );
    }
}
