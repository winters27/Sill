/**
 * What a conversation has cost, said in a pill's worth of characters.
 *
 * Rust adds up: the tokens, the dollars and the speed all arrive on the done
 * event as the conversation's total. This only decides how they are written
 * and, while an answer is still arriving, what to show in the meantime.
 *
 * ## The number that ticks
 *
 * A service says what a request cost once, at the end. A pill that sat still
 * for twelve seconds and then jumped would read as broken, so while the
 * answer arrives the count is the pieces streamed so far, which is usually a
 * token a piece, and the cost is those pieces at the model's output rate.
 * Both are estimates and both are replaced by Rust's number the moment the
 * turn is over. The pill breathes while it is estimating, so the two states
 * look different without a word being spent on it.
 */

import type { AiReady, AiSpent } from "$lib/exthost/commands";

/** The turn in flight, as far as counting is concerned. */
export interface Counting {
  asking: boolean;
  /** Pieces streamed so far this turn. */
  streamed: number;
  /** `performance.now()` at the first piece; zero before one has come. */
  streamBegan: number;
  /** `performance.now()` now, for the rate while it is arriving. */
  now: number;
}

/** What the pill draws. */
export interface Reading {
  tokens: string;
  /** Null when there is no cost to show: a local model, or one unpriced. */
  cost: string | null;
  /** Null unless the model is on this machine and has been timed. */
  rate: string | null;
  /** Whether the figures are the estimate rather than Rust's number. */
  live: boolean;
  /** The sentence on hover, with the counts and where they come from. */
  hint: string;
}

/** A token count, in as few characters as still tell it apart. */
export function tokens(n: number): string {
  if (n < 1000) return `${Math.round(n)}`;
  if (n < 10_000) return `${trimmed(n / 1000)}k`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${trimmed(n / 1_000_000)}M`;
}

/** One decimal, unless it is a zero. */
function trimmed(value: number): string {
  return value.toFixed(1).replace(/\.0$/, "");
}

/**
 * Dollars, to the precision the amount deserves.
 *
 * A launcher question costs a fraction of a cent, so under a dollar three
 * places are the difference between something and nothing. Under a tenth of
 * a cent is said as under, rather than as `$0.000`, which reads as free.
 */
export function dollars(amount: number): string {
  if (amount <= 0) return "$0";
  if (amount < 0.001) return "<$0.001";
  if (amount < 1) return `$${amount.toFixed(3)}`;
  if (amount < 100) return `$${amount.toFixed(2)}`;
  return `$${Math.round(amount)}`;
}

/** Tokens a second, the way somebody says it. */
export function perSecond(rate: number): string {
  return `${rate < 10 ? rate.toFixed(1) : Math.round(rate)} tok/s`;
}

/** A count with its thousands marked, for a sentence rather than a pill. */
function counted(n: number): string {
  return n.toLocaleString("en-US");
}

/** The tokens in and out and how many answers, for the hover sentence. */
function whatWasCounted(spent: AiSpent): string {
  const answers = spent.answers === 1 ? "1 answer" : `${spent.answers} answers`;
  return `${counted(spent.input)} in, ${counted(spent.output)} out over ${answers}`;
}

/**
 * What the pill should say, or nothing when there is nothing to say.
 *
 * Nothing before the first answer and nothing while nobody answers: a pill
 * reading `0` beside an empty conversation is a promise of a bill rather than
 * a reading. It appears with the first piece of the first answer.
 */
export function reading(
  spent: AiSpent | null,
  counting: Counting,
  ready: AiReady | null,
): Reading | null {
  if (!ready?.ready) return null;

  const live = counting.asking && counting.streamed > 0;
  const settled = (spent?.answers ?? 0) > 0;
  if (!settled && !live) return null;

  const local = ready.kind === "local";
  const model = ready.model || ready.name;
  const total = (spent?.input ?? 0) + (spent?.output ?? 0) + (live ? counting.streamed : 0);

  let cost: string | null = null;
  let rate: string | null = null;
  let hint: string;

  if (live) {
    if (!local && (spent?.cost !== null || ready.price)) {
      const arriving = ready.price ? (counting.streamed * ready.price.output) / 1_000_000 : 0;
      cost = dollars((spent?.cost ?? 0) + arriving);
    }
    if (local) {
      const elapsed = counting.now - counting.streamBegan;
      if (counting.streamBegan && elapsed > 400) {
        rate = perSecond(counting.streamed / (elapsed / 1000));
      }
    }
    hint = "Counting as the answer arrives. It settles on the real numbers once it is done.";
  } else {
    // Settled: `spent` is Rust's total and has at least one answer in it.
    const total = spent as AiSpent;
    const counts = whatWasCounted(total);

    if (local) {
      rate = total.rate !== null ? perSecond(total.rate) : null;
      hint = `${counts}. ${ready.name} answers on this machine, so nothing is spent.`;
    } else if (total.cost !== null) {
      cost = dollars(total.cost);
      const short =
        total.unpriced > 0
          ? ` ${total.unpriced} of them could not be priced, so the total is short by that much.`
          : "";
      const rateSaid = ready.price
        ? ` ${model} costs $${ready.price.input.toFixed(2)} in and $${ready.price.output.toFixed(2)} out per million tokens.`
        : ready.kind === "cli"
          ? " Claude Code names what each turn cost."
          : "";
      hint = `${counts}.${rateSaid}${short}`;
    } else {
      hint =
        ready.kind === "cli"
          ? `${counts}. Claude Code did not say what it cost.`
          : `${counts}. No price is known for ${model}, so this counts tokens only.`;
    }
  }

  return { tokens: tokens(total), cost, rate, live, hint };
}
