//! Muting the machine while a dictation is running.
//!
//! Music playing through speakers is picked up by the microphone and
//! transcribed as words, which is the most common way a dictation comes back
//! with something nobody said. Muting the default output for the length of
//! the recording removes the problem at the source.
//!
//! The system mute is used rather than a per-application one: whatever is
//! playing is not necessarily Sill's business to enumerate, and the state is
//! restored the moment the recording ends whichever way it ends.

/// Sets or clears the system output mute. Returns whether it took effect.
///
/// A boolean rather than a `Result` because every caller's response to
/// failure is the same: carry on with the dictation. Muting is a courtesy,
/// and refusing to record because a volume endpoint would not open would be
/// absurd.
#[cfg(windows)]
pub fn mute(on: bool) -> bool {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    // SAFETY: COM is initialised and uninitialised on the same thread around
    // the whole call, and every interface is released by its own Drop.
    unsafe {
        // The hook thread has never initialised COM, and an already
        // initialised apartment answers with a failure code that is not an
        // error. Both cases are fine; only the uninitialise has to match.
        let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();

        let result = (|| -> windows::core::Result<()> {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            volume.SetMute(on, std::ptr::null())
        })();

        if initialised {
            CoUninitialize();
        }

        if let Err(err) = &result {
            crate::say!("could not change the system mute: {err}");
        }
        result.is_ok()
    }
}

#[cfg(not(windows))]
pub fn mute(_on: bool) -> bool {
    false
}
