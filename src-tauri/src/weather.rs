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
///
/// `default` on the struct as well as its fields, which is the rule the
/// preferences module states and this was breaking: it is stored inside the
/// widget settings, so a `Place` written before `latitude` existed would have
/// failed to read and taken every other setting down with it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
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

/// How long a reading is worth reusing.
///
/// The service updates roughly this often and the sky does not change faster
/// than that in any way a launcher should care about. The widget's own timer
/// is the same length, so in the ordinary case one poll produces one call.
const FRESH_FOR: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// The last reading, kept for as long as it is worth reusing.
///
/// A managed service rather than a `static`, which is what rule 2 refuses.
/// The module comment has always said this is in Rust so the reading "can be
/// cached across summons"; it could be, and every call went to the network.
/// That started mattering more when widgets learned to stop while hidden,
/// because coming back takes a reading immediately, so ten summons in ten
/// minutes were ten forecasts of the same minute of the same day.
#[derive(Default)]
pub struct Forecast {
    last: std::sync::Mutex<Option<(String, bool, Weather, std::time::Instant)>>,
}

impl Forecast {
    /// The current conditions at a place, from the last reading when it is
    /// recent enough.
    ///
    /// Keyed by the place and the unit together. Asking for Fahrenheit after
    /// Celsius is a different answer to the same question, and handing back
    /// the other one would show the wrong number rather than a stale one.
    pub async fn at(&self, place: &Place, fahrenheit: bool) -> Result<Weather, String> {
        let key = keyed(place);

        if let Ok(held) = self.last.lock() {
            if let Some((had, unit, weather, when)) = held.as_ref() {
                if *had == key && *unit == fahrenheit && when.elapsed() < FRESH_FOR {
                    return Ok(weather.clone());
                }
            }
        }

        let fetched = fetch(place, fahrenheit).await?;

        if let Ok(mut held) = self.last.lock() {
            *held = Some((key, fahrenheit, fetched.clone(), std::time::Instant::now()));
        }

        Ok(fetched)
    }
}

/// What identifies a place for the cache.
///
/// The coordinates rather than the name: two people can call the same place
/// different things, the request is made with the numbers, and the numbers are
/// what the answer depends on.
fn keyed(place: &Place) -> String {
    format!("{:.4},{:.4}", place.latitude, place.longitude)
}

/// Asks the service, with nothing remembered.
async fn fetch(place: &Place, fahrenheit: bool) -> Result<Weather, String> {
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

    fn place(name: &str, lat: f64, lon: f64) -> Place {
        Place {
            name: name.to_string(),
            region: String::new(),
            latitude: lat,
            longitude: lon,
        }
    }

    /// Two names for one place are one place.
    ///
    /// The request is made with the coordinates and the answer depends on the
    /// coordinates, so that is what the reading is filed under. Keying by the
    /// name would fetch twice for somebody who typed "Bakersfield" once and
    /// "Bakersfield, CA" the next time.
    #[test]
    fn a_place_is_identified_by_where_it_is() {
        assert_eq!(
            keyed(&place("Bakersfield", 35.3733, -119.0187)),
            keyed(&place("Bakersfield, California", 35.3733, -119.0187))
        );
    }

    /// And two places are two places.
    #[test]
    fn somewhere_else_is_not_the_same_reading() {
        assert_ne!(
            keyed(&place("Bakersfield", 35.3733, -119.0187)),
            keyed(&place("Portland", 45.5152, -122.6784))
        );
    }

    /// Four decimal places is about eleven metres, which is the resolution the
    /// request itself is made at. Rounding the key harder than the request
    /// would hand back a reading for somewhere the service was never asked
    /// about.
    #[test]
    fn the_key_is_as_precise_as_the_request() {
        assert_eq!(
            keyed(&place("a", 35.373_312, -119.018_712)),
            "35.3733,-119.0187"
        );
    }
}
