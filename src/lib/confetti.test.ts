import { describe, expect, it } from "vitest";
import { burst, flip, settled, step } from "$lib/confetti";

/** A deterministic stand-in for `Math.random`, so a failure repeats. */
function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1_664_525 + 1_013_904_223) % 4_294_967_296;
    return state / 4_294_967_296;
  };
}

describe("confetti", () => {
  it("every piece eventually leaves the screen", () => {
    const pieces = burst(1920, 1080, 200, seeded(7));

    let seconds = 0;
    while (!settled(pieces, 1080) && seconds < 10) {
      step(pieces, 1 / 60);
      seconds += 1 / 60;
    }

    expect(settled(pieces, 1080)).toBe(true);
    // Over in a few seconds, or it is a screensaver rather than a burst.
    expect(seconds).toBeLessThan(6);
  });

  it("starts at the bottom corners and rises before it falls", () => {
    const pieces = burst(1000, 800, 20, seeded(1));

    for (const piece of pieces) {
      expect(piece.y).toBe(800);
      expect(piece.x === 0 || piece.x === 1000).toBe(true);
      expect(piece.vy).toBeLessThan(0);
    }
  });

  it("is not settled while a piece is still on screen", () => {
    const pieces = burst(1000, 800, 20, seeded(3));
    step(pieces, 0.2);
    expect(settled(pieces, 800)).toBe(false);
  });

  it("never lets paper fall faster than terminal velocity", () => {
    const pieces = burst(1920, 1080, 200, seeded(11));

    for (let frame = 0; frame < 600; frame += 1) {
      step(pieces, 1 / 60);
      for (const piece of pieces) {
        if (piece.paper) expect(piece.vy).toBeLessThanOrEqual(piece.terminal);
      }
    }
  });

  /**
   * The cap is a `min`, and a clamp there would be silent: the
   * burst still looks like a burst, it just never gets more than a few
   * pixels off the bottom, because every launch is pulled straight down to
   * terminal velocity at once.
   */
  it("still throws paper well clear of the bottom", () => {
    const pieces = burst(1920, 1080, 200, seeded(13));
    let highest = 1080;

    for (let frame = 0; frame < 120; frame += 1) {
      step(pieces, 1 / 60);
      for (const piece of pieces) highest = Math.min(highest, piece.y);
    }

    expect(highest).toBeLessThan(1080 / 2);
  });

  /**
   * Without a per-piece phase every piece at the same height is edge-on at
   * the same instant and the whole burst blinks in unison.
   *
   * Forced to one height on purpose. The first version of this stepped a
   * burst and checked the phases differed, and it passed with the seed
   * deleted: a burst throws pieces at different speeds, so they are already
   * at different heights and `cos(y)` differs between them for a reason
   * that has nothing to do with the seed. Sharing a height is the one case
   * the seed is for, so it is the case worth constructing.
   */
  it("gives every piece a phase of its own", () => {
    const pieces = burst(1920, 1080, 60, seeded(17)).filter((piece) => piece.paper);
    for (const piece of pieces) piece.y = 500;

    const phases = new Set(pieces.map((piece) => Math.round(flip(piece) * 50)));
    expect(phases.size).toBeGreaterThan(pieces.length / 3);
  });
  /** The sizes this has to look the same on. */
  const SCREENS: [number, number][] = [
    [1366, 768],
    [1920, 1080],
    [2560, 1440],
    [3840, 2160],
  ];

  it("clears the screen in the same time whatever the screen is", () => {
    const times = SCREENS.map(([wide, tall]) => {
      const pieces = burst(wide, tall, 200, seeded(23));
      let seconds = 0;

      while (!settled(pieces, tall) && seconds < 12) {
        step(pieces, 1 / 60);
        seconds += 1 / 60;
      }

      expect(settled(pieces, tall), `${wide}x${tall} never cleared`).toBe(true);
      return seconds;
    });

    for (const seconds of times) expect(seconds).toBeLessThan(6);

    // Within a fifth of a second of each other, which is the whole claim:
    // the burst is measured in screens rather than in pixels, so a 4K
    // display does not sit through twice the fall a laptop does.
    expect(Math.max(...times) - Math.min(...times)).toBeLessThan(0.2);
  });

  it("throws every piece at least half way up, on any screen", () => {
    for (const [wide, tall] of SCREENS) {
      const pieces = burst(wide, tall, 200, seeded(29)).filter((piece) => piece.paper);

      for (const piece of pieces) {
        // Where it would peak on the launch alone: v squared over 2g.
        const apex = (piece.vy * piece.vy) / (2 * piece.gravity);
        expect(apex, `${wide}x${tall} threw one only ${Math.round(apex)}px`).toBeGreaterThan(tall / 2);
      }
    }
  });
});
