//! Dates typed straight into the search field.
//!
//! `today + 3 weeks`, `days until 2026-12-25`, `2026-03-01 - 2026-01-15`.
//! fend knows some of this and the calculator's gate refuses the rest, and
//! the two disagree in the worst way: a date minus a date used to reach fend
//! as integer subtraction and answer a number that was not a count of
//! anything. So the small grammar people actually type lives here, in front
//! of the calculator, and answers or stays silent on its own.
//!
//! Pure. The clock is read in one place and handed in, so every case below
//! is a fixture.

use crate::calculator::Answer;
use crate::timers::{civil_from_days, days_from_civil, weekday};

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

impl Civil {
    fn days(self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }

    fn from_days(days: i64) -> Self {
        let (year, month, day) = civil_from_days(days);
        Self { year, month, day }
    }

    fn is_real(self) -> bool {
        (1..=12).contains(&self.month)
            && self.day >= 1
            && self.day <= days_in_month(self.year, self.month)
    }

    /// The same day `months` months on, with the day clamped to the month it
    /// lands in: the 31st of January plus a month is the last day of February,
    /// which is what every calendar application answers.
    fn plus_months(self, months: i64) -> Self {
        let index = self.year * 12 + (self.month - 1) + months;
        let year = index.div_euclid(12);
        let month = index.rem_euclid(12) + 1;
        let day = self.day.min(days_in_month(year, month));
        Self { year, month, day }
    }
}

/// The date on this machine's clock.
#[cfg(windows)]
pub fn today() -> Civil {
    let now = crate::timers::now();
    Civil {
        year: i64::from(now.year),
        month: i64::from(now.month),
        day: i64::from(now.day),
    }
}

#[cfg(not(windows))]
pub fn today() -> Civil {
    Civil {
        year: 1970,
        month: 1,
        day: 1,
    }
}

/// Evaluates `input` as a date sum, or returns `None` when it is not one.
///
/// The gate is the first word: `today`, `tomorrow`, `yesterday`, `days`, or
/// a date written as `YYYY-MM-DD`. Everything else is a search and costs one
/// comparison. A bare date on its own is deliberately not answered, because
/// `2026-09-05` typed alone is at least as likely to be the start of a file
/// name as a question about a Saturday.
pub fn evaluate(input: &str, today: Civil) -> Option<Answer> {
    let trimmed = input.trim();
    let lower = trimmed.to_ascii_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    let first = *words.first()?;
    let opens = matches!(first, "today" | "tomorrow" | "yesterday" | "days") || is_iso(first);
    if !opens {
        return None;
    }

    let text = answer(&words, today)?;

    Some(Answer {
        text,
        input: trimmed.to_string(),
    })
}

/// The shapes understood, tried in order.
fn answer(words: &[&str], today: Civil) -> Option<String> {
    // days until <date>, days since <date>
    if let ["days", word, rest @ ..] = words {
        let target = date_of(rest, today)?;
        let apart = target.days() - today.days();
        return Some(match *word {
            "until" | "till" | "to" => said_from_now(apart),
            "since" | "from" => said_from_now(-apart),
            _ => return None,
        });
    }

    // <date> + <span>, <date> - <span>, <date> - <date>
    let at = words.iter().position(|word| *word == "+" || *word == "-")?;
    let (left, right) = (&words[..at], &words[at + 1..]);
    let start = date_of(left, today)?;
    let subtracting = words[at] == "-";

    if let Some(span) = span_of(right) {
        let landed = if subtracting {
            span.taken_from(start)
        } else {
            span.added_to(start)
        };
        return landed.is_real().then(|| said_date(landed));
    }

    if subtracting {
        let end = date_of(right, today)?;
        let apart = start.days() - end.days();
        return Some(format!("{apart} days"));
    }

    None
}

/// A length of time in whole days or whole months, kept apart because a
/// month is not a number of days.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span {
    Days(i64),
    Months(i64),
}

impl Span {
    fn added_to(self, date: Civil) -> Civil {
        match self {
            Span::Days(days) => Civil::from_days(date.days() + days),
            Span::Months(months) => date.plus_months(months),
        }
    }

    fn taken_from(self, date: Civil) -> Civil {
        match self {
            Span::Days(days) => Civil::from_days(date.days() - days),
            Span::Months(months) => date.plus_months(-months),
        }
    }
}

/// `3 weeks`, `10 days`, `1 month`, `2 years`, also written `3weeks`.
fn span_of(words: &[&str]) -> Option<Span> {
    let (amount, unit) = match words {
        [amount, unit] => (*amount, *unit),
        [joined] => {
            let digits = joined
                .find(|c: char| !c.is_ascii_digit())
                .filter(|at| *at > 0)?;
            joined.split_at(digits)
        }
        _ => return None,
    };

    let amount: i64 = amount.parse().ok().filter(|n| (0..=100_000).contains(n))?;

    Some(match unit {
        "day" | "days" | "d" => Span::Days(amount),
        "week" | "weeks" | "wk" | "wks" | "w" => Span::Days(amount * 7),
        "month" | "months" | "mo" => Span::Months(amount),
        "year" | "years" | "yr" | "yrs" | "y" => Span::Months(amount * 12),
        _ => return None,
    })
}

/// One word naming a date, relative or written out.
fn date_of(words: &[&str], today: Civil) -> Option<Civil> {
    let [word] = words else {
        return None;
    };

    match *word {
        "today" | "now" => Some(today),
        "tomorrow" => Some(Civil::from_days(today.days() + 1)),
        "yesterday" => Some(Civil::from_days(today.days() - 1)),
        _ => parse_iso(word),
    }
}

/// Whether a word has the shape `YYYY-MM-DD`.
fn is_iso(word: &str) -> bool {
    let bytes = word.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(at, byte)| at == 4 || at == 7 || byte.is_ascii_digit())
}

fn parse_iso(word: &str) -> Option<Civil> {
    if !is_iso(word) {
        return None;
    }

    let date = Civil {
        year: word[..4].parse().ok()?,
        month: word[5..7].parse().ok()?,
        day: word[8..].parse().ok()?,
    };

    date.is_real().then_some(date)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
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

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `Friday 25 December 2026`: the weekday first, because it is usually the
/// thing being asked.
fn said_date(date: Civil) -> String {
    let weekday = WEEKDAYS[weekday(date.days()) as usize];
    let month = MONTHS[(date.month - 1) as usize];
    format!("{weekday} {} {month} {}", date.day, date.year)
}

/// A count of days from now, said the way round it happened.
fn said_from_now(days: i64) -> String {
    match days {
        0 => "today".to_string(),
        1 => "1 day".to_string(),
        -1 => "1 day ago".to_string(),
        n if n > 0 => format!("{n} days"),
        n => format!("{} days ago", -n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Saturday, which the tests below lean on.
    const SATURDAY: Civil = Civil {
        year: 2026,
        month: 9,
        day: 5,
    };

    fn said(input: &str) -> Option<String> {
        evaluate(input, SATURDAY).map(|answer| answer.text)
    }

    #[test]
    fn today_plus_three_weeks_lands_on_the_right_weekday() {
        assert_eq!(said("today + 3 weeks").as_deref(), Some("Saturday 26 September 2026"));
        assert_eq!(said("Today + 3weeks").as_deref(), Some("Saturday 26 September 2026"));
    }

    #[test]
    fn days_until_christmas_counts_from_a_fixed_today() {
        assert_eq!(said("days until 2026-12-25").as_deref(), Some("111 days"));
        assert_eq!(said("days till 2026-12-25").as_deref(), Some("111 days"));
        assert_eq!(said("days since 2026-09-01").as_deref(), Some("4 days"));
    }

    #[test]
    fn two_dates_subtract_to_days() {
        assert_eq!(said("2026-03-01 - 2026-01-15").as_deref(), Some("45 days"));
        assert_eq!(said("2026-01-15 - 2026-03-01").as_deref(), Some("-45 days"));
    }

    #[test]
    fn month_arithmetic_clamps_the_day() {
        assert_eq!(
            said("2026-01-31 + 1 month").as_deref(),
            Some("Saturday 28 February 2026")
        );
        assert_eq!(
            said("2024-02-29 + 1 year").as_deref(),
            Some("Friday 28 February 2025")
        );
    }

    #[test]
    fn yesterday_and_tomorrow_are_one_day_either_side() {
        assert_eq!(said("tomorrow + 1 day").as_deref(), Some("Monday 7 September 2026"));
        assert_eq!(said("yesterday - 1 week").as_deref(), Some("Friday 28 August 2026"));
        assert_eq!(said("days until tomorrow").as_deref(), Some("1 day"));
        assert_eq!(said("days until yesterday").as_deref(), Some("1 day ago"));
        assert_eq!(said("days until today").as_deref(), Some("today"));
    }

    #[test]
    fn a_search_is_not_a_date() {
        for not in [
            "",
            "today",
            "tomorrow",
            "days off",
            "today show",
            "2026-09-05",
            "2026-09-05 screenshot",
            "2026-13-01 + 1 day",
            "today + a while",
            "today + 3 fortnights",
            "notepad",
            "days until",
            "today + 1 + 1",
        ] {
            assert!(said(not).is_none(), "{not:?} was read as a date sum");
        }
    }

    #[test]
    fn a_date_that_does_not_exist_is_refused() {
        assert!(said("2026-02-30 + 1 day").is_none());
        assert!(said("days until 2026-04-31").is_none());
    }

    #[test]
    fn the_input_is_kept_as_it_was_typed() {
        let answer = evaluate("  Days Until 2026-12-25 ", SATURDAY).unwrap();
        assert_eq!(answer.input, "Days Until 2026-12-25");
    }

    #[test]
    fn the_weekday_of_the_epoch_was_a_thursday() {
        assert_eq!(WEEKDAYS[weekday(0) as usize], "Thursday");
        assert_eq!(said_date(SATURDAY), "Saturday 5 September 2026");
    }
}
