/**
 * Revealing streamed text at a steady pace instead of in bursts.
 *
 * Tokens do not arrive smoothly. They come in clumps whenever a packet lands,
 * so drawing them the instant they arrive makes a reply lurch: a paragraph
 * at once, then nothing, then another. What arrived is kept as a target and
 * what is on screen catches up to it, which is what makes a reply read as
 * though it is being written.
 *
 * The step scales with how far behind the screen is, so a burst is absorbed
 * within a few ticks and the text never drifts noticeably behind. One tick
 * every `--motion-reveal` rather than every frame: each tick re-parses the
 * whole answer, and at a few kilobytes doing that sixty times a second missed
 * frames. Twenty-five a second still reads as writing.
 *
 * Pure. The component owns the one timeout; this says what the next tick
 * shows and whether there needs to be one.
 */

/** How many characters the next tick adds, for how far behind it is. */
export function stride(behind: number): number {
  return Math.max(3, Math.ceil(behind / 6));
}

/**
 * What to show next.
 *
 * Text that was replaced rather than extended, which is a different answer
 * reusing the element, jumps rather than replaying: `shown` is no longer a
 * prefix of `target`, so there is nothing to catch up to.
 */
export function advance(shown: string, target: string): string {
  if (!target.startsWith(shown)) return target;
  if (shown.length >= target.length) return target;
  return target.slice(0, shown.length + stride(target.length - shown.length));
}

/** Whether there is anything left to reveal. */
export function behind(shown: string, target: string): boolean {
  return shown !== target;
}
