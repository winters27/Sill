//! The radios in the machine.
//!
//! Wifi and Bluetooth, through `Windows.Devices.Radios`, which is the one
//! documented way to turn either off. Unlike the notification state and the
//! night light blob, this is a contract rather than a shape somebody worked
//! out, so it is safe to build a row on.
//!
//! Nothing is read until something asks. Enumerating radios is a call into the
//! system, not a subscription, so this costs what it is used and nothing at
//! rest.

use serde::Serialize;

/// One radio, and whether it is on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Radio {
    /// Which kind it is, as an id a row can be built on.
    pub kind: String,
    /// What it is called, for saying what changed.
    pub name: String,
    pub on: bool,
}

#[cfg(windows)]
mod platform {
    use super::Radio;
    use windows::Devices::Radios::{
        Radio as WinRadio, RadioAccessStatus, RadioKind, RadioState,
    };

    /// The kinds worth a row.
    ///
    /// Not every radio is one somebody would switch. A machine reports its
    /// mobile broadband and its FM receiver too, and neither is a thing anybody
    /// opens a launcher to toggle.
    fn named(kind: RadioKind) -> Option<&'static str> {
        match kind {
            RadioKind::WiFi => Some("wifi"),
            RadioKind::Bluetooth => Some("bluetooth"),
            _ => None,
        }
    }

    fn label(kind: &str) -> &'static str {
        match kind {
            "wifi" => "Wi-Fi",
            _ => "Bluetooth",
        }
    }

    /// Every radio Sill offers to switch.
    pub fn radios() -> Vec<Radio> {
        let Ok(found) = WinRadio::GetRadiosAsync().and_then(|task| task.join()) else {
            return Vec::new();
        };

        let mut out = Vec::new();

        for radio in found.into_iter() {
            let Ok(kind) = radio.Kind() else { continue };
            let Some(id) = named(kind) else { continue };

            // One row per kind. A machine with two Bluetooth adapters is a
            // machine with one Bluetooth switch as far as anybody is concerned.
            if out.iter().any(|r: &Radio| r.kind == id) {
                continue;
            }

            out.push(Radio {
                kind: id.to_string(),
                name: label(id).to_string(),
                on: radio.State().map(|s| s == RadioState::On).unwrap_or(false),
            });
        }

        out
    }

    /// Turns one on or off, and says what it ended up as.
    pub fn set_radio(kind: &str, on: bool) -> Result<bool, String> {
        let found = WinRadio::GetRadiosAsync()
            .and_then(|task| task.join())
            .map_err(|err| format!("could not reach the radios: {err}"))?;

        let mut touched = false;

        for radio in found.into_iter() {
            let Ok(radio_kind) = radio.Kind() else { continue };
            if named(radio_kind) != Some(kind) {
                continue;
            }

            let wanted = if on { RadioState::On } else { RadioState::Off };
            let status = radio
                .SetStateAsync(wanted)
                .and_then(|task| task.join())
                .map_err(|err| format!("could not switch {}: {err}", label(kind)))?;

            // The one failure worth naming. Switching a radio needs permission
            // the first time, and a machine where it is denied says nothing
            // otherwise: the call succeeds and the radio does not move.
            match status {
                RadioAccessStatus::Allowed => touched = true,
                RadioAccessStatus::DeniedByUser => {
                    return Err(format!("Windows is not letting Sill change the {}", label(kind)))
                }
                RadioAccessStatus::DeniedBySystem => {
                    return Err(format!("this machine does not allow the {} to be changed", label(kind)))
                }
                _ => {}
            }
        }

        if !touched {
            return Err(format!("there is no {} in this machine", label(kind)));
        }

        Ok(on)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Radio;

    pub fn radios() -> Vec<Radio> {
        Vec::new()
    }

    pub fn set_radio(_kind: &str, _on: bool) -> Result<bool, String> {
        Err("switching a radio needs Windows".to_string())
    }
}

pub use platform::{radios, set_radio};
