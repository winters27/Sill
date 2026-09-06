import { describe, expect, it, afterEach } from "vitest";
import { flushSync, mount, unmount } from "svelte";

import ExtDropdown from "./ExtDropdown.svelte";
import type { Dropdown } from "$lib/exthost/present";

/**
 * The picker beside the search field, and the one thing about it that cannot
 * be seen by looking.
 *
 * A dropdown that draws correctly and never says what it opened on is
 * indistinguishable on screen from one that works: the control shows the
 * right feed, the options are all there, and the list below it is empty. The
 * extension is not broken and has not failed. It is waiting to be told which
 * of its fifteen feeds it is on, because in Raycast the launcher is what
 * knows that, and it had not said.
 *
 * That was Hacker News, and it is the shape of every command written as
 * `usePromise(fetch, [choice], { execute: !!choice })`.
 */

let mounted: Record<string, unknown> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

/** A picker with two feeds, and whatever the extension said about them. */
function dropdown(extra: Partial<Dropdown> = {}): Dropdown {
  return {
    id: 7,
    storeValue: false,
    tooltip: "Select Page",
    onChange: "h1",
    options: [
      { value: "frontpage", title: "Front Page" },
      { value: "newest", title: "Newest" },
    ],
    ...extra,
  };
}

/** Mounts it and collects everything it reports, in order. */
function show(props: Dropdown): string[] {
  const picked: string[] = [];
  mounted = mount(ExtDropdown, {
    target: document.body,
    props: { dropdown: props, onpick: (value: string) => picked.push(value) },
  });
  flushSync();
  return picked;
}

describe("what the extension is told before anybody touches it", () => {
  it("reports the start the extension suggested", () => {
    expect(show(dropdown({ initial: "newest" }))).toEqual(["newest"]);
  });

  /*
   * A picker with no opinion still opens on something, and that something is
   * as much news to the extension as a default would have been. A select with
   * no value lands on its first option whatever anybody intended, so the only
   * question is whether the extension is told what the person can already see.
   */
  it("reports the first option when the extension suggested nothing", () => {
    expect(show(dropdown())).toEqual(["frontpage"]);
  });

  /*
   * An extension passing `value` is driving the picker: it chose the value,
   * it already knows, and telling it would be answering a question nobody
   * asked. Worse, an extension that sets state from `onChange` would be told
   * its own value, set it again, and render again.
   */
  it("says nothing to an extension driving it", () => {
    expect(show(dropdown({ value: "newest" }))).toEqual([]);
  });

  it("says nothing when there is nothing to choose from", () => {
    expect(show(dropdown({ options: [] }))).toEqual([]);
  });

  /*
   * Once, not once per render. The extension answering by setting state is
   * the ordinary case, and each answer redraws this: a report on every draw
   * would be an extension told the same thing forever.
   */
  it("reports what it opened on once, however often it is redrawn", () => {
    const picked: string[] = [];
    const props = $state({ dropdown: dropdown({ initial: "newest" }), onpick: (v: string) => picked.push(v) });

    mounted = mount(ExtDropdown, { target: document.body, props });
    flushSync();

    // The same picker again, which is what a re-render hands over.
    props.dropdown = dropdown({ initial: "newest" });
    flushSync();
    props.dropdown = dropdown({ initial: "newest" });
    flushSync();

    expect(picked).toEqual(["newest"]);
  });

  /*
   * A person choosing is already reported by the control's own change event.
   * The effect that reports the opening value must not report it a second
   * time, or one press of a feed name asks the extension for it twice.
   */
  it("does not repeat a choice the person just made", () => {
    const picked = show(dropdown({ initial: "frontpage" }));
    expect(picked).toEqual(["frontpage"]);

    const select = document.querySelector("select");
    expect(select).not.toBeNull();
    select!.value = "newest";
    // Bubbling, because Svelte delegates this one to the root: an event that
    // does not travel is an event the component never hears, and the test
    // would pass while proving nothing.
    select!.dispatchEvent(new Event("change", { bubbles: true }));
    flushSync();

    expect(picked).toEqual(["frontpage", "newest"]);
  });

  /*
   * A different command's picker is a different question, so it is asked.
   * Keyed by the node rather than by the value, because two commands can
   * perfectly well open on the same word.
   */
  it("asks again for a different picker", () => {
    const picked: string[] = [];
    const props = $state({
      dropdown: dropdown({ id: 7, initial: "newest" }),
      onpick: (v: string) => picked.push(v),
    });

    mounted = mount(ExtDropdown, { target: document.body, props });
    flushSync();

    props.dropdown = dropdown({ id: 9, initial: "newest" });
    flushSync();

    expect(picked).toEqual(["newest", "newest"]);
  });
});
