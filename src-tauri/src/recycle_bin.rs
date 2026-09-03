//! What is in the recycle bin, and emptying it.
//!
//! ## Why this is not a file action
//!
//! `actions::recycle` puts one file in the bin, and the bin is what makes that
//! undoable: the comment on it says so, and that is why it returns no undo
//! token. Emptying the bin is the other end of the same idea. It is the one
//! thing Sill can do to a file that nothing anywhere can take back, so it is
//! not filed with the file actions at all. It is a row that changes Windows,
//! beside the ones that end a session, and it is asked about first for the
//! same reason they are.
//!
//! ## Why what is in it is read twice
//!
//! Once to ask, so the question can name what is about to go, and once by the
//! empty itself, so the answer can say what was freed. Windows gives no total
//! back from the emptying, and a figure read beforehand and reported afterwards
//! as if it were the result would be a guess dressed as a measurement. Reading
//! it immediately before the call is as close as this gets, and it is honest
//! about being a reading of the bin rather than of what was deleted.

/// What the recycle bin is holding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub bytes: u64,
    pub items: u64,
}

impl Held {
    /// Nothing to delete, so nothing to ask about.
    pub fn is_empty(self) -> bool {
        self.items == 0
    }

    /// What is in there, said the way somebody would say it.
    ///
    /// Both halves, because either alone leaves the question half answered:
    /// "312 items" does not say whether emptying is worth doing and "1.4 GB"
    /// does not say whether it is one download or a year of them.
    pub fn in_words(self) -> String {
        format!(
            "{} in {} item{}",
            size_in_words(self.bytes),
            self.items,
            if self.items == 1 { "" } else { "s" }
        )
    }

    /// What was freed, past tense, for after it has gone.
    pub fn freed(self) -> String {
        format!(
            "Emptied the recycle bin, {} freed",
            size_in_words(self.bytes)
        )
    }
}

/// A size somebody would say out loud.
///
/// Whole units above a megabyte and one decimal place below a gigabyte,
/// because a bin is routinely several gigabytes and "3.7 GB" is the sentence
/// somebody reads. Its own function so the rounding can be tested without a
/// bin, which is the only part of this file that can be.
pub fn size_in_words(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes = bytes as f64;

    if bytes >= GB {
        return format!("{:.1} GB", bytes / GB);
    }

    if bytes >= MB {
        return format!("{:.1} MB", bytes / MB);
    }

    if bytes >= KB {
        return format!("{} KB", (bytes / KB).round() as u64);
    }

    format!("{} bytes", bytes as u64)
}

/// What every drive's bin holds, together.
///
/// A null root asks about all of them, which is what the row means: somebody
/// emptying the recycle bin is not thinking about which volume a file was on.
///
/// A refusal reads as an empty bin rather than as an error. The call fails on
/// a machine with no recycle bin at all, and the honest response to "there is
/// nothing to empty" is to say so rather than to put a Windows error code in
/// front of somebody.
#[cfg(windows)]
pub fn held() -> Held {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };

    // SAFETY: the struct declares its own size, which is what the call reads
    // to know which version it was handed, and a null path asks about every
    // drive rather than one.
    let asked = unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info) };

    if asked.is_err() {
        return Held::default();
    }

    Held {
        bytes: info.i64Size.max(0) as u64,
        items: info.i64NumItems.max(0) as u64,
    }
}

#[cfg(not(windows))]
pub fn held() -> Held {
    Held::default()
}

/// Empties every drive's bin, and says what was in it.
///
/// **Nothing here asks anything.** `system::Asked` is the asking, and this is
/// reached from exactly one place, inside the arm that has already been
/// answered. Called on its own it deletes, which is why it is not called on
/// its own anywhere.
///
/// Windows' own confirmation is suppressed, and that is not an oversight. Sill
/// has already asked, on the row, in the words of the key that answers; a
/// second dialog arriving behind an always-on-top launcher is a dialog nobody
/// can answer, which this codebase has already been bitten by once. The
/// progress window goes with it for the same reason, and the line this returns
/// is what says it is done.
#[cfg(windows)]
pub fn empty() -> Result<Held, String> {
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    // Read immediately before, because it is the only number there will ever
    // be: the call itself gives nothing back but success.
    let was = held();

    // SAFETY: no owner window, a null path meaning every drive, and three
    // documented flags. It borrows nothing this process owns.
    unsafe {
        SHEmptyRecycleBinW(
            None,
            windows::core::PCWSTR::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    }
    .map_err(|err| format!("Windows would not empty the recycle bin: {err}"))?;

    Ok(was)
}

#[cfg(not(windows))]
pub fn empty() -> Result<Held, String> {
    Err("Only Windows has a recycle bin.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_is_said_in_the_unit_somebody_would_use() {
        assert_eq!(size_in_words(0), "0 bytes");
        assert_eq!(size_in_words(512), "512 bytes");
        assert_eq!(size_in_words(2048), "2 KB");
        assert_eq!(size_in_words(3 * 1024 * 1024), "3.0 MB");
        assert_eq!(
            size_in_words(1024 * 1024 * 1024 + 1024 * 1024 * 512),
            "1.5 GB"
        );
    }

    #[test]
    fn one_item_is_not_one_items() {
        let one = Held {
            bytes: 1024,
            items: 1,
        };
        assert_eq!(one.in_words(), "1 KB in 1 item");

        let two = Held {
            bytes: 2048,
            items: 2,
        };
        assert_eq!(two.in_words(), "2 KB in 2 items");
    }

    #[test]
    fn an_empty_bin_says_so_rather_than_reporting_nothing() {
        assert!(Held::default().is_empty());
        assert!(!Held { bytes: 0, items: 3 }.is_empty());
    }

    /// Only meaningful on the machine, so it is ignored. It reads the bin and
    /// does not touch it, which is the half of this file that is safe to run.
    #[test]
    #[ignore]
    fn what_is_in_the_bin_here() {
        let bin = held();
        println!("  {}", bin.in_words());
    }
}
