import { describe, expect, it } from "vitest";
import { burst, settled, step } from "$lib/confetti";

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
});
