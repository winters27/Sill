import { describe, expect, it } from "vitest";

import { advance, behind, stride } from "./reveal";

describe("revealing text at a pace", () => {
  it("adds at least three characters a tick", () => {
    expect(stride(1)).toBe(3);
    expect(stride(3)).toBe(3);
  });

  it("catches up faster the further behind it is", () => {
    expect(stride(60)).toBe(10);
    expect(stride(600)).toBe(100);
  });

  it("reaches the target in a bounded number of ticks", () => {
    const target = "x".repeat(2000);
    let shown = "";
    let ticks = 0;

    while (behind(shown, target)) {
      shown = advance(shown, target);
      ticks += 1;
      expect(ticks).toBeLessThan(200);
    }

    expect(shown).toBe(target);
  });

  it("never overshoots the target", () => {
    expect(advance("Hell", "Hello")).toBe("Hello");
    expect(advance("Hello", "Hello")).toBe("Hello");
  });

  /// A regenerate, or a different answer reusing the element: nothing to
  /// catch up to, so it jumps.
  it("jumps when the text was replaced rather than extended", () => {
    expect(advance("Hello there", "Goodbye")).toBe("Goodbye");
  });
});
