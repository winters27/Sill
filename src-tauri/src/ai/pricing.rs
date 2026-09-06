//! What a model costs, for the services that do not say.
//!
//! Claude Code names the dollars on its result line, and OpenRouter names
//! them on its usage chunk when asked. Everybody else names tokens and leaves
//! the arithmetic to whoever is paying, so the published rates live here and
//! a turn is priced from them once its counts are in.
//!
//! ## Why a table, when the rule elsewhere is no tables
//!
//! `short_model` refuses a table of pretty names because a name on screen that
//! is merely stale is still the model's name. A price is different: a stale
//! one is a wrong number beside a dollar sign, and the window would rather
//! show tokens alone than a figure it cannot stand behind. So the table is
//! dated, matches only models it has actually seen a price for, and a model
//! it does not know is counted and not priced, with the pill saying so.
//!
//! Rates are the standard tier at the ordinary context length. The surcharge
//! some services add past 200k tokens of context is not modelled: a launcher
//! conversation is bounded at forty turns and never gets there. Cached input
//! is not modelled either, so what is shown is the ceiling, never less than
//! the bill.

use serde::Serialize;

use super::openai::Usage;

/// Dollars per million tokens, in and out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rate {
    pub input: f64,
    pub output: f64,
}

const fn rate(input: f64, output: f64) -> Rate {
    Rate { input, output }
}

/// Every model with a known price, by the start of its id.
///
/// Matched as a prefix, longest match first, so `gpt-5` does not claim
/// `gpt-5-mini` and `claude-opus-4` does not claim `claude-opus-4-6`. A dated
/// snapshot such as `claude-sonnet-4-5-20250929` matches its undated entry.
///
/// Each block says where its numbers came from and when they were read.
const KNOWN: &[(&str, Rate)] = &[
    // Anthropic, first-party rates, read 2026-06-24. OpenRouter bills the same.
    ("claude-fable-5-1", rate(10.0, 50.0)),
    ("claude-mythos-5-1", rate(10.0, 50.0)),
    ("claude-fable-5", rate(10.0, 50.0)),
    ("claude-opus-5", rate(5.0, 25.0)),
    ("claude-opus-4-8", rate(5.0, 25.0)),
    ("claude-opus-4-7", rate(5.0, 25.0)),
    ("claude-opus-4-6", rate(5.0, 25.0)),
    ("claude-opus-4-5", rate(5.0, 25.0)),
    ("claude-sonnet-5", rate(2.0, 10.0)),
    ("claude-sonnet-4-6", rate(3.0, 15.0)),
    ("claude-sonnet-4-5", rate(3.0, 15.0)),
    ("claude-haiku-4-5", rate(1.0, 5.0)),
    // OpenAI, developers.openai.com/api/docs/pricing, read 2026-09-05.
    ("gpt-6-astra", rate(10.0, 50.0)),
    ("gpt-5.6-sol", rate(4.0, 20.0)),
    ("gpt-5.6-terra", rate(2.0, 12.0)),
    ("gpt-5.6-luna", rate(0.2, 1.2)),
    ("gpt-5.5-pro", rate(30.0, 180.0)),
    ("gpt-5.5", rate(5.0, 30.0)),
    ("gpt-5.4-pro", rate(30.0, 180.0)),
    ("gpt-5.4-mini", rate(0.75, 4.5)),
    ("gpt-5.4-nano", rate(0.2, 1.25)),
    ("gpt-5.4", rate(2.5, 15.0)),
    ("gpt-5.2-pro", rate(21.0, 168.0)),
    ("gpt-5.2", rate(1.75, 14.0)),
    ("gpt-5.1", rate(1.25, 10.0)),
    ("gpt-5-pro", rate(15.0, 120.0)),
    ("gpt-5-mini", rate(0.25, 2.0)),
    ("gpt-5-nano", rate(0.05, 0.4)),
    ("gpt-5", rate(1.25, 10.0)),
    ("gpt-4.1-mini", rate(0.4, 1.6)),
    ("gpt-4.1-nano", rate(0.1, 0.4)),
    ("gpt-4.1", rate(2.0, 8.0)),
    ("gpt-4o-mini", rate(0.15, 0.6)),
    ("gpt-4o", rate(2.5, 10.0)),
    ("o4-mini", rate(1.1, 4.4)),
    ("o3-mini", rate(1.1, 4.4)),
    ("o3", rate(2.0, 8.0)),
    ("gpt-3.5-turbo", rate(0.5, 1.5)),
    // xAI, docs.x.ai/docs/models, read 2026-09-05, under 200k of context.
    ("grok-4.6", rate(2.0, 6.0)),
    ("grok-4.5", rate(2.0, 6.0)),
    ("grok-4.3", rate(1.25, 2.5)),
    ("grok-4.20", rate(1.25, 2.5)),
    ("grok-build", rate(1.0, 2.0)),
    // xAI's earlier list, which the page no longer carries. Kept because
    // `grok-4` is the model Sill offers by default and it still answers.
    ("grok-4-fast", rate(0.2, 0.5)),
    ("grok-4", rate(3.0, 15.0)),
    ("grok-3-mini", rate(0.3, 0.5)),
    ("grok-3", rate(3.0, 15.0)),
    ("grok-code-fast-1", rate(0.2, 1.5)),
    // Google, ai.google.dev/gemini-api/docs/pricing, read 2026-09-05. The
    // paid tier at the standard context; the 3.x Flash rates are promotional
    // and double on 2027-01-01.
    ("gemini-3.8-flash", rate(0.75, 3.75)),
    ("gemini-3.7-flash", rate(0.75, 3.75)),
    ("gemini-3.6-flash", rate(0.75, 3.75)),
    ("gemini-3.5-flash-lite", rate(0.3, 2.5)),
    ("gemini-3.5-flash", rate(1.5, 9.0)),
    ("gemini-3.1-flash-lite", rate(0.25, 1.5)),
    ("gemini-3.1-pro", rate(2.0, 12.0)),
    ("gemini-3-flash", rate(0.5, 3.0)),
    ("gemini-3-pro", rate(2.0, 12.0)),
    ("gemini-2.5-flash-lite", rate(0.1, 0.4)),
    ("gemini-2.5-flash", rate(0.3, 2.5)),
    ("gemini-2.5-pro", rate(1.25, 10.0)),
];

/// The published rate for a model, when there is one.
///
/// Takes the id as it is asked for or as the service reported it back. Who
/// published it is not which model it is, so `anthropic/claude-sonnet-5` and
/// `claude-sonnet-5` are the same row, the way `short_model` reads them.
pub fn rate_for(model: &str) -> Option<Rate> {
    let bare = bare(model);

    if bare.is_empty() {
        return None;
    }

    KNOWN
        .iter()
        .filter(|(prefix, _)| bare.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, rate)| *rate)
}

/// What a request cost in dollars, when its model has a known rate.
pub fn cost(model: &str, usage: Usage) -> Option<f64> {
    let rate = rate_for(model)?;
    Some((usage.input as f64 * rate.input + usage.output as f64 * rate.output) / 1_000_000.0)
}

/// The model without its publisher, lowercased.
fn bare(model: &str) -> String {
    let model = model.trim();
    let name = match model.rsplit_once('/') {
        Some((_, name)) if !name.is_empty() => name,
        _ => model,
    };
    name.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one the table is shaped around. `gpt-5` is a prefix of nearly the
    /// whole OpenAI block, and a first-match table would have priced every
    /// mini and nano as the flagship.
    #[test]
    fn the_longest_prefix_wins() {
        assert_eq!(rate_for("gpt-5-mini"), Some(rate(0.25, 2.0)));
        assert_eq!(rate_for("gpt-5"), Some(rate(1.25, 10.0)));
        assert_eq!(rate_for("claude-opus-4-6"), Some(rate(5.0, 25.0)));
        assert_eq!(rate_for("gemini-2.5-flash-lite"), Some(rate(0.1, 0.4)));
        assert_eq!(rate_for("grok-4.6"), Some(rate(2.0, 6.0)));
        assert_eq!(rate_for("grok-4"), Some(rate(3.0, 15.0)));
    }

    /// Who published it is not which model it is.
    #[test]
    fn a_publisher_prefix_is_not_part_of_the_name() {
        assert_eq!(
            rate_for("anthropic/claude-sonnet-5"),
            rate_for("claude-sonnet-5")
        );
        assert_eq!(rate_for("openai/gpt-5.2"), rate_for("gpt-5.2"));
        assert_eq!(rate_for("google/gemini-3-flash"), Some(rate(0.5, 3.0)));
    }

    /// A dated snapshot is the same model at the same price.
    #[test]
    fn a_dated_snapshot_matches_its_undated_row() {
        assert_eq!(
            rate_for("claude-sonnet-4-5-20250929"),
            Some(rate(3.0, 15.0))
        );
        assert_eq!(rate_for("GPT-4o-2024-08-06"), Some(rate(2.5, 10.0)));
    }

    /// A model on somebody's own machine, or one this has never heard of,
    /// is counted and not priced. Guessing would put a wrong number beside
    /// a dollar sign.
    #[test]
    fn a_model_nobody_priced_is_not_priced() {
        assert_eq!(rate_for("qwen3:9b"), None);
        assert_eq!(rate_for("huihui_ai/qwen3.5-abliterated:9b"), None);
        assert_eq!(rate_for("llama3.2"), None);
        assert_eq!(rate_for(""), None);
        assert_eq!(rate_for("something/"), None);
    }

    #[test]
    fn a_turn_is_priced_from_both_counts() {
        // A million in and a million out, at $2 and $6.
        let usage = Usage {
            input: 1_000_000,
            output: 1_000_000,
        };
        assert_eq!(cost("grok-4.6", usage), Some(8.0));

        // Small numbers stay small: a thousand in and a hundred out at
        // sonnet's rates is two tenths of a cent plus a tenth of a cent.
        let usage = Usage {
            input: 1_000,
            output: 100,
        };
        let cost = cost("claude-sonnet-5", usage).expect("priced");
        assert!((cost - 0.003).abs() < 1e-9, "{cost}");

        assert_eq!(cost_of_unknown(), None);
    }

    fn cost_of_unknown() -> Option<f64> {
        cost(
            "qwen3:9b",
            Usage {
                input: 5,
                output: 5,
            },
        )
    }

    /// Two rows for one model would make the price depend on table order.
    #[test]
    fn no_model_is_listed_twice() {
        let mut seen = std::collections::HashSet::new();
        for (prefix, _) in KNOWN {
            assert!(seen.insert(*prefix), "{prefix} is listed twice");
        }
    }

    /// A rate of nothing is a missing row, not a free model.
    #[test]
    fn every_row_names_a_price() {
        for (prefix, rate) in KNOWN {
            assert!(rate.input > 0.0 && rate.output > 0.0, "{prefix} is free");
        }
    }
}
