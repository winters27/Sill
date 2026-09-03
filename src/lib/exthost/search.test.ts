/**
 * The search field's four promises to an extension.
 *
 * `filtering`, `onSearchTextChange`, `throttle` and `isLoading` were all read
 * by nobody, so typing in an extension's list reached nothing at all. These
 * hold the four rules that make it reach something, and the two that make it
 * reach something correct: one flattening of the rows so the highlight and
 * Enter cannot disagree about which one is under the cursor, and one rate
 * limit whose queued call always carries the newest text rather than the text
 * that was current when its timer was armed.
 */
import { afterEach, describe, expect, test, vi } from "vitest";
import { ROOT_ID, ViewTree, type ElementNode, type Op } from "$lib/exthost/tree";
import {
  itemsOf,
  matches,
  rowsOf,
  searchProps,
  SearchRelay,
  THROTTLE_MS,
} from "$lib/exthost/search";

/** What one node looks like before it is turned into an op stream. */
interface Spec {
  tag: string;
  props?: Record<string, unknown>;
  children?: Spec[];
}

/**
 * Builds a tree the way the host builds one, which is by ops.
 *
 * Not by hand-writing `ElementNode` objects: the shape these functions read is
 * whatever `ViewTree.apply` produces, and a fixture that agrees with the type
 * but not with the reconciler would pass while the real thing failed.
 */
function build(spec: Spec): { tree: ViewTree; node: ElementNode } {
  const tree = new ViewTree();
  const ops: Op[] = [];
  let id = 0;

  const walk = (one: Spec, parent: number) => {
    const mine = ++id;
    ops.push({ op: "create", id: mine, $t: one.tag, props: one.props ?? {} });
    ops.push({ op: "append", parent, child: mine });
    for (const child of one.children ?? []) walk(child, mine);
  };

  walk(spec, ROOT_ID);
  tree.apply(ops);

  const node = tree.top();
  if (!node) throw new Error("the fixture rendered nothing");
  return { tree, node };
}

const item = (title: string, props: Record<string, unknown> = {}): Spec => ({
  tag: "List.Item",
  props: { title, ...props },
});

const titles = (rows: ReturnType<typeof rowsOf>): string[] =>
  itemsOf(rows).map((one) => String(one.props.title));

describe("who filters", () => {
  /*
   * The default that makes an unremarkable list searchable. Nearly every
   * extension in the store renders `<List>` with nothing on it and expects
   * Raycast to narrow the rows.
   */
  test("a list that said nothing is filtered by Sill", () => {
    const { node } = build({ tag: "List" });
    expect(searchProps(node).filtering).toBe(true);
  });

  /*
   * The other half, and the one that was worth getting right. An extension
   * that registered a handler is announcing that it intends to search, so Sill
   * filtering as well would hide rows it went and fetched.
   */
  test("a list that listens is not filtered by Sill", () => {
    const { node } = build({
      tag: "List",
      props: { onSearchTextChange: { $handler: "h4" } },
    });

    const props = searchProps(node);
    expect(props.filtering).toBe(false);
    expect(props.onChange).toBe("h4");
  });

  /*
   * Both, which Raycast allows: fetch on typing and let the launcher narrow
   * what comes back. The handler is still relayed, because Raycast calls it
   * whenever it is registered.
   */
  test("a list that listens and asks for filtering gets both", () => {
    const { node } = build({
      tag: "List",
      props: { filtering: true, onSearchTextChange: { $handler: "h9" } },
    });

    const props = searchProps(node);
    expect(props.filtering).toBe(true);
    expect(props.onChange).toBe("h9");
  });

  /* `filtering={{ keepSectionOrder: true }}` is still a request to filter. */
  test("filtering given as an object is still filtering", () => {
    const { node } = build({ tag: "List", props: { filtering: { keepSectionOrder: true } } });
    expect(searchProps(node).filtering).toBe(true);
  });

  test("a list that refused filtering and listens to nothing ignores typing", () => {
    const { node } = build({ tag: "List", props: { filtering: false } });
    const props = searchProps(node);
    expect(props.filtering).toBe(false);
    expect(props.onChange).toBeUndefined();
  });

  test("throttle and isLoading are read", () => {
    const { node } = build({ tag: "List", props: { throttle: true, isLoading: true } });
    const props = searchProps(node);
    expect(props.throttle).toBe(true);
    expect(props.loading).toBe(true);
  });

  test("nothing rendered yet asks for nothing", () => {
    expect(searchProps(undefined)).toEqual({ filtering: false, throttle: false, loading: false });
  });
});

describe("what a row is matched on", () => {
  test("the title, whatever case it was typed in", () => {
    expect(matches(build(item("Gerald")).node, "ERA")).toBe(true);
  });

  test("the subtitle", () => {
    expect(matches(build(item("One", { subtitle: "Berlin" })).node, "berl")).toBe(true);
  });

  test("the keywords, which are how an extension makes a row findable", () => {
    expect(matches(build(item("🥳", { keywords: ["party", "tada"] })).node, "tada")).toBe(true);
  });

  /*
   * Raycast lets a title be `{ value, tooltip }`. Read as a plain string that
   * is an empty haystack, so every row an extension wrote that way vanished on
   * the first keystroke.
   */
  test("a title written as an object still matches", () => {
    const { node } = build(item("", { title: { value: "Gerald", tooltip: "who" } }));
    expect(matches(node, "gerald")).toBe(true);
  });

  test("an empty query keeps everything", () => {
    expect(matches(build(item("anything")).node, "")).toBe(true);
  });

  test("a word in none of the three does not match", () => {
    expect(matches(build(item("Gerald", { subtitle: "Berlin" })).node, "paris")).toBe(false);
  });
});

describe("the one flattened list", () => {
  const grouped: Spec = {
    tag: "List",
    children: [
      item("loose one"),
      {
        tag: "List.Section",
        props: { title: "Fruit" },
        children: [item("apple"), item("apricot")],
      },
      {
        tag: "List.Section",
        props: { title: "Vegetables" },
        children: [item("leek")],
      },
    ],
  };

  test("sections and loose items share one index space", () => {
    const { tree, node } = build(grouped);
    const rows = rowsOf(tree, node, "");

    expect(rows.map((row) => row.kind)).toEqual([
      "item",
      "section",
      "item",
      "item",
      "section",
      "item",
    ]);
    expect(itemsOf(rows).length).toBe(4);
    expect(rows.filter((row) => row.kind === "item").map((row) => row.index)).toEqual([0, 1, 2, 3]);
  });

  /*
   * The property that keeps Enter honest.
   *
   * Index 0 has to be the first row drawn, not the first row rendered. When
   * the view narrowed its own copy and the page did not, the highlight sat on
   * one row and Enter ran another, and only while something was typed.
   */
  test("narrowing renumbers, so index 0 is the first row on screen", () => {
    const { tree, node } = build(grouped);
    const rows = rowsOf(tree, node, "ap");

    expect(titles(rows)).toEqual(["apple", "apricot"]);
    expect(rows.filter((row) => row.kind === "item").map((row) => row.index)).toEqual([0, 1]);
  });

  /*
   * The same property where it is easiest to get wrong: a row dropped from
   * inside a section, so the survivor after it has to move up. Counting the
   * dropped one leaves index 0 pointing at nothing and every row below it one
   * place out, which is Enter running its neighbour.
   */
  test("a row dropped from inside a section moves the ones after it up", () => {
    const { tree, node } = build(grouped);
    const rows = rowsOf(tree, node, "apricot");

    expect(titles(rows)).toEqual(["apricot"]);
    expect(rows.filter((row) => row.kind === "item").map((row) => row.index)).toEqual([0]);
  });

  test("a section that lost every item goes with them", () => {
    const { tree, node } = build(grouped);
    const rows = rowsOf(tree, node, "leek");

    expect(rows.map((row) => row.kind)).toEqual(["section", "item"]);
    expect(String((rows[0].node as ElementNode).props.title)).toBe("Vegetables");
    // Everything above it went, so the one row left is the first row.
    expect(rows.filter((row) => row.kind === "item").map((row) => row.index)).toEqual([0]);
  });

  test("a grid is flattened by its own tags", () => {
    const { tree, node } = build({
      tag: "Grid",
      children: [
        { tag: "Grid.Item", props: { title: "one" } },
        {
          tag: "Grid.Section",
          props: { title: "More" },
          children: [{ tag: "Grid.Item", props: { title: "two" } }],
        },
      ],
    });

    expect(titles(rowsOf(tree, node, ""))).toEqual(["one", "two"]);
    expect(titles(rowsOf(tree, node, "two"))).toEqual(["two"]);
  });
});

describe("what the extension hears, and how often", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** A relay plus the log of everything it sent. */
  function relaying(over: { fails?: boolean } = {}) {
    const sent: string[] = [];
    const reported: unknown[] = [];
    const relay = new SearchRelay({
      send: (text) => {
        sent.push(text);
        return over.fails ? Promise.reject(new Error(`no: ${text}`)) : Promise.resolve(null);
      },
      failed: (err) => reported.push(err),
    });
    return { relay, sent, reported };
  }

  test("without throttling every keystroke reaches the extension", () => {
    const { relay, sent } = relaying();
    for (const text of ["g", "gi", "git"]) relay.offer(text, false);
    expect(sent).toEqual(["g", "gi", "git"]);
  });

  /*
   * The effect that drives this re-runs whenever anything on the rendered node
   * changes, `isLoading` among them. Without this an extension that renders in
   * answer to typing is asked again about the text it already has, once per
   * render, for as long as it keeps rendering.
   */
  test("text the extension already has is not sent again", () => {
    const { relay, sent } = relaying();
    relay.offer("git", false);
    relay.offer("git", false);
    expect(sent).toEqual(["git"]);
  });

  test("a throttled extension hears the first keystroke immediately", () => {
    vi.useFakeTimers();
    const { relay, sent } = relaying();
    relay.offer("g", true);
    expect(sent).toEqual(["g"]);
  });

  /*
   * What `throttle` is for. A burst inside one window is one further call, and
   * it carries the last thing typed rather than the first thing queued: a
   * timer armed on `gi` that fires with `gi` while the field says `github` is
   * the stale answer arriving by the front door.
   */
  test("a burst costs one further call, carrying the newest text", () => {
    vi.useFakeTimers();
    const { relay, sent } = relaying();

    relay.offer("g", true);
    vi.advanceTimersByTime(40);
    relay.offer("gi", true);
    vi.advanceTimersByTime(40);
    relay.offer("git", true);
    vi.advanceTimersByTime(40);
    relay.offer("github", true);

    expect(sent).toEqual(["g"]);
    vi.advanceTimersByTime(THROTTLE_MS);
    expect(sent).toEqual(["g", "github"]);
  });

  /*
   * A throttle, not a debounce. The deadline belongs to the window that was
   * opened, so somebody who keeps typing still gets an answer every window;
   * pushing the timer back on each keystroke means never answering at all
   * until they stop.
   */
  test("typing on does not push the answer back for ever", () => {
    vi.useFakeTimers();
    const { relay, sent } = relaying();

    relay.offer("a", true);
    for (let n = 0; n < 10; n += 1) {
      vi.advanceTimersByTime(50);
      relay.offer(`a${"b".repeat(n + 1)}`, true);
    }

    expect(sent.length).toBeGreaterThan(1);
  });

  test("leaving the command drops the call that was waiting", () => {
    vi.useFakeTimers();
    const { relay, sent } = relaying();

    relay.offer("g", true);
    relay.offer("gi", true);
    relay.cancel();
    vi.advanceTimersByTime(THROTTLE_MS * 4);

    expect(sent).toEqual(["g"]);
  });

  /*
   * `UI/setSearchText` writes the field. Relaying that back is Sill telling
   * the extension its own words, and for one that sets the text from inside
   * `onSearchTextChange` it is a loop.
   */
  test("text the extension set itself is not read back to it", () => {
    const { relay, sent } = relaying();
    relay.adopt("preset");
    relay.offer("preset", false);
    expect(sent).toEqual([]);
  });

  /*
   * The same rule the root search keeps with `searchId` and the store with
   * `generation`: a slow refusal for text somebody has already typed past is
   * not news about anything on screen.
   */
  test("only the newest call may report a failure", async () => {
    const { relay, reported } = relaying({ fails: true });

    relay.offer("git", false);
    relay.offer("github", false);
    await Promise.resolve();
    await Promise.resolve();

    expect(reported).toHaveLength(1);
    expect(String(reported[0])).toContain("github");
  });

  test("a command that has been left reports nothing at all", async () => {
    const { relay, reported } = relaying({ fails: true });

    relay.offer("git", false);
    relay.cancel();
    await Promise.resolve();
    await Promise.resolve();

    expect(reported).toEqual([]);
  });
});
