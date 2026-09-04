import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, unmount } from "svelte";

/**
 * The notes window, rendered.
 *
 * Two things about it can only be checked here, and both are silent when they
 * are wrong. The window can only be reached through the launcher, so neither
 * would be noticed until somebody had lost a paragraph.
 *
 * - **The window writes what is in it before it goes away.** The saving is
 *   debounced, so the last thing typed is outstanding exactly when somebody
 *   closes the window.
 * - **The window opens on a note that arrives after it has loaded.** Rust
 *   builds the window and then says which note, in that order, so a page that
 *   only asked once on mount would show an empty note and then save it over
 *   the real one.
 *
 * Tauri is mocked at the module boundary rather than at `invoke`, because
 * `$lib/settings` reaches for a good deal more than this window needs and none
 * of it is what is being tested.
 */

const written: Array<{ id: string; text: string }> = [];
const stored = new Map<string, string>();
let arrive: ((id: string) => void) | null = null;

vi.mock("$lib/notes", async (original) => {
  const real = await original<typeof import("$lib/notes")>();

  return {
    ...real,
    readNote: async (id: string) =>
      stored.has(id) ? { id, text: stored.get(id) ?? "", created: 1, updated: 1 } : null,
    writeNote: async (id: string, text: string) => {
      const at = id || `note-${stored.size + 1}`;
      written.push({ id: at, text });
      stored.set(at, text);
      return { id: at, text, created: 1, updated: 2 };
    },
    forgetNote: async (id: string) => stored.delete(id),
  };
});

vi.mock("$lib/settings", () => ({
  getPreferences: async () => ({}),
  applyAppearance: () => {},
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, handler: (event: { payload: string }) => void) => {
    arrive = (id: string) => handler({ payload: id });
    return () => {};
  },
}));

import Page from "./+page.svelte";

let mounted: Record<string, unknown> | null = null;

beforeEach(() => {
  written.length = 0;
  stored.clear();
  arrive = null;
});

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

function draw(): HTMLTextAreaElement {
  const target = document.createElement("div");
  document.body.append(target);
  mounted = mount(Page, { target });

  const area = target.querySelector("textarea");
  if (!area) throw new Error("the note window drew no field to type into");
  return area as HTMLTextAreaElement;
}

/** What the page reports about whether the note is safe. */
function standing(): string {
  return document.querySelector(".standing")?.textContent?.trim() ?? "";
}

function type(area: HTMLTextAreaElement, text: string): void {
  area.value = text;
  area.dispatchEvent(new Event("input", { bubbles: true }));
}

describe("the notes window", () => {
  it("opens on a note that arrives after the page has loaded", async () => {
    stored.set("note-7", "what was already written");
    const area = draw();

    await vi.waitFor(() => expect(arrive).not.toBeNull());
    arrive?.("note-7");

    await vi.waitFor(() => expect(area.value).toBe("what was already written"));
  });

  /**
   * The reason `beforeunload` is listened for at all.
   *
   * Typing and then closing inside the debounce is the ordinary way somebody
   * finishes with a note, and the timer belongs to the page that is going.
   */
  it("writes what is outstanding when the window is torn down", async () => {
    stored.set("note-7", "start");
    const area = draw();

    await vi.waitFor(() => expect(arrive).not.toBeNull());
    arrive?.("note-7");
    await vi.waitFor(() => expect(area.value).toBe("start"));

    type(area, "start and the last line");
    await vi.waitFor(() => expect(standing()).toBe("Not saved yet"));

    // Nothing has been written: the debounce has not fired and will not,
    // because the page is about to go.
    expect(written).toEqual([]);

    window.dispatchEvent(new Event("beforeunload"));

    /*
     * Sooner than the debounce could possibly have fired, and that bound is
     * the whole test.
     *
     * Written first with the default one second wait, and the sabotage passed:
     * deleting the `beforeunload` listener entirely still went green, because
     * the page's own 700 ms timer fired inside the second and wrote the note.
     * The test was watching the debounce it was meant to be proving is not
     * enough. A window that is closing does not get 700 ms.
     */
    await vi.waitFor(
      () => expect(written).toEqual([{ id: "note-7", text: "start and the last line" }]),
      { timeout: 200, interval: 5 },
    );
  });

  /** Opening a note and closing it again writes nothing. */
  it("does not rewrite a note somebody only read", async () => {
    stored.set("note-7", "untouched");
    const area = draw();

    await vi.waitFor(() => expect(arrive).not.toBeNull());
    arrive?.("note-7");
    await vi.waitFor(() => expect(area.value).toBe("untouched"));

    window.dispatchEvent(new Event("beforeunload"));

    // A moment for anything queued to have happened.
    await Promise.resolve();
    expect(written).toEqual([]);
  });

  /** A note that has gone says so, and what is typed becomes a new one. */
  it("says when the note it was opened on is already gone", async () => {
    draw();

    await vi.waitFor(() => expect(arrive).not.toBeNull());
    arrive?.("note-that-was-deleted");

    await vi.waitFor(() =>
      expect(standing()).toBe("That note is gone. What you type here will be a new one."),
    );
  });

  /** Nothing to delete means nothing to press. */
  it("offers no delete until there is a note to delete", async () => {
    draw();

    const button = document.querySelector("button");
    expect(button?.hasAttribute("disabled")).toBe(true);

    await vi.waitFor(() => expect(arrive).not.toBeNull());
    stored.set("note-7", "something");
    arrive?.("note-7");

    await vi.waitFor(() => expect(button?.hasAttribute("disabled")).toBe(false));
  });
});
