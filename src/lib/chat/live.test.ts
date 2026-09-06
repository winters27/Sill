import { describe, expect, it } from "vitest";

import { begin, fresh, reset, said, settle, textOf, thought, used, using } from "./live";

describe("the turn being written", () => {
  it("counts the pieces as they come, and lets them go with the turn", () => {
    const live = fresh();
    begin(live);
    expect(live.streamed).toBe(0);
    expect(live.streamBegan).toBe(0);

    thought(live, "hm");
    said(live, "Hel");
    said(live, "lo");
    expect(live.streamed).toBe(3);
    expect(live.streamBegan).toBeGreaterThan(0);

    settle(live);
    expect(live.streamed).toBe(0);
    expect(live.streamBegan).toBe(0);
  });

  it("forgets the total with the turn, but not on settling", () => {
    const live = fresh();
    live.spent = { input: 1, output: 1, cost: null, unpriced: 0, rate: null, answers: 1 };
    settle(live);
    expect(live.spent).not.toBeNull();

    reset(live);
    expect(live.spent).toBeNull();
  });

  it("joins words that arrive in pieces into one paragraph", () => {
    const live = fresh();
    begin(live);
    said(live, "Hel");
    said(live, "lo");

    expect(live.parts).toEqual([{ kind: "text", text: "Hello" }]);
    expect(textOf(live.parts)).toBe("Hello");
  });

  it("does not start a paragraph with whitespace alone", () => {
    const live = fresh();
    using(live, { id: "a", tool: "read_file", subject: "x" });
    said(live, "\n");
    using(live, { id: "b", tool: "read_file", subject: "y" });

    expect(live.parts.map((part) => part.kind)).toEqual(["step", "step"]);
  });

  it("keeps words either side of a step apart", () => {
    const live = fresh();
    said(live, "Looking.");
    using(live, { id: "a", tool: "read_file", subject: "x" });
    said(live, "Found it.");

    expect(live.parts.map((part) => part.kind)).toEqual(["text", "step", "text"]);
    expect(textOf(live.parts)).toBe("Looking.Found it.");
  });

  it("joins thinking the same way, apart from the words", () => {
    const live = fresh();
    thought(live, "hm");
    thought(live, "m");
    said(live, "eleven");

    expect(live.parts).toMatchObject([
      { kind: "thinking", text: "hmm" },
      { kind: "text", text: "eleven" },
    ]);
  });

  /// Rust stamps the stored copy; the one on screen is stamped here, since
  /// it is drawn long before Rust's copy is read back.
  it("times the thinking from its first piece to whatever follows", () => {
    const live = fresh();
    thought(live, "hmm");
    expect(live.parts[0]).not.toHaveProperty("ms");

    said(live, "eleven");
    expect(live.parts[0]).toHaveProperty("ms");
    expect((live.parts[0] as { ms: number }).ms).toBeGreaterThanOrEqual(0);

    // A turn that ends while still thinking is stamped when it settles.
    const other = fresh();
    thought(other, "hmm");
    const turn = settle(other);
    expect(turn?.parts[0]).toHaveProperty("ms");
  });

  it("marks a step finished by its id, and ignores one it never saw", () => {
    const live = fresh();
    using(live, { id: "a", tool: "read_file", subject: "x" });
    using(live, { id: "b", tool: "read_file", subject: "y" });
    used(live, { id: "a", ok: false });
    used(live, { id: "nobody", ok: true });

    expect(live.parts[0]).toMatchObject({ id: "a", ok: false });
    expect(live.parts[1]).not.toHaveProperty("ok");
  });

  it("settles into a turn and clears itself", () => {
    const live = fresh();
    begin(live);
    said(live, "eleven");

    const turn = settle(live);

    expect(turn).toEqual({
      role: "assistant",
      text: "eleven",
      parts: [{ kind: "text", text: "eleven" }],
      attachments: [],
    });
    expect(live.parts).toEqual([]);
    expect(live.asking).toBe(false);
  });

  it("settles into nothing when nothing arrived", () => {
    const live = fresh();
    begin(live);
    expect(settle(live)).toBeNull();
    expect(live.asking).toBe(false);
  });

  it("beginning clears the card and the trouble from last time", () => {
    const live = fresh();
    live.asked = { id: "1", title: "Move", subject: "x", touches: "a file" };
    live.trouble = "it broke";
    begin(live);

    expect(live.asked).toBeNull();
    expect(live.trouble).toBe("");
    expect(live.asking).toBe(true);

    reset(live);
    expect(live.asking).toBe(false);
  });
});
