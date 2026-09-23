/**
 * When a key may answer an approval card.
 *
 * A card arrives on its own schedule: a turn running in another window, an
 * extension asking mid-command, a model that decided to act. The person it
 * lands in front of is usually typing, and the key they are about to press was
 * meant for the text. Enter is also the key that allows. Without a pause, the
 * next Enter after a card appears approves an action nobody read.
 *
 * So Enter allows only a card that has been on screen for `SETTLES_MS`, and
 * only from a fresh, unshifted, uncomposed keypress. Escape refuses at once:
 * refusing early is the safe direction, and a card that cannot be dismissed
 * quickly is its own kind of trap.
 *
 * Three places answer cards with Enter (the launcher's permission card, the
 * launcher's conversation, and the chat window's composer) and all three ask
 * this, so the rule is written once.
 */

/**
 * How long a card must have been on screen before Enter can allow it.
 *
 * Longer than the gap between two typed keys, so a keystroke already on its
 * way cannot land on the card, and shorter than it takes to read one line.
 */
export const SETTLES_MS = 700;

export type CardAnswer = "allow" | "refuse" | null;

/**
 * When each card was first seen by this window.
 *
 * Keyed on the card object itself, so a card that is answered and dropped is
 * forgotten with it, and the same card read again from Rust by a window that
 * missed the event counts as newly shown there, which it is.
 */
const seen = new WeakMap<object, number>();

/** When this card was first on screen, recording now if it never was. */
export function shownAt(card: object, now: number): number {
  const at = seen.get(card);
  if (at !== undefined) return at;

  seen.set(card, now);
  return now;
}

/** Whether the card has been up long enough for Enter to allow it. */
export function settled(card: object, now: number): boolean {
  return now - shownAt(card, now) >= SETTLES_MS;
}

/**
 * What this key does to the card, or `null` for nothing.
 *
 * `null` for an Enter that came too soon still means the caller swallows it:
 * the card is modal, and an Enter that neither allows nor falls through to the
 * row or the draft underneath is the only safe reading of a key meant for
 * something else.
 */
export function answersCard(
  event: Pick<KeyboardEvent, "key" | "shiftKey" | "repeat" | "isComposing">,
  card: object,
  now: number,
): CardAnswer {
  if (event.isComposing) return null;
  if (event.key === "Escape") return "refuse";
  if (event.key !== "Enter" || event.shiftKey || event.repeat) return null;

  return settled(card, now) ? "allow" : null;
}
