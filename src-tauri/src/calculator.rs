//! Arithmetic, units and conversions typed straight into the search field.
//!
//! A launcher that indexes everything on the machine should not make you open
//! a calculator to add two numbers. Typing `1920 * 0.85` puts the answer at
//! the top of the list, and Enter copies it.
//!
//! The evaluation is [`fend_core`], which is MIT and has spent years on the
//! hard parts: arbitrary precision, unit algebra, number bases, dates. What
//! lives here is the part fend has no opinion about and a launcher must get
//! right: **deciding whether a query is a sum at all.** fend will happily
//! read `m` as metres and `notepad` as an error, and neither belongs at the
//! top of a list of search results.

use std::time::{Duration, Instant};

/// A successful evaluation, with the answer already formatted.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// What to show, and what Enter copies.
    pub text: String,
    /// The query it came from, for the row's subtitle.
    pub input: String,
}

/// How long an expression may take before it is abandoned.
///
/// This runs on every keystroke. fend can be asked things that take a very
/// long time (`10^10^10`), and a launcher that stops accepting input while it
/// works them out is broken in a way a slightly late answer is not.
const BUDGET: Duration = Duration::from_millis(40);

/// Evaluates `input`, or returns `None` when it is not a calculation.
///
/// `None` is the common answer: almost everything typed into a launcher is a
/// search. Anything ambiguous is refused rather than guessed at, because a
/// wrong number at the top of the list is worse than no number.
pub fn evaluate(input: &str) -> Option<Answer> {
    let trimmed = input.trim();
    if !looks_like_a_calculation(trimmed) {
        return None;
    }

    let mut context = fend_core::Context::new();
    // fend renders large results in exponential form by default, which is
    // right for a REPL and wrong for a launcher row.
    context.set_exchange_rate_handler_v2(NoRates);

    let deadline = Deadline::new(BUDGET);
    let prepared = apply_percentage_convention(trimmed);
    let result = fend_core::evaluate_with_interrupt(&prepared, &mut context, &deadline).ok()?;

    let text = result.get_main_result().trim().to_string();
    if text.is_empty() || !is_useful(trimmed, &text) {
        return None;
    }

    Some(Answer {
        text,
        input: trimmed.to_string(),
    })
}

/// Whether the input has the shape of a calculation rather than a search.
///
/// This is the whole guard, and it is deliberately strict. Every rule here
/// exists because something in a real index would otherwise turn into a
/// number: version strings, file names with hyphens, Windows paths.
fn looks_like_a_calculation(input: &str) -> bool {
    if input.is_empty() || input.len() > 200 {
        return false;
    }

    // A path is never a sum, however many slashes and dots it has.
    if input.contains(['\\', ':']) || input.starts_with('/') {
        return false;
    }

    let lower = input.to_ascii_lowercase();

    // A conversion says what it is without needing an operator.
    let converts = [" in ", " to ", " as "]
        .iter()
        .any(|word| lower.contains(word))
        && input.chars().any(|c| c.is_ascii_digit());

    if converts {
        return true;
    }

    if !input.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }

    // An operator between two things. A leading minus is a negative number,
    // and a hyphen inside a word is a file name.
    let has_operator = input.contains(['+', '*', '/', '^', '%', '\u{00d7}', '\u{00f7}'])
        || input.trim_start_matches('-').contains('-')
        || input.contains('(');

    if !has_operator {
        return false;
    }

    // `screenshot-2024-08-28` and `v1.2.3-rc1` have operators by this test but
    // are plainly not sums. A calculation's non-numeric parts are operators,
    // spaces, and the occasional unit; a name is mostly letters.
    let letters = input.chars().filter(|c| c.is_alphabetic()).count();
    let digits = input.chars().filter(|c| c.is_ascii_digit()).count();
    letters <= digits.max(3)
}

/// Rewrites a trailing `+ n%` into the percentage of the left-hand side.
///
/// fend reads `120 + 10%` literally, as 120 plus one tenth, and answers
/// 120.1. Every calculator application reads it as a markup and answers 132,
/// which is what someone adding tax or a discount means. This is the one
/// convention fend does not follow that people expect, so it is corrected on
/// the way in rather than by second-guessing the answer on the way out.
///
/// Deliberately narrow. It only fires when the whole input ends in a number
/// followed by `%`, preceded by a top-level `+` or `-`. Anything more
/// involved is handed to fend untouched, because a rewrite that fires on
/// expressions it does not fully understand is worse than not having one.
fn apply_percentage_convention(input: &str) -> String {
    let Some(body) = input.strip_suffix('%') else {
        return input.to_string();
    };

    // The number immediately before the `%`.
    let digits = body
        .trim_end()
        .rfind(|c: char| !c.is_ascii_digit() && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    let (head, amount) = body.split_at(digits);
    if amount.is_empty() || amount.parse::<f64>().is_err() {
        return input.to_string();
    }

    // The operator joining it to everything before.
    let head = head.trim_end();
    let Some(left) = head.strip_suffix(['+', '-']) else {
        return input.to_string();
    };
    let sign = &head[head.len() - 1..];
    let left = left.trim();

    // A bracket anywhere means the structure is beyond what this understands.
    if left.is_empty() || left.contains(['(', ')', '%']) {
        return input.to_string();
    }

    format!("({left}) {sign} ({left}) * {amount}%")
}

/// Whether an answer is worth showing next to the question.
///
/// fend echoes anything it cannot reduce, so `2024` evaluates to `2024`. A
/// row saying a number equals itself is noise.
fn is_useful(input: &str, output: &str) -> bool {
    let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    strip(input) != strip(output)
}

/// Currency conversion needs live rates, and a launcher should not be making
/// network calls on every keystroke. Refusing the rate is what makes fend say
/// it cannot convert, rather than answering with a stale number.
#[derive(Debug, Clone)]
struct NoRates;

impl fend_core::ExchangeRateFnV2 for NoRates {
    fn relative_to_base_currency(
        &self,
        _currency: &str,
        _options: &fend_core::ExchangeRateFnV2Options,
    ) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
        Err("Sill does not fetch exchange rates".into())
    }
}

/// Stops fend once it has had long enough.
struct Deadline {
    until: Instant,
}

impl Deadline {
    fn new(budget: Duration) -> Self {
        Self {
            until: Instant::now() + budget,
        }
    }
}

impl fend_core::Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(input: &str) -> String {
        evaluate(input)
            .unwrap_or_else(|| panic!("{input:?} should evaluate"))
            .text
    }

    #[test]
    fn a_search_is_not_a_sum() {
        // The guard that matters most: everything typed into a launcher is a
        // search until it very clearly is not.
        for query in [
            "notepad",
            "visual studio code",
            "2024",
            "",
            "   ",
            "readme.md",
            "m",
            "settings",
        ] {
            assert!(evaluate(query).is_none(), "{query:?} should not calculate");
        }
    }

    #[test]
    fn things_that_live_in_a_real_index_are_not_sums() {
        // Every one of these has an operator in it by the crude test.
        for query in [
            r"C:\Windows\System32",
            "/usr/local/bin",
            "screenshot-2024-08-28",
            "v1.2.3-rc1",
            "my-project-2024",
            "docker-compose-override",
        ] {
            assert!(evaluate(query).is_none(), "{query:?} should not calculate");
        }
    }

    #[test]
    fn arithmetic_works() {
        assert_eq!(text_of("2 + 3 * 4"), "14");
        assert_eq!(text_of("(2 + 3) * 4"), "20");
        assert_eq!(text_of("1920 * 0.85"), "1632");
        assert_eq!(text_of("10 / 4"), "2.5");
    }

    #[test]
    fn units_convert_which_is_the_reason_to_use_fend() {
        // Hand-rolled arithmetic could never have done these, and they are
        // half of what a launcher calculator is actually for.
        assert!(text_of("100 km to miles").contains("62.13"));
        assert_eq!(text_of("2 GB to MB"), "2000 MB");
        assert!(text_of("180 degrees to radians").contains("3.14159"));
        assert!(text_of("3 hours to minutes").contains("180"));
    }

    #[test]
    fn a_result_keeps_the_base_it_was_asked_in() {
        assert_eq!(text_of("0xff + 1"), "0x100");
        assert_eq!(text_of("255 to hex"), "ff");
        assert_eq!(text_of("10 to binary"), "1010");
    }

    #[test]
    fn the_percentage_phrasings_people_use_work() {
        assert_eq!(text_of("20% of 60"), "12");
        assert_eq!(text_of("120 + 10%"), "132");

        // `50 * 20%` answers "1000%", which is literally correct: fend
        // treats a percentage as a unit and multiplies it through. It reads
        // oddly, but rewriting fend's answer would mean second-guessing the
        // unit algebra that makes everything else here right.
        assert_eq!(text_of("50 * 20%"), "1000%");
    }

    #[test]
    fn a_trailing_percentage_is_a_markup_on_what_came_before() {
        // fend reads these literally as plus or minus one tenth. Every
        // calculator application reads them as a markup, which is what
        // someone adding tax or taking a discount means.
        assert_eq!(text_of("120 + 10%"), "132");
        assert_eq!(text_of("120 - 10%"), "108");
        assert_eq!(text_of("80 * 2 + 25%"), "200");
    }

    #[test]
    fn the_markup_rewrite_leaves_everything_else_alone() {
        // It has to fire on exactly one shape, or it starts mangling
        // expressions it does not understand.
        for input in ["20% of 60", "50 * 20%", "(120 + 10)%", "120 + 10", "5%"] {
            assert_eq!(
                apply_percentage_convention(input),
                input,
                "{input:?} should be handed to fend untouched"
            );
        }
    }

    #[test]
    fn an_answer_that_repeats_the_question_is_not_shown() {
        // fend echoes what it cannot reduce, and a row saying 2024 equals
        // 2024 is noise.
        assert!(evaluate("2024").is_none());
        assert!(evaluate("0xff").is_none());
    }

    #[test]
    fn nonsense_refuses_rather_than_erroring_into_the_list() {
        assert!(evaluate("2 +").is_none());
        assert!(evaluate("frobnicate(2)").is_none());
        assert!(evaluate("1/0").is_none());
    }

    #[test]
    fn a_half_typed_bracket_still_answers() {
        // fend closes an open bracket for you. That is exactly right for a
        // field that re-evaluates on every keystroke: you get an answer
        // while typing `(2 + 3) * 4` rather than nothing until the last
        // character lands.
        assert_eq!(text_of("(2 + 3"), "5");
    }

    #[test]
    fn currency_is_refused_rather_than_answered_with_a_stale_rate() {
        // A launcher should not make a network call on every keystroke, and
        // a wrong exchange rate is worse than none.
        assert!(evaluate("100 USD to EUR").is_none());
    }

    #[test]
    fn a_long_running_expression_gives_up_instead_of_blocking_the_keystroke() {
        // This runs on every character typed. Whatever the answer, it has to
        // come back promptly or not at all.
        let started = Instant::now();
        let _ = evaluate("10^10^10 + 1");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "took {:?}",
            started.elapsed()
        );
    }
}
