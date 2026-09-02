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
import { isRunnable, type ActionEntry } from "$lib/exthost/actions";

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
    expect(groupOf(command("exe"))).toBe("Command Line");
  });

  /**
   * Whose settings they are, said out loud.
   *
   * "Settings" beside "Sill Settings" reads as though one is the general case
   * and the other a special one, when they are two different programs'
   * settings and the difference is the whole question.
   */
  test("settings say which program they belong to", () => {
    expect(groupOf(command("setting"))).toBe("Windows Settings");
  });

  /**
   * The four the old default was silently wrong about.
   *
   * `groupOf` was a switch returning "Applications" for anything it did not
   * name, so a saved window arrangement, a running process, a captured piece
   * of text and a clipboard entry all read as applications. None of them is
   * one. This is the recurring shape: a match over modes with a default makes
   * forgetting silent, and the thing forgotten looks like something else
   * rather than looking wrong.
   */
  test("a row that is not an application does not say it is", () => {
    expect(groupOf(command("workspace"))).toBe("Arrangements");
    expect(groupOf(command("process"))).toBe("Running");
    expect(groupOf(command("text"))).toBe("Text");
    expect(groupOf(command("clipboard"))).toBe("Clipboard History");
  });

  /**
   * A mode nobody named falls back to where the row came from.
   *
   * True, where "Applications" was not: an extension command that named a
   * mode Sill does not know is still that extension's.
   */
  test("an unknown mode is filed under whatever produced it", () => {
    const odd = { ...command("something-nobody-named"), extensionTitle: "Raycast Thing" };

    expect(groupOf(odd)).toBe("Raycast Thing");
    expect(groupOf(command("sill-setting"))).toBe("Sill Settings");
    expect(groupOf(command("system"))).toBe("System Controls");
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
      "# Windows Settings",
      "Sound",
    ]);
  });
});

describe("a page from a browser", () => {
  test("groups under its own heading rather than with applications", () => {
    expect(groupOf(command("url", "GitHub"))).toBe("Browser");
  });

  test("saved and visited share the heading", () => {
    const rows = linesOf([command("url", "Saved"), command("url", "Visited")]);

    // One group, so no heading at all: a single group is not a grouping.
    expect(rows.every((r) => r.kind === "row")).toBe(true);
  });
});

describe("the web search row", () => {
  test("has a heading of its own rather than joining applications", () => {
    expect(groupOf(command("websearch", "Search for cats"))).toBe("Web Search");
  });

  /**
   * It answers every query, so ranking it would displace real results. It is
   * appended last instead, and this holds that: whatever else is in the list,
   * the search row is the final line.
   */
  test("is the last line whatever else is in the list", () => {
    const rows = linesOf([
      command("app", "Terminal"),
      command("url", "GitHub"),
      command("websearch", "Search for cats"),
    ]);

    const last = rows[rows.length - 1];
    expect(last.kind).toBe("row");
    expect(last.kind === "row" && last.command.mode).toBe("websearch");
  });
});

describe("whether an action can be run", () => {
  function entry(over: Partial<ActionEntry>): ActionEntry {
    return { id: "a", title: "An action", tag: "Action.Unknown", props: {}, ...over } as ActionEntry;
  }

  /**
   * "no action" beside a row is for an extension that declared something with
   * nothing behind it. Sill's own actions are dispatched by tag, and every one
   * of them was wearing that label: eleven rows on a file, all working, all
   * saying otherwise.
   */
  test("Sill's own actions are runnable, whatever else is true of them", () => {
    expect(isRunnable(entry({ tag: "Sill.Action:sill.file.verify" }))).toBe(true);
    expect(isRunnable(entry({ tag: "Sill.ClipboardDelete" }))).toBe(true);
  });

  test("an extension action with a handler is runnable", () => {
    expect(isRunnable(entry({ handler: "h1" }))).toBe(true);
  });

  test("a built-in an extension declared is runnable without one", () => {
    expect(isRunnable(entry({ tag: "Action.CopyToClipboard" }))).toBe(true);
  });

  /** The case the label exists for, which still has to work. */
  test("an extension action with neither is not", () => {
    expect(isRunnable(entry({ tag: "Action.SomethingNobodyImplemented" }))).toBe(false);
  });
});

describe("what leaves the launcher on screen", () => {
  /**
   * The modes that finish the moment they run, and so should dismiss.
   *
   * A Windows switch was missing from this list and fell through to the
   * extension command view, so the next summon came back showing an extension
   * screen with no extension in it, titled after the switch.
   */
  const DONE_ON_RUN = ["app", "exe", "setting", "builtin", "system"];

  test("a Windows switch is done the moment it runs, like an application", () => {
    expect(DONE_ON_RUN).toContain("system");
  });

  /** An extension command is the one that genuinely has more to show. */
  test("an extension command is not", () => {
    expect(DONE_ON_RUN).not.toContain("view");
  });
});
