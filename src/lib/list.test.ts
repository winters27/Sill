/**
 * The result list's arithmetic.
 *
 * This is the part that can be wrong without looking wrong, and it had no
 * tests at all when it rendered a screen of blank space with every result
 * pushed off the bottom.
 */
import { describe, expect, test } from "vitest";
import type { RankedCommand } from "$lib/exthost/commands";
import { groupOf, linesOf, scrollFor } from "$lib/list";

const ROW = 38;
const HEADER = 26;

function command(mode: string, title = "x", strong = false): RankedCommand {
  return {
    id: `${mode}:${title}`,
    extension: "test",
    extensionTitle: "Test",
    title,
    subtitle: "",
    mode,
    entrypoint: "",
    matched: [],
    strong,
  } as unknown as RankedCommand;
}

/** A list of `n` results, all of one kind, so no headings appear. */
function flat(n: number): RankedCommand[] {
  return Array.from({ length: n }, (_, i) => command("app", `app ${i}`));
}

describe("what a row is filed under", () => {
  test("each kind has a heading a person would recognise", () => {
    expect(groupOf(command("app"))).toBe("Applications");
    expect(groupOf(command("emoji"))).toBe("Emoji");
    expect(groupOf(command("window"))).toBe("Open Windows");
    expect(groupOf(command("answer"))).toBe("Answer");
    expect(groupOf(command("exe"))).toBe("Developer");
  });

  test("the row standing in for missing files sits with the files", () => {
    // It is there instead of file results, so a heading of its own would
    // point at the absence rather than at where the results should be.
    expect(groupOf(command("file-setup"))).toBe(groupOf(command("file")));
  });

  test("something unrecognised is still filed somewhere", () => {
    // A new kind added on the Rust side must not produce a heading-less row.
    expect(groupOf(command("something-new"))).toBeTruthy();
  });
});

describe("laying the list out", () => {
  test("one kind of result gets no heading at all", () => {
    // A lone heading over the whole list is noise rather than structure.
    const lines = linesOf(flat(3));

    expect(lines).toHaveLength(3);
    expect(lines.every((line) => line.kind === "row")).toBe(true);
  });

  test("two kinds get a heading each", () => {
    const lines = linesOf([command("app"), command("emoji"), command("app")]);

    expect(lines.filter((line) => line.kind === "header")).toHaveLength(2);
    expect(lines[0]).toMatchObject({ kind: "header", label: "Applications" });
  });

  test("groups are ordered by their best member, not alphabetically", () => {
    // The ranker decides what is seen first. Grouping that fought it would put
    // the answer below a heading nobody was looking at.
    const lines = linesOf([command("emoji"), command("app")]);

    expect(lines[0]).toMatchObject({ label: "Emoji" });
  });

  test("a row keeps the index it had in the results", () => {
    // Selection indexes the results, not the drawn lines. Losing that mapping
    // means Enter opens whatever happens to sit at that row.
    const lines = linesOf([command("app"), command("emoji")]);
    const rows = lines.filter((line) => line.kind === "row");

    expect(rows.map((row) => (row as { index: number }).index)).toEqual([0, 1]);
  });
});

describe("keeping the selected row in view", () => {
  const base = {
    viewport: 400,
    scrollHeight: 5000,
    rowHeight: 40,
    gap: 8,
    first: false,
    last: false,
  };

  test("a row already in view does not move the list", () => {
    // Moving when nothing needs to move is what makes arrowing feel jumpy.
    expect(scrollFor({ ...base, scrollTop: 1000, rowTop: 1100 })).toBe(1000);
  });

  test("a row above the view is brought down to it", () => {
    expect(scrollFor({ ...base, scrollTop: 1000, rowTop: 900 })).toBe(892);
  });

  test("a row below the view is brought up to it", () => {
    // rowTop 1360 + 40 high + 8 gap - 400 viewport.
    expect(scrollFor({ ...base, scrollTop: 1000, rowTop: 1360 })).toBe(1008);
  });

  test("the first row goes all the way to the top", () => {
    // So a group heading above it stays visible rather than being clipped by
    // the gap arithmetic.
    expect(scrollFor({ ...base, scrollTop: 3000, rowTop: 0, first: true })).toBe(0);
  });

  test("the last row goes past the end and lets the browser stop it", () => {
    // The container knows about its own padding below the rows; this file
    // deliberately does not, having been wrong about it twice.
    expect(scrollFor({ ...base, scrollTop: 0, rowTop: 4900, last: true })).toBe(5000);
  });

  test("a row near the top never asks for a negative position", () => {
    expect(scrollFor({ ...base, scrollTop: 20, rowTop: 4 })).toBe(0);
  });
});

describe("a repeated id", () => {
  /**
   * The bug this reproduces blanked the entire launcher.
   *
   * The rows are drawn by a keyed loop, so a repeated key throws and takes the
   * whole block with it: no rows, no headers, nothing. Four Windows settings
   * pages shared the id `setting:mmc.exe`, and because the list used to draw
   * only the rows in view, it went unnoticed until it drew all of them.
   */
  test("costs one row, not the list", () => {
    const rows = linesOf([
      command("app", "Terminal"),
      command("app", "Terminal"),
      command("app", "Browser"),
    ]);

    expect(rows).toHaveLength(2);
    expect(rows.map((r) => (r.kind === "row" ? r.command.title : r.label))).toEqual([
      "Terminal",
      "Browser",
    ]);
  });

  test("does not disturb the positions selection is counted in", () => {
    // The dropped row's own position goes with it; the ones after keep theirs,
    // because selection indexes the results rather than the drawn rows.
    const rows = linesOf([
      command("app", "Terminal"),
      command("app", "Terminal"),
      command("app", "Browser"),
    ]);

    expect(rows.map((r) => (r.kind === "row" ? r.index : -1))).toEqual([0, 2]);
  });

  test("is dropped inside its group, leaving the headings intact", () => {
    const rows = linesOf([
      command("app", "Terminal"),
      command("setting", "Sound"),
      command("setting", "Sound"),
    ]);

    expect(rows.map((r) => (r.kind === "row" ? r.command.title : `# ${r.label}`))).toEqual([
      "# Applications",
      "Terminal",
      "# Settings",
      "Sound",
    ]);
  });
});
