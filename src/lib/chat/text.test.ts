import { describe, expect, it } from "vitest";

import { healRunOns, seconds } from "./text";

describe("healing sentences that ran together", () => {
  it("puts a space after a full stop the stream swallowed", () => {
    expect(healRunOns("That is real.Grabbing more.")).toBe("That is real. Grabbing more.");
    expect(healRunOns("Done!Next")).toBe("Done! Next");
  });

  it("leaves decimals, paths and markup alone", () => {
    for (const kept of ["1.5 GB", "notes.txt", "a.b.c", "**Bold**.Ok", "C:\\x.Y"]) {
      expect(healRunOns(kept)).toBe(kept);
    }
  });
});

describe("saying how long something took", () => {
  it("rounds to what somebody would say", () => {
    expect(seconds(400)).toBe("a moment");
    expect(seconds(2340)).toBe("2.3 s");
    expect(seconds(12_600)).toBe("13 s");
  });
});
