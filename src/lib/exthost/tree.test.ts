/**
 * What the view tree keeps, and what it is supposed to let go of.
 *
 * The op stream never says "forget this node". React removes a child and
 * moves on, and the ids it mints only ever go up, so anything the tree holds
 * after a removal is memory that nothing can address again. That was fine
 * while a command drew one screen and one list: a command now pushes screens
 * and pops them, and a screen opened and closed twenty times used to leave
 * twenty screens behind.
 *
 * The awkward half is that a removal inside one commit is not always a
 * deletion. React reorders by taking a child out and putting it back, so a
 * tree that dropped nodes the moment they were removed would lose one
 * mid-shuffle. The unit is the batch, and these hold both ends of that.
 */
import { describe, expect, test } from "vitest";

import { ROOT_ID, ViewTree, type Op } from "./tree";

/** The ops the reconciler sends to put one element under another. */
function make(id: number, tag: string, parent = ROOT_ID): Op[] {
  return [
    { op: "create", id, $t: tag, props: {} },
    { op: "append", parent, child: id },
  ];
}

describe("what a removal means", () => {
  test("a node taken off and put back in one batch survives it", () => {
    const tree = new ViewTree();
    tree.apply([...make(1, "List"), ...make(2, "List.Item", 1), ...make(3, "List.Item", 1)]);

    // A reorder: the second row moves in front of the first, which React
    // expresses as a removal and an insertion in the same commit.
    tree.apply([
      { op: "remove", parent: 1, child: 3 },
      { op: "insertBefore", parent: 1, child: 3, before: 2 },
    ]);

    const list = tree.top();
    expect(list).toBeDefined();
    expect(tree.elementChildren(list!).map((n) => n.id)).toEqual([3, 2]);
  });

  test("a node removed and left off is forgotten", () => {
    const tree = new ViewTree();
    tree.apply([...make(1, "List"), ...make(2, "List.Item", 1)]);
    expect(tree.get(2)).toBeDefined();

    tree.apply([{ op: "remove", parent: 1, child: 2 }]);

    expect(tree.get(2)).toBeUndefined();
  });

  test("a removed node takes its whole subtree with it", () => {
    const tree = new ViewTree();
    tree.apply([
      ...make(1, "List"),
      ...make(2, "List.Item", 1),
      ...make(3, "$slot", 2),
      ...make(4, "ActionPanel", 3),
    ]);

    tree.apply([{ op: "remove", parent: 1, child: 2 }]);

    // The reconciler sends one op for the row; everything under it went with
    // it and nothing else will ever mention those ids again.
    for (const id of [2, 3, 4]) expect(tree.get(id)).toBeUndefined();
  });

  test("clearing a node forgets what was inside it", () => {
    const tree = new ViewTree();
    tree.apply([...make(1, "List"), ...make(2, "List.Item", 1), ...make(3, "List.Item", 1)]);

    tree.apply([{ op: "clear", id: 1 }]);

    expect(tree.get(2)).toBeUndefined();
    expect(tree.get(3)).toBeUndefined();
    expect(tree.get(1)).toBeDefined();
  });
});

describe("pushing a screen and going back", () => {
  /**
   * The property the navigation stack rests on.
   *
   * Pushing renders a different element into the same React root, so the op
   * stream is a removal of the whole previous screen and a creation of a whole
   * new one. Twenty round trips through that is twenty screens' worth of nodes
   * unless the removal actually removes something, and a stack that grows
   * every time somebody presses Escape is worse than no stack at all.
   */
  test("twenty round trips cost what one costs", () => {
    const tree = new ViewTree();

    let id = 1;
    const screen = (tag: string, rows: number) => {
      const root = id++;
      const ops: Op[] = [...make(root, tag)];
      for (let n = 0; n < rows; n++) ops.push(...make(id++, `${tag}.Item`, root));
      return { root, ops };
    };

    const first = screen("List", 30);
    tree.apply(first.ops);
    const settled = tree.size;

    let showing = first.root;
    for (let round = 0; round < 20; round++) {
      const detail = screen("Detail", 0);
      tree.apply([{ op: "remove", parent: ROOT_ID, child: showing }, ...detail.ops]);
      expect(tree.top()?.tag).toBe("Detail");

      const back = screen("List", 30);
      tree.apply([{ op: "remove", parent: ROOT_ID, child: detail.root }, ...back.ops]);
      expect(tree.top()?.tag).toBe("List");
      showing = back.root;
    }

    expect(tree.size).toBe(settled);
  });

  test("a list that shrinks shrinks", () => {
    const tree = new ViewTree();
    const ops: Op[] = [...make(1, "List")];
    for (let n = 2; n <= 201; n++) ops.push(...make(n, "List.Item", 1));
    tree.apply(ops);

    const full = tree.size;

    // What typing into a list of two hundred does: most of the rows go.
    tree.apply(Array.from({ length: 190 }, (_, n) => ({
      op: "remove" as const,
      parent: 1,
      child: n + 12,
    })));

    expect(tree.size).toBe(full - 190);
  });
});
