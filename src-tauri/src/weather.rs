//! What it is doing outside.
//!
//! ## Why this is in Rust and not the window
//!
//! Not because the arithmetic is hard. The window could fetch this itself, and
//! then the address, the caching and the failure handling would live in a
//! component that is reloaded every time the page is. Here it can be cached
//! across summons, and there is one place that knows which service is being
//! asked and what it is told.
//!
//! ## Open-Meteo, and no key
//!
//! Free, no account, no key to store, and no key means no credential to seal,
//! no settings row to paste one into and nothing to leak. That is worth more
//! than a slightly better forecast.
//!
//! ## What is sent
//!
//! A latitude and a longitude, and nothing else. **The location is one the
//! user typed**, geocoded once and remembered; Sill does not ask the machine
//! where it is and does not send an address to be looked up by IP. A launcher
//! that quietly reports your position because you wanted the temperature is
//! not a trade anybody agreed to.

use serde::{Deserialize, Serialize};

/// Where the forecast is for.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    /// What to call it, as the service spells it.
    pub name: String,
    /// The region, when there is one worth showing: there are a great many
    /// Portlands and the state is what tells them apart.
    #[serde(default)]
    pub region: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// One reading of the sky.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub place: String,
    pub temperature: f64,
    pub feels_like: f64,
    pub high: f64,
    pub low: f64,
    /// WMO code, which the window turns into words and a glyph.
    pub code: u8,
    pub is_day: bool,
    /// `F` or `C`, so the window does not have to guess what it was given.
    pub unit: char,
}

/// Finds a place by name.
///
/// One result. A picker of five Portlands is a decision nobody wants to make
/// about weather, and the first result is the populous one, which is the one
/// somebody typing "Portland" means.
pub async fn find(name: &str) -> Result<Place, String> {
    let name = name.trim();

    if name.is_empty() {
        return Err("Type somewhere to get the weather for.".to_string());
    }

    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=en&format=json",
        urlencoding(name)
    );

    let body: serde_json::Value = crate::dictation::fetch::client()
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("could not look that place up: {err}"))?
        .json()
        .await
        .map_err(|err| format!("that place could not be read: {err}"))?;

    let first = body["results"]
        .get(0)
        .ok_or_else(|| format!("Nowhere called \"{name}\" was found."))?;

    Ok(Place {
        name: first["name"].as_str().unwrap_or(name).to_string(),
        region: first["admin1"].as_str().unwrap_or_default().to_string(),
        latitude: first["latitude"].as_f64().unwrap_or_default(),
        longitude: first["longitude"].as_f64().unwrap_or_default(),
    })
}

/// The current conditions at a place.
pub async fn at(place: &Place, fahrenheit: bool) -> Result<Weather, String> {
    let unit = if fahrenheit { "fahrenheit" } else { "celsius" };

    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={:.4}&longitude={:.4}\
         &current=temperature_2m,weather_code,is_day,apparent_temperature\
         &daily=temperature_2m_max,temperature_2m_min\
         &temperature_unit={unit}&timezone=auto&forecast_days=1",
        place.latitude, place.longitude
    );

    let body: serde_json::Value = crate::dictation::fetch::client()
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("could not reach the forecast: {err}"))?
        .json()
        .await
        .map_err(|err| format!("the forecast could not be read: {err}"))?;

    let current = &body["current"];
    let daily = &body["daily"];

    let number = |value: &serde_json::Value| value.as_f64().unwrap_or_default();

    Ok(Weather {
        place: if place.region.is_empty() {
            place.name.clone()
        } else {
            format!("{}, {}", place.name, place.region)
        },
        temperature: number(&current["temperature_2m"]),
        feels_like: number(&current["apparent_temperature"]),
        high: number(&daily["temperature_2m_max"][0]),
        low: number(&daily["temperature_2m_min"][0]),
        code: current["weather_code"].as_u64().unwrap_or(0) as u8,
        // The service answers 1 or 0 rather than a boolean.
        is_day: current["is_day"].as_u64().unwrap_or(1) == 1,
        unit: if fahrenheit { 'F' } else { 'C' },
    })
}

/// Percent-encodes a place name for a query string.
///
/// Written out rather than pulled in: the only thing that reaches this is a
/// place name, and the set of characters that need escaping in one is small
/// and known. Everything unreserved passes through, and everything else goes
/// as its bytes, which is what a query string wants.
fn urlencoding(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_place_with_a_space_survives_the_query_string() {
        assert_eq!(urlencoding("San Francisco"), "San%20Francisco");
        assert_eq!(urlencoding("Portland"), "Portland");
    }

    /// Names are not all ASCII, and a raw multi-byte character in a query
    /// string is what makes a service answer 400.
    #[test]
    fn an_accented_name_goes_as_its_bytes() {
        assert_eq!(urlencoding("Zürich"), "Z%C3%BCrich");
        assert_eq!(urlencoding("São Paulo"), "S%C3%A3o%20Paulo");
    }

    /// Nothing typed is a message rather than a request for nowhere.
    #[tokio::test]
    async fn nowhere_is_asked_for_when_nothing_was_typed() {
        assert!(find("   ").await.is_err());
    }
}
