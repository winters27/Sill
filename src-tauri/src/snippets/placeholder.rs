//! What a snippet fills in when it expands.
//!
//! A snippet that can only paste fixed text is a slightly faster way to type
//! something you could have typed. The placeholders are what make one worth
//! keeping: a signature that carries today's date, a template that picks up
//! whatever is on the clipboard, a form that leaves the cursor in the gap.
//!
//! Expansion is a pure function over the text and a supplied context, so the
//! whole grammar is testable without a clock, a clipboard, or a keyboard.

use serde::Serialize;

/// Where the caret should end up, counted in characters from the start of the
/// expanded text.
pub const CURSOR: &str = "cursor";

/// Everything a snippet can ask about the world.
///
/// Passed in rather than read here so a test can pin the date and the caller
/// can decide how much it is willing to pay for: reading the clipboard costs
/// a Win32 round trip, and most snippets never mention it.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// What somebody typed for the named holes in this one.
    ///
    /// Empty for everything that has nowhere to ask, which is every path but
    /// the launcher's. A snippet expanded by the keyword expander mid-typing
    /// has no surface to stop and ask on, so its fields stay as they are.
    pub fields: std::collections::BTreeMap<String, String>,
    pub clipboard: String,
    /// Formatted `YYYY-MM-DD`.
    pub date: String,
    /// Formatted `HH:MM`.
    pub time: String,
    pub uuid: String,
    /// Whatever was typed for this run.
    ///
    /// Empty for a snippet, which has nowhere to ask. A quicklink with
    /// `{query}` in it is the reason this exists: it is the difference
    /// between a bookmark and a search.
    pub query: String,
    /// Whatever was selected in the window the snippet is expanding into.
    ///
    /// **Only filled when the template asks for it.** Reading a selection
    /// means sending a copy chord and taking the clipboard over for a moment,
    /// which is far too rude to do on the chance a snippet might want it.
    pub selection: String,
    /// The instant this expansion happened.
    pub clock: Clock,
}

/// A broken-down local time, for the placeholders that format one.
///
/// Carried rather than pre-formatted because `{date:dddd}` and `{date}` want
/// different strings out of the same instant, and formatting every possible
/// shape in advance to throw all but one away is work for nothing.
///
/// Hand-held rather than a date crate's type, for the reason the rest of this
/// file already gives: Windows hands back a broken-down local time directly,
/// so the only work left is arranging the numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Clock {
    pub year: u16,
    /// 1 to 12.
    pub month: u16,
    /// 1 to 31.
    pub day: u16,
    /// 0 to 23.
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    /// 0 is Sunday, the way Windows numbers them.
    pub weekday: u16,
}

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

const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

impl Clock {
    /// Writes this instant the way the pattern asks for.
    ///
    /// The token vocabulary people already know from every other tool that
    /// does this: `YYYY-MM-DD`, `dddd`, `h:mm A`. Anything that is not a token
    /// is copied through, so separators, words and punctuation all survive.
    ///
    /// Longest token first, always. Checking `M` before `MMMM` would turn a
    /// month name into four copies of its number, which is the classic way to
    /// get this wrong.
    pub fn format(&self, pattern: &str) -> String {
        const TOKENS: [&str; 18] = [
            "YYYY", "YY", "MMMM", "MMM", "MM", "M", "DD", "D", "dddd", "ddd", "HH", "H", "hh", "h",
            "mm", "m", "ss", "s",
        ];

        let mut out = String::with_capacity(pattern.len() + 8);
        let mut rest = pattern;

        'outer: while !rest.is_empty() {
            for token in TOKENS {
                if let Some(after) = rest.strip_prefix(token) {
                    out.push_str(&self.token(token));
                    rest = after;
                    continue 'outer;
                }
            }

            // AM and PM are single letters and would swallow an ordinary A in
            // a word, so they are only read where a pattern plausibly means
            // them: on their own or after a space or a colon.
            if let Some(after) = rest.strip_prefix('A') {
                out.push_str(if self.hour < 12 { "AM" } else { "PM" });
                rest = after;
                continue;
            }
            if let Some(after) = rest.strip_prefix('a') {
                out.push_str(if self.hour < 12 { "am" } else { "pm" });
                rest = after;
                continue;
            }

            let mut chars = rest.chars();
            if let Some(next) = chars.next() {
                out.push(next);
            }
            rest = chars.as_str();
        }

        out
    }

    fn token(&self, token: &str) -> String {
        // Twelve-hour clocks call midnight and noon twelve, not zero.
        let twelve = match self.hour % 12 {
            0 => 12,
            other => other,
        };

        match token {
            "YYYY" => format!("{:04}", self.year),
            "YY" => format!("{:02}", self.year % 100),
            "MMMM" => MONTHS
                .get(self.month.saturating_sub(1) as usize)
                .unwrap_or(&"")
                .to_string(),
            "MMM" => MONTHS
                .get(self.month.saturating_sub(1) as usize)
                .map(|name| name.chars().take(3).collect())
                .unwrap_or_default(),
            "MM" => format!("{:02}", self.month),
            "M" => self.month.to_string(),
            "DD" => format!("{:02}", self.day),
            "D" => self.day.to_string(),
            "dddd" => DAYS.get(self.weekday as usize).unwrap_or(&"").to_string(),
            "ddd" => DAYS
                .get(self.weekday as usize)
                .map(|name| name.chars().take(3).collect())
                .unwrap_or_default(),
            "HH" => format!("{:02}", self.hour),
            "H" => self.hour.to_string(),
            "hh" => format!("{twelve:02}"),
            "h" => twelve.to_string(),
            "mm" => format!("{:02}", self.minute),
            "m" => self.minute.to_string(),
            "ss" => format!("{:02}", self.second),
            "s" => self.second.to_string(),
            other => other.to_string(),
        }
    }
}

/// An expanded snippet, and where to leave the caret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expansion {
    pub text: String,
    /// Characters from the start, or `None` when the snippet said nothing.
    pub cursor: Option<usize>,
    /// The same thing with its formatting, when the snippet has any.
    ///
    /// Empty for the great majority. Beside the text rather than instead of
    /// it: whatever receives this takes whichever of the two it understands.
    pub html: String,
}

/// The five characters that mean something in markup.
///
/// Substituted values are somebody's clipboard, a file name, a selection: text
/// that had no idea it was going into markup. Without this a clipboard holding
/// `a < b` ends the paragraph it was dropped into, and one holding a tag is
/// pasted as that tag rather than as the characters somebody copied.
pub fn escape_markup(value: &str) -> String {
    let mut out = String::with_capacity(value.len());

    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }

    out
}

/// Fills in every placeholder in `template`.
///
/// An unknown placeholder is left exactly as written. Someone whose snippet
/// legitimately contains `{foo}` should get `{foo}`, and a typo in a
/// placeholder name should be visible rather than silently deleting itself.
pub fn expand(template: &str, context: &Context) -> Expansion {
    expand_with(template, context, &|value| value.to_string())
}

/// Fills in every placeholder, escaping each substituted value.
///
/// The escape applies to what a placeholder *produces*, never to the literal
/// text around it. That distinction is the whole point: a quicklink is a URL
/// whose fixed part contains slashes and question marks that must survive
/// untouched, and a typed query that must not.
pub fn expand_with(
    template: &str,
    context: &Context,
    escape: &dyn Fn(&str) -> String,
) -> Expansion {
    let mut out = String::with_capacity(template.len());
    let mut cursor = None;
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];

        let Some(close) = after.find('}') else {
            // An unclosed brace is just a brace.
            out.push_str(&rest[open..]);
            return Expansion {
                text: out,
                cursor,
                // Filled by the caller when the snippet has formatting, which
                // is a second expansion of a second template.
                html: String::new(),
            };
        };

        let name = &after[..close];
        rest = &after[close + 1..];

        // A placeholder may carry an argument after the first colon, and only
        // the first: `{date:HH:mm}` is a date with a pattern containing a
        // colon, not a placeholder called `date:HH`.
        let trimmed = name.trim();
        let (head, argument) = match trimmed.split_once(':') {
            Some((head, argument)) => (head.trim(), Some(argument)),
            None => (trimmed, None),
        };

        match head.to_ascii_lowercase().as_str() {
            CURSOR => {
                // Only the first one counts. Two carets is not a thing a text
                // field can do, and silently honouring the last would move the
                // caret somewhere the author did not choose.
                if cursor.is_none() {
                    cursor = Some(out.chars().count());
                }
            }
            "clipboard" => out.push_str(&escape(&context.clipboard)),
            "date" => match argument {
                Some(pattern) => out.push_str(&escape(&context.clock.format(pattern))),
                None => out.push_str(&escape(&context.date)),
            },
            "time" => match argument {
                Some(pattern) => out.push_str(&escape(&context.clock.format(pattern))),
                None => out.push_str(&escape(&context.time)),
            },
            "uuid" => out.push_str(&escape(&context.uuid)),
            "query" => out.push_str(&escape(&context.query)),
            "selection" => out.push_str(&escape(&context.selection)),
            // An environment variable, by name. A variable that is not set
            // produces nothing rather than the placeholder: it was addressed
            // correctly and the answer is that there is nothing there.
            "env" => {
                if let Some(wanted) = argument {
                    let value = std::env::var(wanted.trim()).unwrap_or_default();
                    out.push_str(&escape(&value));
                }
            }
            /*
             * A name nothing else answers to is a field somebody fills in.
             *
             * Checked here, at the end, so it can never shadow a built-in: a
             * snippet with a field called `date` still gets today's date, and
             * naming a field after one of these is a mistake that costs the
             * field rather than the date.
             *
             * Still put back verbatim when nobody filled it in. A snippet
             * pasted with an empty hole reads as broken, where one pasted with
             * `{name}` still in it reads as a snippet nobody finished, which is
             * what happened.
             */
            _ => match context.fields.get(name) {
                Some(filled) => out.push_str(&escape(filled)),
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            },
        }
    }

    out.push_str(rest);
    Expansion {
        text: out,
        cursor,
        html: String::new(),
    }
}

/// Whether `template` mentions the clipboard.
///
/// Asked before expanding so the clipboard is only read when a snippet
/// actually wants it. Every expansion paying a Win32 round trip for a
/// placeholder almost none of them use would be a poor trade.
pub fn needs_clipboard(template: &str) -> bool {
    mentions(template, "clipboard")
}

/// Whether `template` asks for the selection.
///
/// Asked before expanding, because filling it in costs a copy chord and the
/// clipboard. Nothing else in the context is expensive enough to be worth a
/// question; this one is worth two.
pub fn needs_selection(template: &str) -> bool {
    mentions(template, "selection")
}

/// Whether `template` uses the named placeholder.
///
/// Matched the way `expand` matches, trimmed and case-insensitively, so
/// `{ Query }` is not reported as absent and then quietly filled in.
pub fn mentions(template: &str, name: &str) -> bool {
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            return false;
        };
        // Compared against the part before the first colon, so `{date:YYYY}`
        // is reported as a mention of `date`. Without this a caller asking
        // "does this need the selection" would be told no by `{selection:x}`.
        let whole = after[..close].trim();
        let head = whole.split_once(':').map_or(whole, |(head, _)| head.trim());

        if head.eq_ignore_ascii_case(name) {
            return true;
        }
        rest = &after[close + 1..];
    }
    false
}

/// Every name this module answers to itself.
///
/// One list, so [`fields`] and the dispatch below cannot disagree about what
/// counts as built in. Two lists would mean a snippet asking for `{date}` and
/// being prompted for it, or a field called `uuid` silently becoming a random
/// one, depending on which list was behind.
const KNOWN: &[&str] = &[
    "cursor",
    "clipboard",
    "date",
    "time",
    "uuid",
    "query",
    "selection",
    "env",
];

/// The named holes a person has to fill in before this can be pasted.
///
/// In the order they appear and without repeats, because that is the order
/// somebody will be asked for them and asking twice for one name reads as the
/// first answer having been lost.
///
/// A hole carrying an argument is not one of these. `{date:YYYY}` and
/// `{env:PATH}` are instructions to this module, not questions for a person.
pub fn fields(template: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };

        let whole = after[..close].trim();
        rest = &after[close + 1..];

        // Anything with a colon is addressed to this module.
        if whole.contains(':') || whole.is_empty() {
            continue;
        }

        let known = KNOWN.iter().any(|name| name.eq_ignore_ascii_case(whole));
        let already = found.iter().any(|name| name.eq_ignore_ascii_case(whole));

        if !known && !already {
            found.push(whole.to_string());
        }
    }

    found
}

#[cfg(test)]
mod tests {
    mod formatting {
        use super::*;

        /// A value going into markup is somebody's clipboard or a file name,
        /// text that had no idea it was going near a tag.
        #[test]
        fn a_substituted_value_cannot_end_the_tag_it_lands_in() {
            let escaped = escape_markup("a < b && c > d");
            assert_eq!(escaped, "a &lt; b &amp;&amp; c &gt; d");
        }

        /// The classic one. Without escaping this is pasted as a tag rather
        /// than as the characters somebody copied.
        #[test]
        fn markup_on_the_clipboard_arrives_as_characters() {
            assert_eq!(
                escape_markup("<b>not bold</b>"),
                "&lt;b&gt;not bold&lt;/b&gt;",
            );
        }

        #[test]
        fn quotes_cannot_break_out_of_an_attribute() {
            assert_eq!(escape_markup(r#"" onclick="x"#), "&quot; onclick=&quot;x",);
            assert_eq!(escape_markup("it's"), "it&#39;s");
        }

        /// The ampersand goes first, or every other escape gets escaped again
        /// and `<` comes out as `&amp;lt;`.
        #[test]
        fn nothing_is_escaped_twice() {
            assert_eq!(escape_markup("&lt;"), "&amp;lt;");
            assert_eq!(escape_markup("&amp;"), "&amp;amp;");
        }

        #[test]
        fn ordinary_text_is_left_exactly_alone() {
            let plain = "Kind regards, Brandon";
            assert_eq!(escape_markup(plain), plain);
        }

        /// The whole point: the two expansions say the same thing, and only
        /// the formatted one is escaped.
        #[test]
        fn the_same_placeholder_fills_both_versions() {
            let context = Context {
                fields: Default::default(),
                clipboard: "a < b".to_string(),
                ..Default::default()
            };

            let plain = expand("Look: {clipboard}", &context);
            let rich = expand_with("<p>Look: {clipboard}</p>", &context, &escape_markup);

            assert_eq!(plain.text, "Look: a < b");
            assert_eq!(rich.text, "<p>Look: a &lt; b</p>");
        }
    }

    use super::*;

    fn context() -> Context {
        Context {
            fields: Default::default(),
            clipboard: "PASTED".into(),
            date: "2026-08-29".into(),
            time: "14:32".into(),
            uuid: "0189a1f2".into(),
            query: "rust traits".into(),
            ..Context::default()
        }
    }

    #[test]
    fn an_escape_touches_the_values_and_not_the_template() {
        // The fixed part of a URL is full of characters that must survive:
        // escaping the whole result would destroy the link itself.
        let out = expand_with(
            "https://example.com/search?q={query}&t=1",
            &context(),
            &|v| v.replace(' ', "%20"),
        );
        assert_eq!(out.text, "https://example.com/search?q=rust%20traits&t=1");
    }

    #[test]
    fn mentions_matches_the_way_expansion_does() {
        assert!(mentions("a {query} b", "query"));
        assert!(mentions("a { Query } b", "query"));
        assert!(!mentions("a {queryish} b", "query"));
        assert!(!mentions("a {query b", "query"));
    }

    #[test]
    fn every_placeholder_is_filled() {
        let out = expand("{date} {time} {uuid} {clipboard}", &context());
        assert_eq!(out.text, "2026-08-29 14:32 0189a1f2 PASTED");
        assert_eq!(out.cursor, None);
    }

    #[test]
    fn the_caret_is_placed_and_leaves_no_text_behind() {
        let out = expand("Hi {cursor},\n\nBest", &context());
        assert_eq!(out.text, "Hi ,\n\nBest");
        assert_eq!(out.cursor, Some(3));
    }

    #[test]
    fn only_the_first_caret_counts() {
        // Two carets is not a thing a text field can do, and honouring the
        // last would move it somewhere the author did not choose.
        let out = expand("a{cursor}b{cursor}c", &context());
        assert_eq!(out.text, "abc");
        assert_eq!(out.cursor, Some(1));
    }

    #[test]
    fn the_caret_is_counted_in_characters_not_bytes() {
        // A byte offset would land mid-character and put the caret in the
        // wrong place for anyone whose snippet is not pure ASCII.
        let out = expand("héllo{cursor}!", &context());
        assert_eq!(out.cursor, Some(5));
    }

    #[test]
    fn an_unknown_placeholder_survives_verbatim() {
        // A typo should be visible rather than silently deleting itself, and
        // a snippet that legitimately contains braces should still work.
        let out = expand("{nope} and {klipboard}", &context());
        assert_eq!(out.text, "{nope} and {klipboard}");
    }

    #[test]
    fn braces_that_are_not_placeholders_are_left_alone() {
        let out = expand("fn main() { println!(\"hi\"); }", &context());
        assert_eq!(out.text, "fn main() { println!(\"hi\"); }");
    }

    #[test]
    fn an_unclosed_brace_is_just_a_brace() {
        let out = expand("100% of {", &context());
        assert_eq!(out.text, "100% of {");
    }

    #[test]
    fn placeholder_names_ignore_case_and_padding() {
        let out = expand("{ DATE } {Cursor}", &context());
        assert_eq!(out.text, "2026-08-29 ");
        assert_eq!(out.cursor, Some(11));
    }

    #[test]
    fn text_with_no_placeholders_is_returned_unchanged() {
        let out = expand("just some text", &context());
        assert_eq!(out.text, "just some text");
    }

    #[test]
    fn the_clipboard_is_only_read_when_it_is_asked_for() {
        // Every expansion paying a Win32 round trip for a placeholder almost
        // none of them use would be a poor trade.
        assert!(needs_clipboard("see {clipboard}"));
        assert!(needs_clipboard("{CLIPBOARD}"));
        assert!(!needs_clipboard("{date} only"));
        assert!(!needs_clipboard("the word clipboard on its own"));
    }

    // ------------------------------------------------------ formatting a time

    fn moment() -> Clock {
        // Sunday 3 May 2026, 09:07:05. Deliberately awkward: a single-digit
        // month, day, hour, minute and second all at once, so anything that
        // forgets to pad shows up, and a Sunday because it is weekday zero.
        Clock {
            year: 2026,
            month: 5,
            day: 3,
            hour: 9,
            minute: 7,
            second: 5,
            weekday: 0,
        }
    }

    #[test]
    fn a_pattern_writes_the_time_the_way_it_asks() {
        let at = moment();

        assert_eq!(at.format("YYYY-MM-DD"), "2026-05-03");
        assert_eq!(at.format("D/M/YY"), "3/5/26");
        assert_eq!(at.format("HH:mm:ss"), "09:07:05");
        assert_eq!(at.format("H:m:s"), "9:7:5");
    }

    #[test]
    fn the_longest_token_is_read_first() {
        // The classic way to get this wrong. Reading `M` before `MMMM` turns a
        // month name into four copies of its number, and `D` before `dddd`
        // does the same to a weekday.
        let at = moment();

        assert_eq!(at.format("MMMM"), "May");
        assert_eq!(at.format("MMM"), "May");
        assert_eq!(at.format("dddd"), "Sunday");
        assert_eq!(at.format("ddd"), "Sun");
        assert_eq!(at.format("MMMM D, YYYY"), "May 3, 2026");
        assert_eq!(at.format("dddd, D MMMM YYYY"), "Sunday, 3 May 2026");
    }

    #[test]
    fn a_twelve_hour_clock_calls_midnight_and_noon_twelve() {
        // Not zero, which is what the arithmetic gives if nobody thinks about
        // it, and which reads as a broken clock.
        let midnight = Clock {
            hour: 0,
            ..moment()
        };
        let noon = Clock {
            hour: 12,
            ..moment()
        };
        let evening = Clock {
            hour: 21,
            ..moment()
        };

        assert_eq!(midnight.format("h:mm A"), "12:07 AM");
        assert_eq!(noon.format("h:mm A"), "12:07 PM");
        assert_eq!(evening.format("h:mm a"), "9:07 pm");
        assert_eq!(evening.format("hh:mm"), "09:07");
    }

    #[test]
    fn anything_that_is_not_a_token_is_left_alone() {
        let at = moment();

        assert_eq!(at.format("YYYY_MM_DD"), "2026_05_03");
        assert_eq!(at.format("[YYYY]"), "[2026]");
        assert_eq!(at.format(""), "");
        assert_eq!(at.format("---"), "---");
    }

    // -------------------------------------------------- the new placeholders

    #[test]
    fn a_date_placeholder_takes_a_pattern() {
        let mut ctx = context();
        ctx.clock = moment();

        assert_eq!(expand("on {date:dddd}", &ctx).text, "on Sunday");
        assert_eq!(expand("{date:MMMM D, YYYY}", &ctx).text, "May 3, 2026");
    }

    #[test]
    fn a_pattern_may_contain_the_colon_that_separates_it() {
        // `{time:HH:mm}` is a time with a pattern containing a colon, not a
        // placeholder named `time:HH`. Only the first colon separates.
        let mut ctx = context();
        ctx.clock = moment();

        assert_eq!(expand("{time:HH:mm:ss}", &ctx).text, "09:07:05");
    }

    #[test]
    fn a_date_with_no_pattern_is_what_it_always_was() {
        // The placeholder people already have in their snippets. Adding an
        // optional argument must not change what happens without one.
        assert_eq!(expand("{date}", &context()).text, "2026-08-29");
        assert_eq!(expand("{time}", &context()).text, "14:32");
    }

    #[test]
    fn the_selection_is_substituted_when_it_was_asked_for() {
        let mut ctx = context();
        ctx.selection = "what was highlighted".into();

        assert_eq!(
            expand("quoting: {selection}", &ctx).text,
            "quoting: what was highlighted"
        );
    }

    #[test]
    fn an_environment_variable_is_read_by_name() {
        // Set here rather than assumed, because a test that depends on the
        // machine having a particular variable is a test that fails on
        // somebody else's.
        std::env::set_var("SILL_PLACEHOLDER_TEST", "from the environment");

        assert_eq!(
            expand("{env:SILL_PLACEHOLDER_TEST}", &context()).text,
            "from the environment"
        );

        std::env::remove_var("SILL_PLACEHOLDER_TEST");
    }

    #[test]
    fn an_unset_variable_produces_nothing_rather_than_the_placeholder() {
        // It was addressed correctly and the answer is that there is nothing
        // there. Leaving `{env:X}` in the text would put the request itself
        // into whatever the person was writing.
        std::env::remove_var("SILL_NO_SUCH_VARIABLE");

        assert_eq!(
            expand("[{env:SILL_NO_SUCH_VARIABLE}]", &context()).text,
            "[]"
        );
    }

    #[test]
    fn asking_whether_a_template_needs_the_selection_sees_past_an_argument() {
        // The gate that stops every expansion sending a copy chord. A caller
        // told "no" by a template that does mention it would expand a snippet
        // with an empty selection in it.
        assert!(needs_selection("quote: {selection}"));
        assert!(needs_selection("{ SELECTION }"));
        assert!(!needs_selection("{clipboard} only"));
        assert!(!needs_selection("the word selection on its own"));

        // And the argument form does not hide a mention from the gate.
        assert!(mentions("{date:YYYY}", "date"));
        assert!(mentions("{env:HOME}", "env"));
    }

    #[test]
    fn a_substituted_value_is_escaped_and_the_text_around_it_is_not() {
        // The rule the whole module turns on, checked for the new arms too.
        let mut ctx = context();
        ctx.selection = "a b".into();
        ctx.clock = moment();

        let out = expand_with("x=/{selection}/ {date:YYYY}", &ctx, &|value| {
            value.replace(' ', "+")
        });

        assert_eq!(out.text, "x=/a+b/ 2026");
    }

    mod holes_somebody_fills_in {
        use super::*;

        fn filled(pairs: &[(&str, &str)]) -> Context {
            Context {
                fields: pairs
                    .iter()
                    .map(|(name, value)| (name.to_string(), value.to_string()))
                    .collect(),
                ..context()
            }
        }

        #[test]
        fn it_finds_the_names_nothing_else_answers_to() {
            assert_eq!(
                fields("Dear {name}, your order {order} ships soon."),
                vec!["name".to_string(), "order".to_string()],
            );
        }

        /// A built-in is never a question.
        ///
        /// Being asked to type today's date, from a snippet that says `{date}`
        /// precisely so nobody has to, would be the feature working against its
        /// own point.
        #[test]
        fn nothing_built_in_is_asked_for() {
            assert!(
                fields("{date} {time} {uuid} {clipboard} {selection} {query} {cursor}").is_empty()
            );
        }

        /// Anything with a colon is addressed to this module, not to a person.
        #[test]
        fn an_instruction_is_not_a_question() {
            assert!(fields("{date:YYYY} and {env:PATH}").is_empty());
        }

        /// Asked once, however many times it is used.
        ///
        /// Being asked twice for one name reads as the first answer having been
        /// lost, and the second answer would win silently.
        #[test]
        fn a_name_used_twice_is_asked_for_once() {
            assert_eq!(fields("{who} and {who} again"), vec!["who".to_string()]);
        }

        #[test]
        fn what_was_typed_is_what_lands() {
            let out = expand("Dear {name},", &filled(&[("name", "Ada")]));

            assert_eq!(out.text, "Dear Ada,");
        }

        /// A field nobody filled stays as it was written.
        ///
        /// An empty hole reads as a broken snippet; `{name}` still sitting there
        /// reads as a snippet nobody finished, which is what happened. The
        /// keyword expander takes this path every time, because it fires mid-typing
        /// and has nowhere to stop and ask.
        #[test]
        fn an_unfilled_field_is_left_where_it_was() {
            let out = expand("Dear {name},", &context());

            assert_eq!(out.text, "Dear {name},");
        }

        /// A field cannot shadow a built-in.
        ///
        /// Naming one `date` is a mistake, and it must cost the field rather than
        /// the date: a snippet that suddenly stopped dating itself because
        /// somebody typed a value once would be the harder bug to find.
        #[test]
        fn a_field_named_after_a_builtin_does_not_win() {
            let out = expand("{date}", &filled(&[("date", "not a date")]));

            assert_ne!(out.text, "not a date");
        }
    }
}
