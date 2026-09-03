/**
 * What a view draws instead of its content, and which one it draws.
 *
 * Three things can be true of a pane that has no rows in it: something failed,
 * something is still being read, or there is genuinely nothing to show. They
 * are different answers and a person reading the screen has to be able to tell
 * them apart, because only one of the three is worth waiting for and only one
 * of them is worth doing something about.
 *
 * ## Why the choice is a function rather than an `{#if}` chain
 *
 * The chain was written eleven times and got written wrong. `ActionPanel`
 * tested `actions.length === 0` twice, once above the list and once below it,
 * so an empty panel drew **both** of its empty states at the same time and the
 * words disagreed with each other. `ClipboardView` had no failure branch at
 * all, so a refused `clipboard_search` was drawn as "Nothing copied yet",
 * which is a sentence about the person's clipboard rather than about Sill.
 *
 * One value cannot be two things, so a view that derives its standing from
 * this and switches on the result cannot draw two states at once. That is the
 * property, and it is what the tests hold.
 *
 * ## The order, and why it is this order
 *
 * Failure first. A view clears its own failure when the retry succeeds rather
 * than when it starts, so the last thing that is known to have happened stays
 * on screen until something better is known. A pane that flickers back to
 * "Reading..." and then to the same error has told the reader nothing twice.
 *
 * Then loading, then empty. Loading only wins while there is nothing on
 * screen: once rows are drawn, a later page arriving is an append, and
 * replacing a list somebody is reading with a status line to say more is
 * coming is worse than letting it grow underneath them.
 */

/** Which of the four a view is in. Exactly one, always. */
export type Standing = "failed" | "loading" | "empty" | "content";

/** What a view knows about itself, which is all this needs. */
export interface Reading {
  /** Whether the last read threw. */
  failed: boolean;
  /** Whether a read is in flight. */
  loading: boolean;
  /** How many rows are on screen now. */
  count: number;
}

export function standing({ failed, loading, count }: Reading): Standing {
  if (failed) return "failed";
  if (loading && count === 0) return "loading";
  return count === 0 ? "empty" : "content";
}

/**
 * What a filtered list says when the filter matched nothing.
 *
 * Written from the reader's side. "No results for tada" names the thing they
 * typed, which is the one fact that tells them whether to try again or to try
 * something else; "Query returned empty set" describes the machinery and
 * leaves them to work out that it was their word that did it.
 *
 * No quotation marks around the term, matching the rest of the interface. The
 * sentence already ends there and a pair of quotes inside a line of UI copy
 * reads as punctuation somebody forgot to take out.
 */
export function noMatch(query: string, what = "results"): string {
  const asked = query.trim();
  return asked ? `No ${what} for ${asked}` : `No ${what}`;
}

/**
 * What a failed read says.
 *
 * `what` names the thing the person was trying to do, not the subsystem that
 * refused: "read your clipboard history", never "clipboard_search". They did
 * not ask for a command to succeed, they asked to see their clipboard, and a
 * message naming the command is a message they cannot act on.
 *
 * Same rule and the same wording as `status.ts`, which puts the identical
 * sentence in the tray, so a failure reads the same wherever it is met.
 */
export function couldNot(what: string): string {
  return `Sill could not ${what}`;
}
