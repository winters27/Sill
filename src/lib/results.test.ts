/**
 * When the search field is a combobox.
 *
 * Written inside a component originally, where it could not be tested, and it
 * decides whether a screen reader says anything at all.
 *
 * The merge that used to be tested here moved to Rust with the search itself:
 * placing a second search's results was the reason the window made two invokes
 * per keystroke. Its seven cases are now in `commands/search.rs`.
 */
import { describe, expect, test } from "vitest";
import type { RankedCommand } from "$lib/exthost/commands";
import { LISTBOX, isBrowsing, optionId, selectionAfter } from "$lib/results";

function result(id: string, strong = false): RankedCommand {
  return {
    id,
    extension: "test",
    extensionTitle: "Test",
    title: id,
    subtitle: "",
    mode: "app",
    entrypoint: "",
    matched: [],
    strong,
  } as unknown as RankedCommand;
}

const ids = (rows: RankedCommand[]) => rows.map((row) => row.id);

describe("when the field is a combobox", () => {
  test("it is one while browsing a list with something in it", () => {
    for (const mode of ["root", "switcher", "emoji"]) {
      expect(isBrowsing(mode, 5)).toBe(true);
    }
  });

  /*
   * The trap. The field is shared across modes and the list is not: pointing
   * at a listbox that is not rendered leaves a screen reader announcing
   * nothing, which is the exact state the combobox wiring was added to fix.
   */
  test("it is not one where the list is not on screen", () => {
    for (const mode of ["clipboard", "command", "argument", "collection"]) {
      expect(isBrowsing(mode, 5)).toBe(false);
    }
  });

  test("naming something is typing, not filtering", () => {
    // Alias and collection modes take a name. There is nothing to arrow
    // through, so announcing a highlighted row would be announcing a fiction.
    expect(isBrowsing("alias", 5)).toBe(false);
    expect(isBrowsing("collection", 5)).toBe(false);
  });

  test("an empty list is not something to point at", () => {
    for (const mode of ["root", "switcher", "emoji"]) {
      expect(isBrowsing(mode, 0)).toBe(false);
    }
  });
});

describe("the ids the two sides agree on", () => {
  test("a row id is stable and distinct per row", () => {
    expect(optionId(0)).toBe(optionId(0));
    expect(optionId(0)).not.toBe(optionId(1));
  });

  test("both ids are usable in markup", () => {
    // They end up in `id` and `aria-controls`. A space or a hash would make
    // the reference silently fail to resolve.
    for (const id of [LISTBOX, optionId(0), optionId(999)]) {
      expect(id).toMatch(/^[A-Za-z][\w-]*$/);
    }
  });
});

/**
 * Where the highlight goes when the results change underneath it.
 *
 * The selection was a number, and a number means nothing once the list it
 * counted into has been replaced.
 */
describe("keeping the selection while the list changes", () => {
  const rows = (...ids: string[]) => ids.map((id) => ({ id }));

  test("a new query starts at the top", () => {
    const held = { id: "notepad", index: 4 };

    expect(selectionAfter(held, rows("a", "notepad", "c"), false)).toBe(0);
  });

  /**
   * The bug. Files and browser pages arrive a moment after the commands, and
   * rebuilding the list moved the highlight to whatever had taken that
   * position.
   */
  test("a late page of files does not move the highlighted row", () => {
    const held = { id: "notepad", index: 1 };
    const grown = rows("a", "notepad", "c", "file1", "file2");

    expect(selectionAfter(held, grown, true)).toBe(1);
  });

  test("a row that moved up is followed", () => {
    const held = { id: "notepad", index: 3 };

    expect(selectionAfter(held, rows("notepad", "a", "b"), true)).toBe(0);
  });

  test("a row that is gone falls back to the same position", () => {
    const held = { id: "vanished", index: 1 };

    expect(selectionAfter(held, rows("a", "b", "c"), true)).toBe(1);
  });

  test("a row that is gone from a shorter list falls back to the top", () => {
    const held = { id: "vanished", index: 7 };

    expect(selectionAfter(held, rows("a", "b"), true)).toBe(0);
  });

  test("an empty list is the top whatever was held", () => {
    expect(selectionAfter({ id: "notepad", index: 3 }, [], true)).toBe(0);
  });

  test("nothing held starts where it was, if that still exists", () => {
    expect(selectionAfter({ id: undefined, index: 2 }, rows("a", "b", "c"), true)).toBe(2);
    expect(selectionAfter({ id: undefined, index: 9 }, rows("a", "b", "c"), true)).toBe(0);
  });
});
