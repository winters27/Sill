/**
 * Merging two searches into one list, and when the field is a combobox.
 *
 * Both were written inside a component and neither could be tested there. The
 * merge decides where a whole group of results reads, and the combobox rule
 * decides whether a screen reader says anything at all.
 */
import { describe, expect, test } from "vitest";
import type { RankedCommand } from "$lib/exthost/commands";
import { LISTBOX, isBrowsing, merged, optionId } from "$lib/results";

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

describe("putting a second search into the first", () => {
  test("results found by name keep the top", () => {
    const index = [result("named", true), result("named too", true), result("loose")];

    expect(ids(merged(index, [result("emoji")]))).toEqual([
      "named",
      "named too",
      "emoji",
      "loose",
    ]);
  });

  /*
   * The measurement this exists for. Typing "tada" matched eighty-four things
   * in the index, every one a coincidence of spelling, and the emoji somebody
   * had plainly named landed eighty-fifth, where Enter opened a Sill setting.
   */
  test("when the index only half-recognised the query, the named result leads", () => {
    const loose = Array.from({ length: 84 }, (_, i) => result(`loose ${i}`));

    expect(ids(merged(loose, [result("emoji")]))[0]).toBe("emoji");
  });

  test("with nothing strong and nothing to add, nothing moves", () => {
    const loose = [result("a"), result("b")];

    expect(ids(merged(loose, []))).toEqual(["a", "b"]);
  });

  test("an empty first list is just the second", () => {
    expect(ids(merged([], [result("emoji")]))).toEqual(["emoji"]);
  });

  test("neither list is reordered within itself", () => {
    const index = [result("s1", true), result("s2", true), result("w1"), result("w2")];
    const extra = [result("e1"), result("e2")];

    expect(ids(merged(index, extra))).toEqual(["s1", "s2", "e1", "e2", "w1", "w2"]);
  });

  test("everything strong means the second list goes last", () => {
    // Nothing to get above, so it reads after what was asked for.
    const index = [result("a", true), result("b", true)];

    expect(ids(merged(index, [result("e")]))).toEqual(["a", "b", "e"]);
  });

  test("nothing is lost whatever the mix", () => {
    // The property. A merge that drops a result is worse than one that orders
    // them oddly, and much harder to notice.
    const index = [result("a", true), result("b"), result("c", true), result("d")];
    const extra = [result("e"), result("f")];
    const out = merged(index, extra);

    expect(out).toHaveLength(6);
    for (const row of [...index, ...extra]) {
      expect(ids(out)).toContain(row.id);
    }
  });
});

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
