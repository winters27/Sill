import { describe, expect, it } from "vitest";

import type { AiConversation } from "$lib/exthost/commands";
import { conversationRows } from "$lib/conversations";
import { byDay, narrow, whereFrom } from "./rail";

const one = (id: string, title: string, age: number): AiConversation => ({
  id,
  title,
  replies: 1,
  age,
  open: false,
});

describe("grouping conversations by day", () => {
  // A fixed moment: 2026-09-05 at 10:00 local.
  const now = Math.floor(new Date(2026, 8, 5, 10, 0, 0).getTime() / 1000);
  const fetched = now;

  it("puts this morning under today, last night under yesterday, and the rest earlier", () => {
    const groups = byDay(
      [
        one("a", "this morning", 2 * 3600),
        one("b", "last night", 11 * 3600),
        one("c", "last week", 7 * 86_400),
      ],
      fetched,
      now,
    );

    expect(groups.map((g) => [g.label, g.rows.map((r) => r.id)])).toEqual([
      ["Today", ["a"]],
      ["Yesterday", ["b"]],
      ["Earlier", ["c"]],
    ]);
  });

  it("leaves out a day nothing happened on", () => {
    const groups = byDay([one("a", "now", 60)], fetched, now);
    expect(groups.map((g) => g.label)).toEqual(["Today"]);
  });

  it("orders each group newest first", () => {
    const [today] = byDay([one("old", "x", 3000), one("new", "y", 60)], fetched, now);
    expect(today.rows.map((r) => r.id)).toEqual(["new", "old"]);
  });

  /// Ages were read when the list was fetched. An hour later the same rows
  /// are an hour older, and a row fetched as "today" can have become
  /// "yesterday" without the list being fetched again.
  it("reads ages against the clock, not against the fetch", () => {
    const later = now + 20 * 3600;
    const groups = byDay([one("a", "asked at 08:00", 2 * 3600)], fetched, later);
    expect(groups[0].label).toBe("Yesterday");
  });
});

describe("narrowing by what was typed", () => {
  const all = [one("a", "What windows are open", 1), one("b", "Find the largest files", 2)];

  it("agrees with the launcher's list about which rows match", () => {
    for (const query of ["", "  ", "WIND", "files", "nothing here"]) {
      expect(narrow(all, query).map((r) => r.id)).toEqual(
        conversationRows(all, query).map((r) => r.entrypoint),
      );
    }
  });
});

describe("saying where the answer comes from", () => {
  const ready = { ready: true, id: "ollama", name: "Ollama", model: "qwen3.5 9b", whyNot: "" };

  it("names the three places", () => {
    expect(whereFrom({ ...ready, kind: "local" } as never)).toBe("on this PC");
    expect(whereFrom({ ...ready, kind: "cli" } as never)).toBe("through Claude Code");
    expect(whereFrom({ ...ready, kind: "key" } as never)).toBe("by key");
  });

  it("says nothing when nothing answers", () => {
    expect(whereFrom(null)).toBe("");
    expect(whereFrom({ ...ready, ready: false, kind: "key" } as never)).toBe("");
  });
});
