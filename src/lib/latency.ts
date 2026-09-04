/**
 * How long the window took to answer, measured where the window is.
 *
 * ## Why this cannot be measured in Rust
 *
 * Rust knows when a search was asked for and when it replied. It does not know
 * when the reply reached a screen, and the screen is the half somebody waited
 * for. A launcher that ranks in three milliseconds and draws in ninety is a
 * launcher that feels slow, and every number this project had until now was
 * the three.
 *
 * It is the same argument `timing.rs` makes about a summon, applied to the
 * thing that happens far more often: a summon happens once and then a person
 * types eight characters.
 *
 * ## What is timed, exactly
 *
 * **The clock starts when the field says its value changed** and stops in two
 * places, both of which are recorded:
 *
 * - `keystrokeAnswered` is the moment the rows for that keystroke are in the
 *   document and the frame that will draw them has begun. This is the number
 *   the budget is about, because it is the part Sill does and the part that
 *   has to fit inside a frame.
 * - `keystrokePresented` is the start of the frame after that one, which is
 *   the first moment the pixels are certainly on a screen. It is always about
 *   one refresh longer, and on a sixty hertz display that refresh is sixteen
 *   milliseconds of waiting for the monitor rather than of Sill doing
 *   anything.
 *
 * Both are recorded because either one alone misleads. `answered` on its own
 * quietly leaves the paint out of a number called "to paint". `presented` on
 * its own charges Sill for the display's refresh rate and makes a perfect
 * answer look like a sixteen millisecond one.
 *
 * **What neither includes** is everything before the field heard about the
 * key: the keyboard, the driver, Windows, and WebView2's own input plumbing.
 * Nothing inside the page can see that, and no number here should be read as
 * if it could.
 *
 * ## Why nothing is sent per keystroke
 *
 * One user operation must not become several calls into Rust, and the budget
 * table already allows a keystroke one search and one delayed page. A third
 * call to report on the first two would be the instrumentation making the
 * thing it measures worse.
 *
 * So readings are kept here, bounded, and handed over in one call when the
 * launcher is put away. That is not a hot path: it happens once per visit
 * rather than once per letter, and by then nobody is waiting.
 */

/** What was drawn, and therefore which stopwatch a reading belongs to. */
export type Painted =
  | "keystrokeAnswered"
  | "keystrokePresented"
  | "extensionFirstRender";

/**
 * How many readings of one kind are kept between flushes.
 *
 * A person can type faster than the launcher is put away, so this is the
 * bound that stops a long session from turning a diagnostic into a leak.
 * Sixty-four is several seconds of continuous typing and a few kilobytes.
 *
 * The **oldest** go when it is full, deliberately. The interesting readings
 * are the ones nearest whatever somebody just noticed.
 */
export const MOST_KEPT = 64;

/** Microseconds, per kind, oldest first. */
type Kept = Map<Painted, number[]>;

/**
 * A set of stopwatches and their readings.
 *
 * An object rather than module state, because module state that accumulates
 * is the shape the constitution says not to build and because two of these
 * make the tests independent of each other.
 */
export class Latency {
  private kept: Kept = new Map();

  /**
   * Notes a reading.
   *
   * Rounded to whole microseconds and never negative: a clock that went
   * backwards is a machine that slept, and a negative duration in a
   * diagnostic is worse than a zero.
   */
  record(what: Painted, us: number): void {
    const rounded = Math.max(0, Math.round(us));
    const list = this.kept.get(what) ?? [];

    if (list.length === MOST_KEPT) list.shift();
    list.push(rounded);

    this.kept.set(what, list);
  }

  /** What has been recorded and not yet handed over, per kind. */
  pending(what: Painted): readonly number[] {
    return this.kept.get(what) ?? [];
  }

  /**
   * Everything recorded so far, and forgets it.
   *
   * Empty kinds are left out rather than sent as empty lists: "nothing was
   * measured" and "something was measured and took no time" are different
   * answers and Rust should not have to tell them apart.
   */
  flush(): { what: Painted; tookUs: number[] }[] {
    const out: { what: Painted; tookUs: number[] }[] = [];

    for (const [what, tookUs] of this.kept) {
      if (tookUs.length > 0) out.push({ what, tookUs });
    }

    this.kept.clear();
    return out;
  }
}

/**
 * A frame scheduler, which is `requestAnimationFrame` everywhere real.
 *
 * Taken as an argument rather than reached for, so the rule below can be
 * tested without a browser. The rule is the part that is easy to get wrong
 * and impossible to notice: the difference between one frame and two here is
 * the difference between measuring Sill and measuring the monitor.
 */
export type Frames = (run: () => void) => void;

/**
 * Runs one callback when the current frame is about to draw, and another once
 * it has.
 *
 * A single `requestAnimationFrame` fires *before* the browser styles, lays out
 * and paints that frame, so it says "everything needed is in the document",
 * not "somebody can see it". The frame after it cannot begin until the one
 * before was composited, so a second, nested call is the ordinary way to ask
 * when the pixels went out.
 *
 * Both are offered because a measurement that used only the first would be a
 * keystroke-to-paint number with the paint left out, and one that used only
 * the second would charge Sill for the display's refresh interval.
 */
export function aroundPaint(frames: Frames, answered: () => void, presented: () => void): void {
  frames(() => {
    answered();
    frames(presented);
  });
}
