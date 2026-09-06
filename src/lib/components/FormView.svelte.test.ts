import { describe, expect, it, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";

import FormView from "./FormView.svelte";
import { ROOT_ID, ViewTree, type Op } from "$lib/exthost/tree";

/**
 * What a form opens on, and what it remembers.
 *
 * Raycast's `storeValue` asks the launcher to reopen a field on what somebody
 * last left it set to rather than on what its author defaulted to. Nothing
 * about that is visible from a screenshot: a field showing the default and a
 * field showing a remembered value that happens to equal the default are the
 * same picture, and a field that quietly ignores what was remembered looks
 * exactly like one that was never set.
 */

let mounted: Record<string, unknown> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

interface Node {
  tag: string;
  props?: Record<string, unknown>;
  children?: Node[];
}

/** Builds a tree from a nested description, the way the op stream would. */
function grow(spec: Node): ViewTree {
  const tree = new ViewTree();
  const ops: Op[] = [];
  let next = 1;

  const add = (node: Node, parent: number) => {
    const id = next++;
    ops.push({ op: "create", id, $t: node.tag, props: node.props ?? {} });
    ops.push({ op: "append", parent, child: id });
    for (const child of node.children ?? []) add(child, id);
  };

  add(spec, ROOT_ID);
  tree.apply(ops);
  return tree;
}

/** Mounts the form over a tree and hands back what submitting collects. */
function form(spec: Node, stored: Record<string, unknown> = {}) {
  const tree = grow(spec);
  const submitted: Record<string, unknown>[] = [];
  const remembered: { id: string; value: unknown }[] = [];

  const view = mount(FormView, {
    target: document.body,
    props: {
      tree,
      node: tree.top()!,
      version: 1,
      session: "s1",
      stored,
      onsubmit: (values: Record<string, unknown>) => submitted.push(values),
      onremember: (id: string, value: unknown) => remembered.push({ id, value }),
    },
  }) as Record<string, unknown> & { submit: () => void };

  mounted = view;
  flushSync();

  return { view, submitted, remembered };
}

const field = (tag: string, props: Record<string, unknown>): Node => ({ tag, props });

describe("a field the extension asked to have remembered", () => {
  it("opens on what was left there rather than on the author's default", () => {
    const { view, submitted } = form(
      { tag: "Form", children: [field("Form.TextField", { id: "who", defaultValue: "nobody", storeValue: true })] },
      { who: "somebody" },
    );

    view.submit();
    expect(submitted[0]).toEqual({ who: "somebody" });
  });

  /*
   * The default is not wrong, it is only second. A command run for the first
   * time has nothing remembered and has to open on what its author chose.
   */
  it("falls back to the default when nothing was remembered", () => {
    const { view, submitted } = form({
      tag: "Form",
      children: [field("Form.TextField", { id: "who", defaultValue: "nobody", storeValue: true })],
    });

    view.submit();
    expect(submitted[0]).toEqual({ who: "nobody" });
  });

  /*
   * A field that did not ask keeps its author's default however much is
   * remembered about it, or turning the setting off would not turn it off.
   */
  it("is ignored by a field that never asked for it", () => {
    const { view, submitted } = form(
      { tag: "Form", children: [field("Form.TextField", { id: "who", defaultValue: "nobody" })] },
      { who: "somebody" },
    );

    view.submit();
    expect(submitted[0]).toEqual({ who: "nobody" });
  });

  /*
   * A key that is not the same one next time is not a memory. An unnamed
   * field falls back to its node number, which changes between runs, so
   * remembering under it would restore the wrong field or nothing at all.
   */
  it("does nothing for a field that never named itself", () => {
    const { view, remembered } = form({
      tag: "Form",
      children: [field("Form.TextField", { defaultValue: "nobody", storeValue: true })],
    });

    view.submit();
    expect(remembered).toEqual([]);
  });

  /* On submit, not on change: a half-typed field is not a choice somebody
     made, and writing per keystroke would be a write per keystroke. */
  it("is remembered when the form is submitted, and only then", () => {
    const { view, remembered } = form(
      {
        tag: "Form",
        children: [
          field("Form.TextField", { id: "who", defaultValue: "nobody", storeValue: true }),
          field("Form.Checkbox", { id: "also", label: "Also", defaultValue: false }),
        ],
      },
      { who: "somebody" },
    );

    expect(remembered, "nothing is written before the form is submitted").toEqual([]);

    view.submit();
    expect(remembered).toEqual([{ id: "who", value: "somebody" }]);
  });

  /* Every kind of field, because each one seeds itself differently and each
     one could have been left out of the change. */
  it("works for the kinds of field that are not text", () => {
    const { view, submitted } = form(
      {
        tag: "Form",
        children: [
          field("Form.Checkbox", { id: "on", label: "On", defaultValue: false, storeValue: true }),
          field("Form.TagPicker", { id: "tags", defaultValue: [], storeValue: true }),
          {
            tag: "Form.Dropdown",
            props: { id: "pick", storeValue: true },
            children: [
              field("Form.Dropdown.Item", { value: "a", title: "A" }),
              field("Form.Dropdown.Item", { value: "b", title: "B" }),
            ],
          },
        ],
      },
      { on: true, tags: ["x"], pick: "b" },
    );

    view.submit();
    expect(submitted[0]).toEqual({ on: true, tags: ["x"], pick: "b" });
  });
});

describe("a checkbox the extension declared as ticked", () => {
  /*
   * Found by a `storeValue` test and older than it.
   *
   * The control used to be bound two ways, so it wrote its own unticked state
   * into the collected values as it mounted, before the seeding had run. The
   * key then existed, the seeding skipped it, and a box the extension asked
   * for ticked drew unticked and submitted `false`. Nothing on screen said so:
   * an unticked box is exactly what an unticked box looks like.
   */
  it("draws ticked and submits true", () => {
    const { view, submitted } = form({
      tag: "Form",
      children: [field("Form.Checkbox", { id: "on", label: "On", defaultValue: true })],
    });

    const box = document.querySelector<HTMLInputElement>('input[type="checkbox"]');
    expect(box?.checked, "the box the extension asked for ticked").toBe(true);

    view.submit();
    expect(submitted[0]).toEqual({ on: true });
  });

  it("still reports what somebody unticked", () => {
    const { view, submitted } = form({
      tag: "Form",
      children: [field("Form.Checkbox", { id: "on", label: "On", defaultValue: true })],
    });

    const box = document.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    box.checked = false;
    box.dispatchEvent(new Event("change", { bubbles: true }));
    flushSync();

    view.submit();
    expect(submitted[0]).toEqual({ on: false });
  });
});
