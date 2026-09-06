import { describe, expect, it } from "vitest";

import type { AiPart } from "$lib/exthost/commands";
import { fromQuestion, fromTurn, groupParts } from "./parts";

const step = (id: string): AiPart => ({ kind: "step", id, tool: "read_file", subject: id });

describe("grouping an answer's parts into blocks", () => {
  it("draws consecutive steps as one timeline", () => {
    const blocks = groupParts([step("a"), step("b"), { kind: "text", text: "done" }]);

    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toMatchObject({ kind: "steps", at: 0 });
    expect((blocks[0] as { steps: unknown[] }).steps).toHaveLength(2);
    expect(blocks[1]).toEqual({ kind: "text", at: 2, text: "done" });
  });

  it("keeps steps either side of words as two timelines", () => {
    const blocks = groupParts([step("a"), { kind: "text", text: "so" }, step("b")]);
    expect(blocks.map((block) => block.kind)).toEqual(["steps", "text", "steps"]);
  });

  /// The key is where the block came from, not where it sits: a step
  /// arriving mid-answer must not move the paragraph after it.
  it("keys each block by the part it began at", () => {
    const blocks = groupParts([
      { kind: "thinking", text: "hmm", ms: 40 },
      step("a"),
      { kind: "text", text: "eleven" },
    ]);

    expect(blocks.map((block) => block.at)).toEqual([0, 1, 2]);
    expect(blocks[0]).toEqual({ kind: "thinking", at: 0, text: "hmm", ms: 40 });
  });

  it("groups nothing into nothing", () => {
    expect(groupParts([])).toEqual([]);
  });
});

describe("turns as the window draws them", () => {
  it("reads a turn from Rust, with or without parts", () => {
    const older = fromTurn({ role: "assistant", text: "x", attachments: [] } as never);
    expect(older.parts).toEqual([]);

    const newer = fromTurn({
      role: "assistant",
      text: "x",
      attachments: [],
      parts: [step("a")],
    });
    expect(newer.parts).toHaveLength(1);
  });

  it("makes a question with nothing but its words", () => {
    expect(fromQuestion("hi")).toEqual({ role: "user", text: "hi", parts: [], attachments: [] });
  });
});
