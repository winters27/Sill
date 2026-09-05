/*!
Timers, held by the same scheduler the automations are.

## The decision, because the item suggested the other one

`P3-11` proposed timers as an extension, "since they must tick". That was
written before `P8-02`, and `P8-02` changes the answer.

Something does have to tick. The question is whose thread it is. An extension
that survives the launcher window closing is a Node worker resident for as long
as the timer runs: `P4-09` measured an empty one at about 11 MB before it has
done anything, and it would be the first thing in Sill with an `alwaysRunning`
shape, which is the activation model rule 23 names as the one to avoid. It
would also die with Sill, so a reminder set at four would be lost by a restart
at half past.

Windows is already running a scheduler. It survives a restart, it knows about
sleep and battery, it is visible in a tool the person already has, and `P8-02`
built the whole of Sill's side of it. So a timer **is** a scheduled task, and
this module is only the part that turns what somebody typed into one.

**What ticks: the Task Scheduler service, which was running anyway.** What it
costs with no timer set: nothing. There is no task, so there is nothing for the
service to hold, and nothing in Sill is different from a build without this
file. What it costs with a timer pending is also nothing in Sill: the process
can exit and the reminder still arrives, because Windows is the thing waiting.

## What the task runs, and why it is allowed to

`sill.exe run sill.reminder.show <message> --kind reminder`, which is the
command line [`crate::outside`] already answers to, reaching the registry the
same way a keypress does. It passes [`crate::automation::may_schedule`] because
showing a reminder only draws in a window Sill already owns, which is
`Capability::Ui`. Setting one does not pass it, on purpose: writing a scheduled
task changes the machine, so a trigger cannot make more triggers.

## What it leaves behind

Nothing, once it has fired. A one-off task carries an end boundary and
`DeleteExpiredTaskAfter`, so Windows removes it rather than leaving a folder
that fills up with every reminder anybody has ever set. That is the one
difference between this and the daily triggers `P8-02` writes, and it is the
whole reason `When::Once` exists rather than being spelled as a daily trigger
that somebody has to remember to delete.

## What ticks here

Nothing. This module is a parser and some arithmetic. It reads the clock once,
when somebody presses Enter, and holds nothing between one press and the next.
*/

use std::time::Duration;

/// A reminder somebody has described but not yet set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timer {
    /// What it will say when it arrives.
    pub message: String,
    /// How long from the moment it is set.
    pub after: Duration,
}

/// The shortest timer worth setting.
///
/// Below this the task would very likely be registered after the moment it was
/// meant to fire, and a reminder that never arrives is worse than a refusal
/// that says why.
const AT_LEAST: u64 = 10;

/// The longest.
///
/// Thirty days. Past that somebody wants a calendar, and a task sitting in the
/// scheduler for a year is a thing they will find later and not recognise.
const AT_MOST: u64 = 30 * 24 * 60 * 60;

/// What a reminder says when nobody said anything.
const UNNAMED: &str = "Timer";

/**
The words that ask for a timer, and nothing else does.

The first word, exactly, which is the gate `media`, `terminals` and `notes` all
use. Everything after it is this module's own small grammar rather than a
search, so nothing here costs a query that was not asking.
*/
const ASKING: &[&str] = &["remind", "reminder", "timer"];

/// Words that carry no meaning here and are stepped over.
///
/// So that `remind me in 20 minutes to call Sam` and `remind 20m call Sam` are
/// the same sentence. A grammar that only accepted one of them would be a
/// grammar somebody has to learn.
const FILLER: &[&str] = &["me", "in", "to", "that", "about", "at"];

/**
Whether this query is asking for a timer, and which one.

`None` costs one `split_whitespace` and up to three string comparisons, which
is what every query that is not asking pays.

Deliberately strict about the duration. `remind me to call Sam` names no time
and is answered with nothing rather than with a guess: a reminder that arrives
at a moment nobody chose is worse than no row at all, and the row is the only
place the misunderstanding could have been noticed.
*/
pub fn matched(query: &str) -> Option<Timer> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let first = words.first()?.to_lowercase();

    if !ASKING.contains(&first.as_str()) {
        return None;
    }

    let mut at = 1;

    // Filler before the duration: "remind me in ...".
    while at < words.len() && FILLER.contains(&words[at].to_lowercase().as_str()) {
        at += 1;
    }

    let (after, used) = duration_at(&words[at..])?;
    at += used;

    // And filler after it: "... 20 minutes to call Sam".
    while at < words.len() && FILLER.contains(&words[at].to_lowercase().as_str()) {
        at += 1;
    }

    let message = words[at..].join(" ");

    Some(Timer {
        message: if message.is_empty() {
            UNNAMED.to_string()
        } else {
            message
        },
        after,
    })
}

/**
A length of time at the front of these words, and how many words it took.

Three spellings, because all three are things people type: `90` on its own,
`20m` glued together, and `20 minutes` as two words. A bare number is minutes,
which is what a launcher's `timer 5` means everywhere else it exists.

`None` for anything else, including a number outside the bounds. Returning
`Some` with a clamped value would set a timer for a length nobody asked for.
*/
fn duration_at(words: &[&str]) -> Option<(Duration, usize)> {
    let first = words.first()?.to_lowercase();

    /*
     * A bare number is looked at before anything else, and the word after it
     * decides what it means.
     *
     * The other way round was wrong and the tests caught it. `glued` reads a
     * bare `20` as twenty minutes and says it took one word, so
     * `remind me in 20 minutes to call Sam` set a twenty minute timer called
     * "minutes to call Sam", and `timer 1 day` was a minute. The number and
     * its unit are one thing and have to be read as one thing.
     */
    if let Ok(number) = first.parse::<u64>() {
        if let Some(unit) = words
            .get(1)
            .and_then(|next| seconds_per(&next.to_lowercase()))
        {
            return sound(number.checked_mul(unit)?).map(|after| (after, 2));
        }

        // No unit after it, so minutes, which is what `timer 5` means in every
        // launcher that has one. Whatever follows is the message.
        return sound(number.checked_mul(60)?).map(|after| (after, 1));
    }

    // `1h30m`, `20m`, `90s`.
    sound(glued(&first)?).map(|after| (after, 1))
}

/// A run of number-and-unit pairs written as one word, in seconds.
///
/// `1h30m` is two pairs. A bare number never reaches here: whether it is
/// minutes or the first half of `20 minutes` depends on the word after it,
/// which is a decision [`duration_at`] makes because it can see that word.
fn glued(word: &str) -> Option<u64> {
    let mut total: u64 = 0;
    let mut number = String::new();
    let mut saw_a_pair = false;

    for c in word.chars() {
        if c.is_ascii_digit() {
            number.push(c);
            continue;
        }

        if number.is_empty() {
            return None;
        }

        let unit = seconds_per(&c.to_string())?;
        total = total.checked_add(number.parse::<u64>().ok()?.checked_mul(unit)?)?;
        number.clear();
        saw_a_pair = true;
    }

    // A trailing number with no unit, as in `1h30`. Refused rather than
    // guessed at: whether the 30 is minutes or seconds is not knowable, and
    // guessing wrong is a reminder at the wrong time.
    if !number.is_empty() {
        return None;
    }

    saw_a_pair.then_some(total)
}

/// How many seconds one of these is, or `None` for a word that is not a unit.
fn seconds_per(unit: &str) -> Option<u64> {
    Some(match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        _ => return None,
    })
}

/// Whether a length of time is one this will set.
fn sound(seconds: u64) -> Option<Duration> {
    (AT_LEAST..=AT_MOST)
        .contains(&seconds)
        .then(|| Duration::from_secs(seconds))
}

/// How long it is, in the words somebody would use.
///
/// For the row, which has to say what pressing Enter will do before it is
/// pressed. Rounded to the largest unit that divides it, because "in 90
/// minutes" reads worse than "in 1 hour 30 minutes" and "in 5400 seconds"
/// reads like a machine.
pub fn said(after: Duration) -> String {
    let total = after.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);

    let mut parts = Vec::new();

    for (count, one) in [(hours, "hour"), (minutes, "minute"), (seconds, "second")] {
        if count == 0 {
            continue;
        }
        parts.push(format!(
            "{count} {one}{}",
            if count == 1 { "" } else { "s" }
        ));
    }

    parts.join(" ")
}

/// A moment on a local wall clock, which is what Task Scheduler reads.
///
/// `Copy` and plain integers so that [`crate::automation::When`] can hold one
/// and stay `Copy` itself, and serialisable because a trigger crosses to the
/// window as JSON like everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Local {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/**
When a timer set now will fire, as the text a task's start boundary holds.

Pure, taking the clock rather than reading it, so every case below is a
fixture rather than a statement about what time the test ran.

**Local wall clock, with the caveat that names itself.** Task Scheduler reads a
boundary with no zone on it as local time, so adding twenty minutes to the wall
clock is exactly right twenty minutes from now. It is an hour out for a timer
that spans the two moments a year when the clocks move, which is the honest
cost of the format Windows wants and is not worth an hours-long timer being
spelled a second way to avoid.
*/
pub fn fires_at(now: Local, after: Duration) -> Local {
    let days = days_from_civil(now.year as i64, now.month as i64, now.day as i64);
    let seconds =
        days * 86_400 + now.hour as i64 * 3600 + now.minute as i64 * 60 + now.second as i64;

    let then = seconds + after.as_secs() as i64;
    let (day_of, rest) = (then.div_euclid(86_400), then.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(day_of);

    Local {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        hour: (rest / 3600) as u8,
        minute: ((rest % 3600) / 60) as u8,
        second: (rest % 60) as u8,
    }
}

impl Local {
    /// The spelling `<StartBoundary>` and `<EndBoundary>` take.
    pub fn boundary(self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// The time of day, for a row that says when something will happen.
    pub fn clock(self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }

    /// Whether this is a moment that exists.
    ///
    /// Checked because a `Local` can arrive from the window as JSON, where
    /// nothing about the type says a month is at most twelve. A boundary
    /// naming the 32nd of September registers as a task that then never runs,
    /// which is the worst failure this feature has: silent, and only noticed
    /// by the reminder not arriving.
    pub fn is_a_moment(self) -> bool {
        let days_in = match self.month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                let year = self.year as i64;
                if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            _ => return false,
        };

        (1..=days_in).contains(&self.day) && self.hour < 24 && self.minute < 60 && self.second < 60
    }
}

/*
 * Days between 1970-01-01 and a date, and back.
 *
 * Howard Hinnant's algorithms, which are the short exact ones rather than a
 * table of month lengths and a leap year rule written out again. They are here
 * rather than pulled in as a dependency because this is the only arithmetic on
 * dates in the project and a crate for six lines is a crate to keep updated.
 *
 * Written as free functions taking plain integers so every case in the tests
 * below is a fixture.
 */

/// Days since 1970-01-01 for a civil date.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

/// The day of the week a count of days since 1970-01-01 lands on, Sunday
/// being zero. The epoch was a Thursday, which is the four.
pub(crate) fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

/// The civil date a count of days since 1970-01-01 lands on.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };

    (year + i64::from(month <= 2), month, day)
}

/// What the clock says right now, on this machine, in local time.
///
/// The one impure line in the module, and it is deliberately one line: every
/// decision above is made on a `Local` that a test can hand over.
#[cfg(windows)]
pub fn now() -> Local {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    // SAFETY: fills in a stack SYSTEMTIME and cannot fail.
    let at = unsafe { GetLocalTime() };

    Local {
        year: at.wYear,
        month: at.wMonth as u8,
        day: at.wDay as u8,
        hour: at.wHour as u8,
        minute: at.wMinute as u8,
        second: at.wSecond as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(timer: &Timer) -> u64 {
        timer.after.as_secs()
    }

    /// Only these three words, and only first.
    #[test]
    fn the_word_is_the_gate() {
        for asking in ["timer 20m", "remind me in 20m tea", "reminder 20m tea"] {
            assert!(
                matched(asking).is_some(),
                "{asking} does not ask for a timer"
            );
        }

        for not in [
            "",
            "timers",
            "reminding",
            "set a timer for 20m",
            "20m",
            "notepad",
            "1 + 1",
        ] {
            assert_eq!(matched(not), None, "{not} was read as a timer");
        }
    }

    /// The three spellings of a length of time people actually type.
    #[test]
    fn a_length_of_time_can_be_written_three_ways() {
        assert_eq!(
            secs(&matched("timer 20").expect("bare is minutes")),
            20 * 60
        );
        assert_eq!(
            secs(&matched("timer 20 eggs").expect("a unit it does not know")),
            20 * 60,
            "a word that is not a unit is the message, and the number is minutes"
        );
        assert_eq!(secs(&matched("timer 20m").expect("glued")), 20 * 60);
        assert_eq!(
            secs(&matched("timer 20 minutes").expect("two words")),
            20 * 60
        );
        assert_eq!(secs(&matched("timer 90s").expect("seconds")), 90);
        assert_eq!(secs(&matched("timer 2h").expect("hours")), 2 * 3600);
        assert_eq!(secs(&matched("timer 1h30m").expect("two pairs")), 5400);
        assert_eq!(secs(&matched("timer 1 day").expect("days")), 86_400);
    }

    /// Filler is stepped over on both sides of the duration.
    #[test]
    fn the_same_sentence_can_be_said_several_ways() {
        let wanted = Timer {
            message: "call Sam".to_string(),
            after: Duration::from_secs(20 * 60),
        };

        for said in [
            "remind me in 20 minutes to call Sam",
            "remind in 20m call Sam",
            "remind 20m call Sam",
            "reminder me at 20m that call Sam",
            "timer 20m call Sam",
        ] {
            assert_eq!(matched(said).as_ref(), Some(&wanted), "{said}");
        }
    }

    /// The message is left exactly as it was typed, filler inside it included.
    ///
    /// Only the words before the message are stepped over. A reminder saying
    /// "tell me about the thing" must not arrive saying "tell the thing".
    #[test]
    fn the_message_keeps_every_word_of_itself() {
        let timer = matched("remind me in 5m to tell Ana about the invoice").expect("a timer");
        assert_eq!(timer.message, "tell Ana about the invoice");
    }

    /// No time named is no timer, rather than a timer at a time nobody chose.
    #[test]
    fn a_reminder_with_no_time_in_it_is_refused() {
        assert_eq!(matched("remind me to call Sam"), None);
        assert_eq!(matched("timer"), None);
        assert_eq!(matched("remind me in"), None);
        // `1h30` is ambiguous: 30 of what? Refused rather than guessed.
        assert_eq!(matched("timer 1h30"), None);
        assert_eq!(matched("timer twenty minutes"), None);
    }

    /// A length outside the bounds is refused, not clamped.
    #[test]
    fn a_length_nobody_would_mean_is_refused_rather_than_rounded() {
        assert_eq!(matched("timer 1s"), None, "too short to register in time");
        assert_eq!(matched("timer 400 days"), None, "past a month");
        assert_eq!(matched("timer 0"), None);
        assert!(matched("timer 10s").is_some(), "the shortest one allowed");
        assert!(
            matched("timer 30 days").is_some(),
            "the longest one allowed"
        );
        // Big enough to overflow the multiplication rather than the bound.
        assert_eq!(matched("timer 99999999999999999999 hours"), None);
    }

    /// A timer with nothing to say still says something.
    #[test]
    fn a_timer_with_no_message_is_still_a_timer() {
        assert_eq!(matched("timer 5m").expect("a timer").message, UNNAMED);
    }

    /// The row has to say what will happen before Enter is pressed.
    #[test]
    fn a_length_of_time_is_said_the_way_somebody_would_say_it() {
        assert_eq!(said(Duration::from_secs(60)), "1 minute");
        assert_eq!(said(Duration::from_secs(20 * 60)), "20 minutes");
        assert_eq!(said(Duration::from_secs(5400)), "1 hour 30 minutes");
        assert_eq!(said(Duration::from_secs(90)), "1 minute 30 seconds");
        assert_eq!(said(Duration::from_secs(2 * 3600)), "2 hours");
    }

    fn at(year: u16, month: u8, day: u8, hour: u8, minute: u8) -> Local {
        Local {
            year,
            month,
            day,
            hour,
            minute,
            second: 0,
        }
    }

    /// Adding time to a clock, including over every edge that has ever been
    /// got wrong by hand.
    #[test]
    fn a_timer_lands_on_the_right_moment() {
        // Ordinary.
        assert_eq!(
            fires_at(at(2026, 9, 4, 14, 15), Duration::from_secs(20 * 60)).boundary(),
            "2026-09-04T14:35:00",
        );
        // Over midnight, and so over a month end.
        assert_eq!(
            fires_at(at(2026, 9, 30, 23, 50), Duration::from_secs(20 * 60)).boundary(),
            "2026-10-01T00:10:00",
        );
        // Over a year end.
        assert_eq!(
            fires_at(at(2026, 12, 31, 23, 59), Duration::from_secs(120)).boundary(),
            "2027-01-01T00:01:00",
        );
        // The 29th of February exists in 2028 and does not in 2027.
        assert_eq!(
            fires_at(at(2028, 2, 28, 12, 0), Duration::from_secs(86_400)).boundary(),
            "2028-02-29T12:00:00",
        );
        assert_eq!(
            fires_at(at(2027, 2, 28, 12, 0), Duration::from_secs(86_400)).boundary(),
            "2027-03-01T12:00:00",
        );
        // 2100 is not a leap year, which is the rule a hand-written one drops.
        assert_eq!(
            fires_at(at(2100, 2, 28, 12, 0), Duration::from_secs(86_400)).boundary(),
            "2100-03-01T12:00:00",
        );
        // 2000 is, which is the rule the correction to that rule drops.
        assert_eq!(
            fires_at(at(2000, 2, 28, 12, 0), Duration::from_secs(86_400)).boundary(),
            "2000-02-29T12:00:00",
        );
    }

    /// The calendar arithmetic round-trips over a long stretch of days.
    ///
    /// Cheaper than trusting two algorithms that were typed in, and it is the
    /// pair being inverses that everything above rests on.
    #[test]
    fn every_day_for_forty_years_reads_back_as_itself() {
        for days in -3_650..=11_000 {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(
                days_from_civil(year, month, day),
                days,
                "{year:04}-{month:02}-{day:02} does not read back"
            );
        }
    }

    /// A moment that does not exist is not a moment.
    ///
    /// The check is on the way into a task's XML, where the cost of getting it
    /// wrong is a task that registers happily and then never fires.
    #[test]
    fn a_date_that_does_not_exist_is_refused() {
        assert!(at(2026, 9, 4, 14, 15).is_a_moment());
        assert!(at(2028, 2, 29, 0, 0).is_a_moment(), "2028 is a leap year");

        assert!(!at(2027, 2, 29, 0, 0).is_a_moment(), "2027 is not");
        assert!(!at(2100, 2, 29, 0, 0).is_a_moment(), "nor is 2100");
        assert!(at(2000, 2, 29, 0, 0).is_a_moment(), "2000 is");
        assert!(!at(2026, 9, 31, 0, 0).is_a_moment(), "September has 30");
        assert!(!at(2026, 13, 1, 0, 0).is_a_moment());
        assert!(!at(2026, 0, 1, 0, 0).is_a_moment());
        assert!(!at(2026, 9, 0, 0, 0).is_a_moment());
        assert!(!at(2026, 9, 4, 24, 0).is_a_moment());
        assert!(!at(2026, 9, 4, 0, 60).is_a_moment());
    }

    /// Every moment this module can produce is one that exists.
    ///
    /// The two halves have to agree: `fires_at` builds the boundary and
    /// `is_a_moment` is what `automation::When::sound` refuses one by, so a
    /// disagreement is a timer that Sill refuses to set for a reason it
    /// invented itself.
    #[test]
    fn a_moment_this_arrives_at_is_always_a_moment() {
        let start = at(2026, 1, 1, 0, 0);

        for hours in 0..(400 * 24) {
            let landed = fires_at(start, Duration::from_secs(hours * 3600));
            assert!(
                landed.is_a_moment(),
                "{} is not a moment",
                landed.boundary()
            );
        }
    }

    /// The row says a time of day, because "in 20 minutes" is not checkable
    /// against a clock on the wall and "14:35" is.
    #[test]
    fn the_moment_is_said_as_a_time_of_day() {
        assert_eq!(
            fires_at(at(2026, 9, 4, 14, 15), Duration::from_secs(20 * 60)).clock(),
            "14:35",
        );
    }
}
