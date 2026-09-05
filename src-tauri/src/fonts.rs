//! Installed fonts, found by name.
//!
//! `font mono` lists the monospace faces on this machine, each drawn in
//! itself, and Enter copies the name for the stylesheet or the settings page
//! that wanted it. The list is Windows' own, read through GDI the way every
//! font picker reads it.
//!
//! ## What it costs when nobody asks
//!
//! Nothing. [`asked`] is a comparison against two words, [`matched`] takes
//! the reading as a closure, and the reading is held for ten minutes once
//! taken, so typing `font` twice enumerates once.

use std::time::Duration;

/// How long the list is held. Installing a font is rare and ten minutes late
/// is fine; enumerating on every keystroke of `font` is not.
pub const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

/// How many faces one query shows. Enough to find one, few enough to read.
const MOST_ROWS: usize = 20;

/// Only these words, and only first.
const ASKED_BY: &[&str] = &["font", "fonts", "typeface"];

/// What the query asks for, if it asks for fonts at all.
///
/// `Some("")` is the whole list; `Some("mono")` narrows it. `None` is every
/// other query, which is nearly all of them.
pub fn asked(query: &str) -> Option<&str> {
    let query = query.trim_start();
    let word = query.split_whitespace().next()?;

    if !ASKED_BY.contains(&word.to_ascii_lowercase().as_str()) {
        return None;
    }

    Some(query[word.len()..].trim())
}

/// The faces a query asks for, reading the list only if it does.
///
/// Every word of the filter has to appear somewhere in the name, in any
/// order, so `font ui semi` finds `Segoe UI Semibold`.
pub fn matched(query: &str, read: impl FnOnce() -> std::sync::Arc<Vec<String>>) -> Vec<String> {
    let Some(filter) = asked(query) else {
        return Vec::new();
    };

    let words: Vec<String> = filter
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();

    read()
        .iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            words.iter().all(|word| lower.contains(word))
        })
        .take(MOST_ROWS)
        .cloned()
        .collect()
}

/// The family names worth listing, from whatever GDI handed over.
///
/// GDI reports a family once per character set it covers, and reports the
/// vertical variant of each East Asian face under an `@` prefix. Neither is
/// a font anybody types the name of, so one name per family, in order.
pub fn tidy(raw: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = raw
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty() && !name.starts_with('@'))
        .collect();

    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    names
}

/// Every font family installed on this machine.
#[cfg(windows)]
pub fn installed() -> Vec<String> {
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::Graphics::Gdi::{
        EnumFontFamiliesExW, GetDC, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, TEXTMETRICW,
    };

    unsafe extern "system" fn collect(
        font: *const LOGFONTW,
        _metrics: *const TEXTMETRICW,
        _kind: u32,
        names: LPARAM,
    ) -> i32 {
        // SAFETY: GDI hands a live LOGFONTW for the length of the callback,
        // and the LPARAM is the Vec this enumeration was started with.
        unsafe {
            let names = &mut *(names.0 as *mut Vec<String>);
            let face = &(*font).lfFaceName;
            let end = face.iter().position(|&c| c == 0).unwrap_or(face.len());
            names.push(String::from_utf16_lossy(&face[..end]));
        }
        1
    }

    let mut names: Vec<String> = Vec::new();
    let asking = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };

    // SAFETY: a screen DC is taken and released around one synchronous
    // enumeration, and the callback matches the required signature.
    unsafe {
        let dc = GetDC(None);
        let _ = EnumFontFamiliesExW(
            dc,
            &asking,
            Some(collect),
            LPARAM(&mut names as *mut Vec<String> as isize),
            0,
        );
        let _ = ReleaseDC(None, dc);
    }

    tidy(names)
}

#[cfg(not(windows))]
pub fn installed() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some() -> std::sync::Arc<Vec<String>> {
        std::sync::Arc::new(
            ["Cascadia Mono", "Consolas", "Segoe UI", "Segoe UI Semibold"]
                .iter()
                .map(|name| name.to_string())
                .collect(),
        )
    }

    #[test]
    fn the_word_is_the_gate() {
        assert_eq!(asked("font"), Some(""));
        assert_eq!(asked("Fonts mono"), Some("mono"));
        assert_eq!(asked("typeface segoe"), Some("segoe"));

        for not in ["", "fontawesome", "my font", "notepad"] {
            assert_eq!(asked(not), None, "{not:?} asked for fonts");
        }
    }

    #[test]
    fn every_word_after_it_narrows() {
        assert_eq!(matched("font", some).len(), 4);
        assert_eq!(matched("font mono", some), vec!["Cascadia Mono"]);
        assert_eq!(matched("font ui semi", some), vec!["Segoe UI Semibold"]);
        assert!(matched("font nothing", some).is_empty());
    }

    #[test]
    fn vertical_faces_are_dropped_and_names_are_unique() {
        let raw = vec![
            "Segoe UI".to_string(),
            "@Yu Gothic".to_string(),
            "Yu Gothic".to_string(),
            "Segoe UI".to_string(),
            "segoe ui".to_string(),
            "  ".to_string(),
        ];

        assert_eq!(tidy(raw), vec!["Segoe UI", "Yu Gothic"]);
    }

    #[test]
    fn nothing_is_read_unless_asked() {
        let reads = std::cell::Cell::new(0);
        let read = || {
            reads.set(reads.get() + 1);
            std::sync::Arc::new(Vec::new())
        };

        assert!(matched("notepad", read).is_empty());
        assert!(matched("fontawesome", read).is_empty());
        assert_eq!(reads.get(), 0);

        matched("font", read);
        assert_eq!(reads.get(), 1);
    }
}
