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
import { LISTBOX, isBrowsing, itemId, optionId, selectionAfter } from "$lib/results";

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
  test("it is one while browsing the root list", () => {
    for (const mode of ["root", "switcher", "emoji", "appVolume", "processes", "destination"]) {
      expect(isBrowsing(mode, 5)).toBe(true);
    }
  });

  /*
   * The bug this was widened to fix.
   *
   * These three draw a listbox of their own rather than the root list, and
   * every one of them was silent: `isBrowsing` asked whether the mode walks
   * the RANKED results, which is the root list and nothing else, so the field
   * stopped calling itself a combobox the moment somebody opened the
   * clipboard, the store or the conversation list.
   */
  test("it is one in the views that count their own rows", () => {
    for (const mode of ["clipboard", "store", "conversations"]) {
      expect(isBrowsing(mode, 5)).toBe(true);
    }
  });

  /*
   * The trap. The mode cannot answer for an extension: `command` is whichever
   * of four things the extension rendered, and only two of them are a list.
   */
  test("an extension's view is one only when it rendered a list or a grid", () => {
    for (const mode of ["command", "argument"]) {
      expect(isBrowsing(mode, 5, "List")).toBe(true);
      expect(isBrowsing(mode, 5, "Grid")).toBe(true);

      expect(isBrowsing(mode, 5, "Form")).toBe(false);
      expect(isBrowsing(mode, 5, "Detail")).toBe(false);

      // Nothing rendered yet. Pointing at a row in a tree that has not
      // arrived is the state this whole wiring exists to avoid.
      expect(isBrowsing(mode, 5)).toBe(false);
    }
  });

  test("naming something is typing, not filtering", () => {
    // Alias, collection and workspace modes take a name. There is nothing to
    // arrow through, so announcing a highlighted row would announce a fiction.
    for (const mode of ["alias", "collection", "namingWorkspace"]) {
      expect(isBrowsing(mode, 5)).toBe(false);
      expect(isBrowsing(mode, 5, "List")).toBe(false);
    }
  });

  test("a mode nobody declared is not a list", () => {
    expect(isBrowsing("whatever", 5)).toBe(false);
  });

  test("an empty list is not something to point at", () => {
    for (const mode of ["root", "switcher", "clipboard", "store"]) {
      expect(isBrowsing(mode, 0)).toBe(false);
    }

    expect(isBrowsing("command", 0, "List")).toBe(false);
  });
});

describe("the ids the two sides agree on", () => {
  test("a row id is stable and distinct per row", () => {
    expect(optionId(0)).toBe(optionId(0));
    expect(optionId(0)).not.toBe(optionId(1));
  });

  test("a menu item id is distinct per menu and per item", () => {
    expect(itemId("a", 0)).toBe(itemId("a", 0));
    expect(itemId("a", 0)).not.toBe(itemId("a", 1));
    expect(itemId("a", 0)).not.toBe(itemId("b", 0));
  });

  /*
   * A menu opens OVER a list, so both sets of ids are in the document at the
   * same time. One colliding with the other would resolve
   * `aria-activedescendant` to the wrong element, and nothing about that looks
   * wrong on screen.
   */
  test("a menu item id can never be a row id", () => {
    for (let at = 0; at < 50; at++) {
      expect(itemId("sill-actions", at)).not.toBe(optionId(at));
      expect(itemId("sill-tray-menu", at)).not.toBe(optionId(at));
    }
  });

  test("every id is usable in markup", () => {
    // They end up in `id` and `aria-controls`. A space or a hash would make
    // the reference silently fail to resolve.
    for (const id of [LISTBOX, optionId(0), optionId(999), itemId("sill-actions", 3)]) {
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
