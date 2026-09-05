//! Time zones, from the table Windows already keeps.
//!
//! `tokyo time` should say what time it is there and how far ahead that is,
//! and a pinned clock should be able to show a second city. Neither needs a
//! bundled time zone database: Windows has one, with rules per year and a
//! display name per zone listing the cities it covers, and ICU on the same
//! machine maps each of its keys to the IANA name the browser's own clock
//! understands. So this module reads both and bundles nothing.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. The table is read the first time somebody asks for a city, held
//! for an hour in a `Fresh`, and dropped when it goes stale. [`asked`] is a
//! comparison on the first and last word, and [`matched`] takes the table as
//! a closure, so a keystroke that is not a question about time never
//! enumerates anything.

use std::time::Duration;

/// How long the zone table is held once read. It changes with a Windows
/// update and nothing else, so an hour is generous.
pub const FRESH_FOR: Duration = Duration::from_secs(60 * 60);

/// How many cities one query is answered with.
const MOST_ROWS: usize = 5;

/// One zone as Windows lists it.
#[derive(Clone, Debug)]
pub struct Zone {
    /// Windows' own name, `Tokyo Standard Time`, which is also its registry
    /// key and what ICU is asked about.
    pub key: String,
    /// The cities the display name lists, `Osaka`, `Sapporo`, `Tokyo`.
    pub cities: Vec<String>,
    /// The rules, kept so a reading needs no second enumeration.
    #[cfg(windows)]
    rules: windows::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION,
}

/// What time it is somewhere, relative to here.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub city: String,
    /// The zone's Windows key, for the widget to ask ICU about.
    pub key: String,
    /// The clock there, in this machine's own time format.
    pub clock: String,
    pub weekday: String,
    /// Minutes ahead of this machine. Negative is behind.
    pub offset_minutes: i32,
}

/// One city as the widget draws it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shown {
    pub city: String,
    /// `Asia/Tokyo`, or nothing when neither Windows nor ICU knows the city,
    /// in which case the widget says so rather than showing the wrong time.
    pub iana: Option<String>,
}

/// The cities somebody chose, resolved for the widget.
///
/// `iana` is handed in rather than called, so the resolution can be checked
/// against a fixture without ICU.
pub fn shown(
    wanted: &[String],
    zones: &[Zone],
    iana: impl Fn(&str) -> Option<String>,
) -> Vec<Shown> {
    wanted
        .iter()
        .map(|city| match find(city, zones).into_iter().next() {
            Some(zone) => Shown {
                city: titled(city, zone),
                iana: iana(&zone.key),
            },
            None => Shown {
                city: city.trim().to_string(),
                iana: None,
            },
        })
        .collect()
}

/// The city a query asks about, if it asks about time somewhere.
///
/// `tokyo time`, `time in tokyo`, `time tokyo`. `time` on its own is not a
/// question about anywhere, and `timer` is the reminder's word.
pub fn asked(query: &str) -> Option<String> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let (first, last) = (words.first()?, words.last()?);

    let city: Vec<&str> = if first.eq_ignore_ascii_case("time") {
        let rest = &words[1..];
        match rest.first() {
            Some(word) if word.eq_ignore_ascii_case("in") => rest[1..].to_vec(),
            _ => rest.to_vec(),
        }
    } else if last.eq_ignore_ascii_case("time") {
        words[..words.len() - 1].to_vec()
    } else {
        return None;
    };

    if city.is_empty() {
        return None;
    }

    Some(city.join(" "))
}

/// The zones whose cities the query names, whole words only.
///
/// `paris` finds Paris and not Parish; `new york` finds the zone whose city
/// is New York. Case does not matter.
pub fn find<'a>(city: &str, zones: &'a [Zone]) -> Vec<&'a Zone> {
    let wanted = city.trim().to_ascii_lowercase();
    if wanted.is_empty() {
        return Vec::new();
    }

    zones
        .iter()
        .filter(|zone| {
            zone.cities
                .iter()
                .any(|known| known.to_ascii_lowercase() == wanted)
        })
        .collect()
}

/// The readings a query asks for, reading the table only if it does.
pub fn matched(query: &str, zones: impl FnOnce() -> std::sync::Arc<Vec<Zone>>) -> Vec<Reading> {
    let Some(city) = asked(query) else {
        return Vec::new();
    };

    let zones = zones();
    find(&city, &zones)
        .into_iter()
        .filter_map(|zone| read(zone, &city))
        .take(MOST_ROWS)
        .collect()
}

/// A difference in minutes, said the way a person would.
pub fn said(offset_minutes: i32) -> String {
    if offset_minutes == 0 {
        return "the same time".to_string();
    }

    let way = if offset_minutes > 0 { "ahead" } else { "behind" };
    let minutes = offset_minutes.abs();
    let (hours, rest) = (minutes / 60, minutes % 60);

    let amount = match (hours, rest) {
        (0, m) => format!("{m} minutes"),
        (1, 0) => "1 hour".to_string(),
        (h, 0) => format!("{h} hours"),
        (1, m) => format!("1 hour {m} minutes"),
        (h, m) => format!("{h} hours {m} minutes"),
    };

    format!("{amount} {way}")
}

/// The city as the person typed it, with each word's first letter up.
fn titled(city: &str, zone: &Zone) -> String {
    let wanted = city.trim().to_ascii_lowercase();
    zone.cities
        .iter()
        .find(|known| known.to_ascii_lowercase() == wanted)
        .cloned()
        .unwrap_or_else(|| city.trim().to_string())
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Every zone Windows knows, with the cities its display name lists.
#[cfg(windows)]
pub fn all() -> Vec<Zone> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    use windows::Win32::System::Time::{
        EnumDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION,
    };

    let mut zones = Vec::new();
    let mut index = 0;

    loop {
        let mut rules = DYNAMIC_TIME_ZONE_INFORMATION::default();
        // SAFETY: fills in a stack struct; a non-zero return is the end.
        let result = unsafe { EnumDynamicTimeZoneInformation(index, &mut rules) };
        if result != ERROR_SUCCESS.0 {
            break;
        }
        index += 1;

        let key = wide(&rules.TimeZoneKeyName);
        if key.is_empty() {
            continue;
        }

        // The display name is where the cities are, and it is only in the
        // registry. Localised on a non-English Windows, which is why the key
        // itself is matched as well below.
        let display = crate::apps::read_string(
            HKEY_LOCAL_MACHINE,
            &format!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Time Zones\{key}"),
            "Display",
        )
        .unwrap_or_default();

        zones.push(Zone {
            cities: cities_of(&display, &key),
            key,
            rules,
        });
    }

    zones
}

#[cfg(not(windows))]
pub fn all() -> Vec<Zone> {
    Vec::new()
}

/// The cities in a display name, plus the key's own name.
///
/// `(UTC+09:00) Osaka, Sapporo, Tokyo` is three cities. The key,
/// `Tokyo Standard Time`, contributes `Tokyo` as well, which is what makes a
/// localised display name still findable by the name Windows uses everywhere
/// else.
fn cities_of(display: &str, key: &str) -> Vec<String> {
    let mut cities: Vec<String> = Vec::new();

    let listed = display
        .split_once(") ")
        .map(|(_, rest)| rest)
        .unwrap_or(display);

    for city in listed.split(',') {
        let city = city.trim();
        if !city.is_empty() && !cities.iter().any(|known| known == city) {
            cities.push(city.to_string());
        }
    }

    // `Tokyo Standard Time`, `W. Europe Standard Time`, `UTC`.
    let named = key
        .trim_end_matches(" Standard Time")
        .trim_end_matches(" Daylight Time")
        .trim();
    if !named.is_empty() && !cities.iter().any(|known| known == named) {
        cities.push(named.to_string());
    }

    cities
}

/// What time it is in a zone right now.
#[cfg(windows)]
pub fn read(zone: &Zone, city: &str) -> Option<Reading> {
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::Globalization::{GetTimeFormatEx, TIME_NOSECONDS};
    use windows::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
    use windows::Win32::System::Time::{
        GetTimeZoneInformationForYear, SystemTimeToTzSpecificLocalTime,
        TIME_ZONE_INFORMATION,
    };

    // SAFETY: both fill in stack structs and cannot fail.
    let (utc, here) = unsafe { (GetSystemTime(), GetLocalTime()) };

    let mut rules = TIME_ZONE_INFORMATION::default();
    // SAFETY: the dynamic rules are a valid struct for the life of the call.
    unsafe { GetTimeZoneInformationForYear(utc.wYear, Some(&zone.rules), &mut rules) }.ok()?;

    let mut there = SYSTEMTIME::default();
    // SAFETY: the rules and the input are valid for the life of the call.
    unsafe { SystemTimeToTzSpecificLocalTime(Some(&rules), &utc, &mut there) }.ok()?;

    let mut buffer = [0u16; 32];
    // SAFETY: a null locale name is the user's own, and the buffer length is
    // given.
    let written = unsafe {
        GetTimeFormatEx(
            None,
            TIME_NOSECONDS,
            Some(&there),
            None,
            Some(&mut buffer),
        )
    };
    let clock = if written > 1 {
        String::from_utf16_lossy(&buffer[..(written as usize - 1)])
    } else {
        format!("{:02}:{:02}", there.wHour, there.wMinute)
    };

    Some(Reading {
        city: titled(city, zone),
        key: zone.key.clone(),
        clock,
        weekday: WEEKDAYS[usize::from(there.wDayOfWeek % 7)].to_string(),
        offset_minutes: minutes_apart(&there, &here),
    })
}

#[cfg(not(windows))]
pub fn read(_zone: &Zone, _city: &str) -> Option<Reading> {
    None
}

/// The IANA name for a Windows zone key, `Asia/Tokyo` for
/// `Tokyo Standard Time`, which is what a browser's clock understands.
///
/// Asked of ICU, which Windows ships, rather than of a table bundled here.
#[cfg(windows)]
pub fn iana_of(key: &str) -> Option<String> {
    use windows::core::PCSTR;
    use windows::Win32::Globalization::{ucal_getTimeZoneIDForWindowsID, U_ZERO_ERROR};

    let wide: Vec<u16> = key.encode_utf16().collect();
    let mut out = [0u16; 64];
    let mut status = U_ZERO_ERROR;

    // SAFETY: both buffers are given with their lengths, and a null region
    // asks for the zone's canonical mapping.
    let written = unsafe {
        ucal_getTimeZoneIDForWindowsID(
            wide.as_ptr(),
            wide.len() as i32,
            PCSTR::null(),
            out.as_mut_ptr(),
            out.len() as i32,
            &mut status,
        )
    };

    if status.0 > 0 || written <= 0 {
        return None;
    }

    Some(String::from_utf16_lossy(&out[..written as usize]))
}

#[cfg(not(windows))]
pub fn iana_of(_key: &str) -> Option<String> {
    None
}

/// Minutes from one wall clock to another, both read at the same instant.
#[cfg(windows)]
fn minutes_apart(
    there: &windows::Win32::Foundation::SYSTEMTIME,
    here: &windows::Win32::Foundation::SYSTEMTIME,
) -> i32 {
    let stamp = |t: &windows::Win32::Foundation::SYSTEMTIME| {
        crate::timers::days_from_civil(i64::from(t.wYear), i64::from(t.wMonth), i64::from(t.wDay))
            * 24
            * 60
            + i64::from(t.wHour) * 60
            + i64::from(t.wMinute)
    };

    (stamp(there) - stamp(here)) as i32
}

#[cfg(windows)]
fn wide(text: &[u16]) -> String {
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    String::from_utf16_lossy(&text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(key: &str, display: &str) -> Zone {
        Zone {
            key: key.to_string(),
            cities: cities_of(display, key),
            #[cfg(windows)]
            rules: Default::default(),
        }
    }

    fn fixture() -> Vec<Zone> {
        vec![
            zone("Tokyo Standard Time", "(UTC+09:00) Osaka, Sapporo, Tokyo"),
            zone(
                "Romance Standard Time",
                "(UTC+01:00) Brussels, Copenhagen, Madrid, Paris",
            ),
            zone("Eastern Standard Time", "(UTC-05:00) Eastern Time (US & Canada)"),
            zone("UTC", "(UTC) Coordinated Universal Time"),
        ]
    }

    #[test]
    fn the_word_is_the_gate() {
        assert_eq!(asked("tokyo time").as_deref(), Some("tokyo"));
        assert_eq!(asked("Time in Tokyo").as_deref(), Some("Tokyo"));
        assert_eq!(asked("time tokyo").as_deref(), Some("tokyo"));
        assert_eq!(asked("new york time").as_deref(), Some("new york"));

        for not in ["", "time", "timer 5", "lunchtime", "tokyo", "time in"] {
            assert_eq!(asked(not), None, "{not:?} asked about time somewhere");
        }
    }

    #[test]
    fn a_city_is_matched_as_a_whole_word() {
        let zones = fixture();

        assert_eq!(find("paris", &zones).len(), 1);
        assert_eq!(find("PARIS", &zones).len(), 1);
        assert!(find("parish", &zones).is_empty());
        assert!(find("par", &zones).is_empty());
        assert_eq!(find("utc", &zones).len(), 1);
    }

    #[test]
    fn the_keys_own_name_is_a_city_too() {
        let zones = fixture();

        // `Eastern` from `Eastern Standard Time`, which a localised display
        // name would not have carried.
        assert_eq!(find("eastern", &zones).len(), 1);
        assert_eq!(find("tokyo", &zones).len(), 1, "listed once, not twice");
    }

    #[test]
    fn the_difference_is_said_in_hours_and_halves() {
        assert_eq!(said(0), "the same time");
        assert_eq!(said(60), "1 hour ahead");
        assert_eq!(said(540), "9 hours ahead");
        assert_eq!(said(-300), "5 hours behind");
        assert_eq!(said(330), "5 hours 30 minutes ahead");
        assert_eq!(said(-30), "30 minutes behind");
        assert_eq!(said(90), "1 hour 30 minutes ahead");
    }

    #[test]
    fn the_city_comes_back_spelled_as_windows_spells_it() {
        let zones = fixture();
        assert_eq!(titled("paris", &zones[1]), "Paris");
        assert_eq!(titled("somewhere", &zones[1]), "somewhere");
    }

    #[test]
    fn a_city_nobody_knows_is_still_shown_by_name() {
        let zones = fixture();
        let iana = |key: &str| (key == "Tokyo Standard Time").then(|| "Asia/Tokyo".to_string());

        let shown = shown(&["tokyo".to_string(), "Atlantis".to_string()], &zones, iana);

        assert_eq!(shown[0].city, "Tokyo");
        assert_eq!(shown[0].iana.as_deref(), Some("Asia/Tokyo"));
        assert_eq!(shown[1].city, "Atlantis");
        assert_eq!(shown[1].iana, None);
    }

    #[test]
    fn nothing_is_read_unless_asked() {
        let reads = std::cell::Cell::new(0);
        let zones = || {
            reads.set(reads.get() + 1);
            std::sync::Arc::new(Vec::new())
        };

        assert!(matched("notepad", zones).is_empty());
        assert!(matched("time", zones).is_empty());
        assert_eq!(reads.get(), 0);

        matched("tokyo time", zones);
        assert_eq!(reads.get(), 1);
    }
}
