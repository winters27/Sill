/**
 * The shapes an extension is allowed to pass, and what each one draws.
 *
 * Every one of these is a real form from the `@raycast/api` declarations, and
 * the reason they are worth a test rather than an eyeball is that they are
 * indistinguishable on screen when they are wrong: an icon read the wrong way
 * is a missing icon, and a missing icon looks exactly like a row that never
 * had one. There is nothing to see and nothing to blame.
 */
import { describe, expect, test } from "vitest";

import {
  accessoriesOf,
  colourOf,
  dropdownOf,
  emptyViewOf,
  iconOf,
  metadataOf,
  paginationOf,
} from "./present";
import { ROOT_ID, ViewTree, type Op } from "./tree";

/** Builds a tree from a nested description, the way the op stream would. */
function grow(spec: Node, tree = new ViewTree()): { tree: ViewTree; top: () => ReturnType<ViewTree["top"]> } {
  let next = 1;
  const ops: Op[] = [];

  const add = (node: Node, parent: number) => {
    const id = next++;
    ops.push({ op: "create", id, $t: node.tag, props: node.props ?? {} });
    ops.push({ op: "append", parent, child: id });
    for (const child of node.children ?? []) add(child, id);
  };

  add(spec, ROOT_ID);
  tree.apply(ops);
  return { tree, top: () => tree.top() };
}

interface Node {
  tag: string;
  props?: Record<string, unknown>;
  children?: Node[];
}

describe("icons, in every shape one arrives in", () => {
  test("a name is a mark", () => {
    expect(iconOf("Star")).toEqual({ kind: "mark", name: "Star", tint: undefined });
  });

  test("a URL is a picture", () => {
    expect(iconOf("https://example.invalid/a.png")).toMatchObject({ kind: "image" });
  });

  /*
   * The one that would silently draw nothing. `getAvatarIcon` from
   * `@raycast/utils` returns exactly this and it is all over the store.
   */
  test("a data URI is a picture", () => {
    expect(iconOf("data:image/svg+xml;base64,AAAA")).toMatchObject({ kind: "image" });

    /*
     * The one character that truncates rather than merely being untidy.
     *
     * Extensions write these by hand and put a colour in them, and `#` starts
     * a fragment: the browser loads the document up to it and draws nothing.
     * Hacker News numbers its rows this way, and drew thirty empty tiles.
     */
    expect(
      iconOf(`data:image/svg+xml,<svg><rect fill="#DD7949"/></svg>`),
      "a colour inside an unencoded SVG ends the URL at the #",
    ).toMatchObject({
      kind: "image",
      src: `data:image/svg+xml,<svg><rect fill="%23DD7949"/></svg>`,
    });

    // Parameters before the comma are common and must not stop it being read.
    expect(
      iconOf(`data:image/svg+xml;charset=utf-8,<svg fill="#fff"/>`),
    ).toMatchObject({ src: `data:image/svg+xml;charset=utf-8,<svg fill="%23fff"/>` });

    // An author who encoded it correctly has no bare # left to touch, so this
    // is safe to run over a payload that is already right.
    expect(iconOf(`data:image/svg+xml,<svg fill="%23fff"/>`)).toMatchObject({
      src: `data:image/svg+xml,<svg fill="%23fff"/>`,
    });

    // base64 carries its own encoding and must be left exactly as it is.
    expect(iconOf("data:image/svg+xml;base64,PHN2ZyBmaWxsPSIjZmZmIi8+")).toMatchObject({
      src: "data:image/svg+xml;base64,PHN2ZyBmaWxsPSIjZmZmIi8+",
    });

    // Everything else is somebody else's URL and is not rewritten.
    expect(iconOf("https://example.test/a.png#frag")).toMatchObject({
      src: "https://example.test/a.png#frag",
    });
  });

  test("an emoji is printed rather than looked up", () => {
    expect(iconOf("🎉")).toEqual({ kind: "glyph", text: "🎉", tint: undefined });
  });

  test("a source and a tint keep the tint", () => {
    expect(iconOf({ source: "Circle", tintColor: "raycast-green" })).toEqual({
      kind: "mark",
      name: "Circle",
      tint: "var(--success)",
    });
  });

  test("a light and dark pair resolves to one picture", () => {
    expect(
      iconOf({ light: "https://a.invalid/l.png", dark: "https://a.invalid/d.png" }),
    ).toMatchObject({ kind: "image", src: "https://a.invalid/d.png" });
  });

  /*
   * A row with no icon must be told apart from a row whose icon could not be
   * read. Both draw nothing; only one of them is a bug, and folding them
   * together is how the second becomes invisible.
   */
  test("nothing is nothing", () => {
    expect(iconOf(undefined)).toBeUndefined();
    expect(iconOf("")).toBeUndefined();
    expect(iconOf({ fileIcon: "C:/a.exe" })).toBeUndefined();
  });

  test("a colour Sill has no hue for is not painted a nearby one", () => {
    expect(colourOf("raycast-purple")).toBeUndefined();
    expect(colourOf("raycast-red")).toBe("var(--danger)");
  });
});

describe("accessories", () => {
  const row = (accessories: unknown[]) =>
    grow({ tag: "List.Item", props: { accessories } }).top()!;

  test("text, tags and dates all reach the row", () => {
    const out = accessoriesOf(
      row([{ text: "12 items" }, { tag: "beta" }, { date: "2026-09-03" }]),
    );

    expect(out.map((a) => a.text ?? a.tag)).toEqual(["12 items", "beta", "2026-09-03"]);
  });

  test("a tag written as a value and a colour keeps both", () => {
    const [only] = accessoriesOf(row([{ tag: { value: "ready", color: "raycast-green" } }]));
    expect(only).toMatchObject({ tag: "ready", tint: "var(--success)" });
  });

  /*
   * An extension builds these from data it may not have, so empty entries are
   * normal. Drawing them is a pill with nothing in it beside every third row.
   */
  test("an accessory with nothing in it is not drawn", () => {
    expect(accessoriesOf(row([{}, { text: "" }, { tooltip: "only a tooltip" }]))).toEqual([]);
  });

  test("a row with no accessories has none", () => {
    expect(accessoriesOf(grow({ tag: "List.Item" }).top()!)).toEqual([]);
  });
});

describe("the empty view an extension writes for itself", () => {
  test("its title and description are read", () => {
    const { tree, top } = grow({
      tag: "List",
      children: [
        { tag: "List.EmptyView", props: { title: "No repositories", description: "Sign in." } },
      ],
    });

    expect(emptyViewOf(tree, top()!)).toMatchObject({
      headline: "No repositories",
      hint: "Sign in.",
    });
  });

  test("a grid's is read by the same rule", () => {
    const { tree, top } = grow({
      tag: "Grid",
      children: [{ tag: "Grid.EmptyView", props: { title: "No colours" } }],
    });

    expect(emptyViewOf(tree, top()!)?.headline).toBe("No colours");
  });

  /*
   * A list without one keeps Sill's own words rather than being given empty
   * ones, which would be a blank pane where a sentence belongs.
   */
  test("a list that wrote none has none", () => {
    const { tree, top } = grow({ tag: "List", children: [{ tag: "List.Item" }] });
    expect(emptyViewOf(tree, top()!)).toBeUndefined();
  });
});

describe("the dropdown beside the field", () => {
  const picker = (children: Node[], props: Record<string, unknown> = {}) =>
    grow({
      tag: "List",
      children: [
        {
          tag: "$slot",
          props: { name: "searchBarAccessory" },
          children: [{ tag: "List.Dropdown", props, children }],
        },
      ],
    });

  test("its options are read in order, sections and all", () => {
    const { tree, top } = picker([
      {
        tag: "List.Dropdown.Section",
        props: { title: "Sort By" },
        children: [
          { tag: "List.Dropdown.Item", props: { title: "CPU", value: "cpu" } },
          { tag: "List.Dropdown.Item", props: { title: "Memory", value: "mem" } },
        ],
      },
      { tag: "List.Dropdown.Item", props: { title: "Everything", value: "all" } },
    ]);

    expect(dropdownOf(tree, top()!)?.options).toEqual([
      { value: "cpu", title: "CPU", section: "Sort By", icon: undefined },
      { value: "mem", title: "Memory", section: "Sort By", icon: undefined },
      { value: "all", title: "Everything", section: undefined, icon: undefined },
    ]);
  });

  test("the handler behind onChange is carried", () => {
    const { tree, top } = picker([], { onChange: { $handler: "h7" } });
    expect(dropdownOf(tree, top()!)?.onChange).toBe("h7");
  });

  /*
   * The two are opposite claims and were folded into one field.
   *
   * `value` is an extension driving its own picker and needing no telling;
   * `defaultValue` is an extension suggesting a start and waiting to be told
   * what it got. Read as the same thing, a dropdown that only suggested was
   * treated as driven, nobody told the extension anything, and a command
   * whose fetch waits on that first report drew an empty list forever.
   */
  test("driving the picker and suggesting a start are not the same field", () => {
    const driven = picker([{ tag: "List.Dropdown.Item", props: { value: "b" } }], {
      value: "b",
    });
    expect(dropdownOf(driven.tree, driven.top()!)?.value).toBe("b");
    expect(dropdownOf(driven.tree, driven.top()!)?.initial).toBeUndefined();

    const suggested = picker([{ tag: "List.Dropdown.Item", props: { value: "b" } }], {
      defaultValue: "b",
    });
    expect(dropdownOf(suggested.tree, suggested.top()!)?.initial).toBe("b");
    expect(
      dropdownOf(suggested.tree, suggested.top()!)?.value,
      "a suggested start reads as the extension driving it",
    ).toBeUndefined();
  });

  test("a list with no accessory has no dropdown", () => {
    const { tree, top } = grow({ tag: "List" });
    expect(dropdownOf(tree, top()!)).toBeUndefined();
  });
});

describe("metadata", () => {
  /*
   * The reason the tag is matched by its last segment. A label beside a list
   * is `List.Item.Detail.Metadata.Label` and the same label on a detail page
   * is `Detail.Metadata.Label`, and a table of full names would have to carry
   * both. This asserts one function reads both, because the alternative is a
   * metadata row that works on one surface and not the other depending on
   * which table somebody remembered.
   */
  for (const prefix of ["Detail.Metadata", "List.Item.Detail.Metadata"]) {
    test(`every row kind under ${prefix}`, () => {
      const { tree, top } = grow({
        tag: prefix,
        children: [
          { tag: `${prefix}.Label`, props: { title: "Kind", text: "File" } },
          { tag: `${prefix}.Separator` },
          {
            tag: `${prefix}.Link`,
            props: { title: "Home", text: "example", target: "https://example.invalid" },
          },
          {
            tag: `${prefix}.TagList`,
            props: { title: "Tags" },
            children: [
              { tag: `${prefix}.TagList.Item`, props: { text: "fast", color: "raycast-green" } },
            ],
          },
        ],
      });

      expect(metadataOf(tree, top()!)).toEqual([
        { kind: "label", title: "Kind", text: "File", icon: undefined },
        { kind: "separator" },
        {
          kind: "link",
          title: "Home",
          text: "example",
          url: "https://example.invalid",
        },
        {
          kind: "tags",
          title: "Tags",
          tags: [{ text: "fast", tint: "var(--success)", icon: undefined }],
        },
      ]);
    });
  }
});

describe("a list that says there is more of it", () => {
  /*
   * Every list longer than one request is written this way: the extension
   * renders what it has, says whether there is more, and waits to be asked.
   * A launcher that never reads this draws the first page and calls it the
   * whole answer, which reads as an extension that only found twenty things.
   */
  test("its three parts are carried, and the handler is the asking", () => {
    const { tree, top } = grow({
      tag: "List",
      props: {
        pagination: { pageSize: 20, hasMore: true, onLoadMore: { $handler: "h3" } },
      },
    });

    expect(paginationOf(top()!)).toEqual({ hasMore: true, pageSize: 20, onLoadMore: "h3" });
  });

  /*
   * The end of a list, which is the state that has to be told apart from the
   * middle of one: `hasMore: false` means asking again brings nothing, and a
   * launcher that asked anyway would spin on the last page forever.
   */
  test("a list at its end says so", () => {
    const { tree, top } = grow({
      tag: "List",
      props: { pagination: { pageSize: 20, hasMore: false, onLoadMore: { $handler: "h3" } } },
    });

    expect(paginationOf(top()!)?.hasMore).toBe(false);
  });

  test("a list that never mentioned it has none", () => {
    const { tree, top } = grow({ tag: "List" });
    expect(paginationOf(top()!)).toBeUndefined();
  });

  /*
   * Absent is not false. A `hasMore` nobody set is a list that is not paginated
   * rather than one that has run out, and a `pageSize` nobody set is a number
   * this must not invent.
   */
  test("what an extension left out is not guessed at", () => {
    const { tree, top } = grow({ tag: "List", props: { pagination: {} } });

    expect(paginationOf(top()!)).toEqual({ hasMore: false, pageSize: 0, onLoadMore: undefined });
  });

  /* A handler that is not one cannot be activated, and pretending otherwise
     would have the window asking a worker to run an id it never registered. */
  test("a handler that is not a handler is not carried", () => {
    const { tree, top } = grow({
      tag: "List",
      props: { pagination: { hasMore: true, onLoadMore: "loadMore" } },
    });

    expect(paginationOf(top()!)?.onLoadMore).toBeUndefined();
  });
});
