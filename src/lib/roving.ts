/**
 * Walking a group of choices with the arrow keys.
 *
 * ## Why this exists
 *
 * A radio group is one stop on the Tab order, not one stop per option. The
 * keyboard contract is that Tab lands on whichever option is chosen and the
 * arrows move between them, which is what "roving" names: exactly one option
 * is reachable by Tab at a time, and it is the one that is on.
 *
 * Sill had five groups declaring `role="radiogroup"` and not one of them
 * answered an arrow key, so every option was its own Tab stop and the group
 * behaved like a row of unrelated buttons wearing a group's clothes. A screen
 * reader announces "radio button, 2 of 6" and then the arrows do nothing,
 * which is worse than never having claimed to be a group.
 *
 * ## Why it is a function rather than a component
 *
 * The groups do not look alike. A segmented control is a track with a sliding
 * thumb, the theme picker is six cards each rendering a whole miniature
 * launcher, and the emoji tones are six circles. What they share is which key
 * means which neighbour, and that is the part that was missing from all of
 * them. Wrapping them in one component would have meant one drawing for three
 * designs; a function they each call is the part that is genuinely the same.
 */

/** Which way a group is laid out, because that decides which keys move it. */
export type Along = "row" | "column" | "both";

/**
 * Where an arrow key moves the choice, or `null` if that key means nothing
 * here and the browser should keep it.
 *
 * Wraps at both ends, which is what a radio group does: Right on the last
 * option gives the first. Home and End are absolute, and are the two keys
 * people who cannot see the group rely on to find out how long it is.
 *
 * `null` rather than `at` for a key that does not apply, so a caller can tell
 * "this key moved nothing" from "this key is not ours" and only swallow the
 * second. A group that swallowed Down would take page scrolling with it.
 */
export function rovingTo(key: string, at: number, count: number, along: Along = "row"): number | null {
  if (count <= 0) return null;

  const forward = along === "column" ? ["ArrowDown"] : along === "row" ? ["ArrowRight"] : ["ArrowRight", "ArrowDown"];
  const back = along === "column" ? ["ArrowUp"] : along === "row" ? ["ArrowLeft"] : ["ArrowLeft", "ArrowUp"];

  if (forward.includes(key)) return (at + 1) % count;
  if (back.includes(key)) return (at - 1 + count) % count;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;

  return null;
}

/**
 * The `tabindex` one option carries.
 *
 * Zero on the chosen option and minus one on the rest, so Tab reaches the
 * group once and lands on the answer rather than on the first option.
 *
 * The fallback matters more than it looks: a group whose value matches none of
 * its options would otherwise have every option at minus one and become
 * unreachable by keyboard entirely. When nothing is chosen the first option
 * takes the Tab stop.
 */
export function rovingTab(index: number, chosen: number): 0 | -1 {
  const lands = chosen >= 0 ? chosen : 0;
  return index === lands ? 0 : -1;
}
