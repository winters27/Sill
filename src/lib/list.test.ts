/**
 * The result list's arithmetic.
 *
 * This is the part that can be wrong without looking wrong, and it had no
 * tests at all when it rendered a screen of blank space with every result
 * pushed off the bottom.
 */
import { describe, expect, test } from "vitest";
import type { RankedCommand } from "$lib/exthost/commands";
import { groupOf, lineAt, linesOf, offsetsOf, windowOf } from "$lib/list";

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

describe("where each line sits", () => {
  test("headings and rows are their own heights", () => {
    const offsets = offsetsOf(linesOf([command("app"), command("emoji")]), ROW, HEADER);

    // header, row, header, row, and the end.
    expect(offsets).toEqual([0, HEADER, HEADER + ROW, HEADER * 2 + ROW, HEADER * 2 + ROW * 2]);
  });

  test("an empty list has a zero height rather than no answer", () => {
    expect(offsetsOf([], ROW, HEADER)).toEqual([0]);
  });

  test("a position finds the line it falls inside", () => {
    const lines = linesOf(flat(5));
    const offsets = offsetsOf(lines, ROW, HEADER);

    expect(lineAt(offsets, lines.length, 0)).toBe(0);
    expect(lineAt(offsets, lines.length, ROW - 1)).toBe(0);
    expect(lineAt(offsets, lines.length, ROW)).toBe(1);
    expect(lineAt(offsets, lines.length, ROW * 4)).toBe(4);
  });

  test("a position past the end lands on the last line, not off it", () => {
    const lines = linesOf(flat(3));
    const offsets = offsetsOf(lines, ROW, HEADER);

    expect(lineAt(offsets, lines.length, 10_000)).toBe(2);
  });
});

describe("which slice is drawn", () => {
  const HEIGHT = 400;
  const OVERSCAN = 8;

  function windowFor(count: number, scrollTop: number) {
    const lines = linesOf(flat(count));
    const offsets = offsetsOf(lines, ROW, HEADER);

    return { lines, ...windowOf(offsets, lines.length, scrollTop, HEIGHT, OVERSCAN) };
  }

  test("the top of a long list draws from the first line", () => {
    const { first, last } = windowFor(200, 0);

    expect(first).toBe(0);
    expect(last).toBeGreaterThan(HEIGHT / ROW);
    expect(last).toBeLessThan(200);
  });

  test("scrolling draws the lines that are actually on screen", () => {
    const { at, first } = windowFor(200, ROW * 50);

    expect(at).toBe(ROW * 50);
    expect(first).toBe(50 - OVERSCAN);
  });

  test("a list shorter than the viewport draws all of it from the top", () => {
    const { at, first, last } = windowFor(3, 0);

    expect(at).toBe(0);
    expect(first).toBe(0);
    expect(last).toBe(3);
  });

  /*
   * The bug this file exists for.
   *
   * The remembered scroll position is only refreshed by a scroll event, and
   * the browser clamps the real one on its own whenever the content gets
   * shorter. Between the two, a search returning fewer results than the last
   * one sliced the list from a position past its end, so every row rendered
   * below the viewport: a screen of blank space that only came right if you
   * scrolled and provoked an event.
   */
  test("a position past the end of a short list still draws the whole list", () => {
    // The browser has already corrected the element by the time this is asked,
    // so the position it is given is real. What matters is that a number from
    // anywhere cannot index outside the list.
    const { first, last, lines } = windowFor(3, ROW * 150);

    expect(first).toBe(0);
    expect(last).toBe(lines.length);
  });

  test("the drawn slice is never empty while there are results", () => {
    // The blank screen, stated as the property rather than as one case. No
    // remembered position, however stale, may draw nothing.
    for (const count of [1, 3, 12, 200]) {
      for (const scrollTop of [0, 37, 500, 5_000, 100_000]) {
        const { first, last } = windowFor(count, scrollTop);

        expect(last).toBeGreaterThan(first);
      }
    }
  });

  test("a negative position is treated as the top", () => {
    // Some browsers report one during an overscroll bounce.
    const { at, first } = windowFor(50, -200);

    expect(at).toBe(0);
    expect(first).toBe(0);
  });

  test("an empty list draws nothing without failing", () => {
    const offsets = offsetsOf([], ROW, HEADER);
    const { first, last } = windowOf(offsets, 0, 0, HEIGHT, OVERSCAN);

    expect(first).toBe(0);
    expect(last).toBe(0);
  });

  test("a container with padding below the rows still draws its last row", () => {
    // The bug that shipped. The rows are not all there is inside the scrolling
    // box: 48 pixels of padding below them clears the chin, and that padding
    // scrolls too. Anything here that worked out the reach from the rows alone
    // stopped the drawn slice short of the end, and the bottom went blank.
    const CHIN = 48;
    const lines = linesOf(flat(200));
    const offsets = offsetsOf(lines, ROW, HEADER);
    const bottom = offsets[lines.length] + CHIN - HEIGHT;

    const { last } = windowOf(offsets, lines.length, bottom, HEIGHT, OVERSCAN);

    expect(last).toBe(lines.length);
  });
});

describe("Windows' own switches", () => {
  test("they get a heading of their own rather than Sill's", () => {
    // Filed with Sill's commands they read as Sill features, which is the
    // opposite of true: changing the volume changes the machine.
    expect(groupOf(command("system"))).toBe("System");
    expect(groupOf(command("system"))).not.toBe(groupOf(command("builtin")));
  });

  test("they are not filed with the settings pages either", () => {
    // A settings page opens somewhere and leaves the changing to a person.
    // These change the machine outright, and the difference is worth a
    // separate heading.
    expect(groupOf(command("system"))).not.toBe(groupOf(command("setting")));
  });
});
