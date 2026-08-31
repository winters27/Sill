//! Which speakers Windows is using.
//!
//! Switching between headphones and speakers is the thing people open a sound
//! panel for, and it is three clicks deep in every version of Windows there
//! has been.
//!
//! ## The awkward half
//!
//! Listing the outputs is documented: `IMMDeviceEnumerator` and a property
//! store. **Choosing one is not.** Windows has never shipped a public way to
//! set the default audio endpoint, and the only way anything does it is
//! `IPolicyConfig`, an internal interface that has been in the same place since
//! Vista and that every audio switcher on the platform uses.
//!
//! It is declared here by hand because no crate ships it. The methods before
//! the one that is wanted exist as placeholders and are never called: a COM
//! vtable is one pointer per method in declaration order, so they are what puts
//! `SetDefaultEndpoint` at the right offset. Removing them would silently call
//! the wrong function, which is why they say so.

use serde::Serialize;

/// One thing sound can come out of.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    /// Windows' own id for the endpoint, which is what selects it.
    pub id: String,
    /// What it calls itself, which is what somebody recognises.
    pub name: String,
    pub current: bool,
}

#[cfg(windows)]
mod platform {
    // The interface's method names are Windows' own, which is the point: they
    // have to line up with something somebody else defined.
    #![allow(non_snake_case)]

    use super::Output;
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR};
    use windows::Win32::Media::Audio::{
        eConsole, eCommunications, eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED, STGM_READ,
    };
    // Under Foundation, not PropertiesSystem where the docs group it.
    use windows::Win32::Foundation::PROPERTYKEY;

    /// `PKEY_Device_FriendlyName`, which is the name shown in the sound panel.
    const FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };

    /// The undocumented interface that sets the default endpoint.
    ///
    /// Everything above `SetDefaultEndpoint` is a placeholder. A COM vtable is
    /// one pointer per method in declaration order, so these are what put the
    /// one method that matters at the right offset. **None of them is ever
    /// called, and their signatures are deliberately wrong**: calling one would
    /// be calling a function of a different shape.
    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    unsafe trait IPolicyConfig: IUnknown {
        unsafe fn never_called_get_mix_format(&self) -> HRESULT;
        unsafe fn never_called_get_device_format(&self) -> HRESULT;
        unsafe fn never_called_reset_device_format(&self) -> HRESULT;
        unsafe fn never_called_set_device_format(&self) -> HRESULT;
        unsafe fn never_called_get_processing_period(&self) -> HRESULT;
        unsafe fn never_called_set_processing_period(&self) -> HRESULT;
        unsafe fn never_called_get_share_mode(&self) -> HRESULT;
        unsafe fn never_called_set_share_mode(&self) -> HRESULT;
        unsafe fn never_called_get_property_value(&self) -> HRESULT;
        unsafe fn never_called_set_property_value(&self) -> HRESULT;
        /// The one this exists for.
        unsafe fn SetDefaultEndpoint(&self, id: PCWSTR, role: i32) -> HRESULT;
    }

    /// `CPolicyConfigClient`.
    const POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    /// Runs some COM work with an apartment around it.
    fn with_com<T>(work: impl FnOnce() -> windows::core::Result<T>) -> Result<T, String> {
        // SAFETY: initialised and uninitialised on the same thread around the
        // whole call, and every interface is released by its own Drop.
        unsafe {
            // An already initialised apartment answers with a failure code that
            // is not an error. Only the uninitialise has to match.
            let initialised = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
            let result = work();

            if initialised {
                CoUninitialize();
            }

            result.map_err(|err| format!("the sound system refused: {err}"))
        }
    }

    /// Every output sound can currently come out of.
    pub fn outputs() -> Vec<Output> {
        with_com(|| {
            // SAFETY: every pointer here comes from the call above it, and the
            // one allocation that is handed over is freed below.
            unsafe {
                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

                // The one in use, so a row can say which it is rather than
                // making somebody switch to find out.
                let current = enumerator
                    .GetDefaultAudioEndpoint(eRender, eConsole)
                    .and_then(|device| device.GetId())
                    .map(|id| id.to_string().unwrap_or_default())
                    .unwrap_or_default();

                let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
                let count = devices.GetCount()?;

                let mut found = Vec::with_capacity(count as usize);

                for at in 0..count {
                    let device = devices.Item(at)?;

                    let id = device.GetId()?;
                    let id_text = id.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(id.0.cast()));

                    let store = device.OpenPropertyStore(STGM_READ)?;
                    let name = store.GetValue(&FRIENDLY_NAME)?;
                    let name_text = name.to_string();

                    if id_text.is_empty() || name_text.is_empty() {
                        continue;
                    }

                    found.push(Output {
                        current: id_text == current,
                        id: id_text,
                        name: name_text,
                    });
                }

                Ok(found)
            }
        })
        .unwrap_or_default()
    }

    /// Sends sound to one of them.
    pub fn set_output(id: &str) -> Result<(), String> {
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();

        with_com(|| {
            // SAFETY: the id is a null-terminated wide string that outlives the
            // call, and the interface is released by its own Drop.
            unsafe {
                let policy: IPolicyConfig = CoCreateInstance(&POLICY_CONFIG, None, CLSCTX_ALL)?;

                /*
                 * All three roles, or the switch half happens.
                 *
                 * Windows keeps a separate default for ordinary sound, for
                 * multimedia and for calls. Setting only the first leaves a
                 * voice call coming out of the speakers somebody just switched
                 * away from, which reads as the switch not having worked.
                 */
                for role in [eConsole, eMultimedia, eCommunications] {
                    policy
                        .SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role.0)
                        .ok()?;
                }

                Ok(())
            }
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Output;

    pub fn outputs() -> Vec<Output> {
        Vec::new()
    }

    pub fn set_output(_id: &str) -> Result<(), String> {
        Err("changing the audio output needs Windows".to_string())
    }
}

pub use platform::{outputs, set_output};

/// A short name for an output, for a row that has to fit on one line.
///
/// Windows names an endpoint after the socket and the card: "Speakers (Realtek
/// High Definition Audio)". The part in brackets is the same for every output
/// on the machine, which makes it the part that says nothing.
pub fn short_name(name: &str) -> String {
    let trimmed = name.trim();

    match trimmed.find(" (") {
        // Only when what is left is worth showing. "(Realtek)" alone is not a
        // name, and neither is one character.
        Some(at) if at >= 2 => trimmed[..at].to_string(),
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::short_name;

    /// The bracketed half is the card, and it is the same on every row.
    #[test]
    fn the_card_is_dropped_from_the_name() {
        assert_eq!(
            short_name("Speakers (Realtek High Definition Audio)"),
            "Speakers",
        );
        assert_eq!(short_name("Headphones (2- USB Audio Device)"), "Headphones");
    }

    #[test]
    fn a_name_with_no_bracket_is_left_alone() {
        assert_eq!(short_name("Digital Output"), "Digital Output");
    }

    /// Dropping everything would leave a row with no name at all.
    #[test]
    fn a_name_that_is_only_a_bracket_is_kept_whole() {
        assert_eq!(short_name("(Realtek Audio)"), "(Realtek Audio)");
        assert_eq!(short_name("A (B)"), "A (B)");
    }

    #[test]
    fn surrounding_space_goes() {
        assert_eq!(short_name("  Speakers (Realtek)  "), "Speakers");
    }
}
