import { describe, expect, it, afterEach } from "vitest";
import { mount, tick, unmount } from "svelte";
import RootList from "./RootList.svelte";
import type { RankedCommand } from "$lib/exthost/commands";

/**
 * The result list, rendered.
 *
 * `P7-03` names this gap: nothing here could render a component, so anything
 * a component decides for itself was checked by looking at it. Two of the
 * bugs this codebase keeps meeting live exactly there, and neither could be
 * caught by a build:
 *
 * - **A duplicate key in a keyed `{#each}` silently drops a row.** This was
 *   written down here as "throws and blanks the whole list", and measured
 *   against Svelte 5.56 it does neither: it draws one row where two were
 *   given, on the first render and on an update, with nothing thrown and
 *   nothing logged. Quieter than the belief, and worse for it: a result the
 *   person searched for is simply not in the list.
 * - **A row that draws but never changes is keyed by the wrong identity.**
 */

let mounted: Record<string, unknown> | null = null;

afterEach(() => {
  if (mounted) unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

/** One result row, with only the fields the list actually reads. */
function row(id: string, title: string, mode = "app"): RankedCommand {
  return {
    id,
    extension: "app",
    extensionTitle: "Application",
    title,
    subtitle: "",
    mode,
    entrypoint: `C:/programs/${id}.exe`,
    icon: "",
    matched: [],
  } as RankedCommand;
}

function draw(
  commands: RankedCommand[],
  selected = 0,
  working: { line: string; far: number | null } | null = null,
  outcome = "",
) {
  const target = document.createElement("div");
  document.body.append(target);

  mounted = mount(RootList, {
    target,
    props: {
      commands,
      selected,
      onselect: () => {},
      onrun: () => {},
      working,
      outcome,
    },
  });

  return target;
}

describe("drawing the result list", () => {
  /**
   * Pressing the row used to close the launcher, which is what a crash looks
   * like. The work is Rust's and takes tens of seconds, so the row it was
   * started from is where it reports.
   */
  it("draws a bar on the row that started an update", () => {
    const target = draw(
      [row("sill:extensions-behind", "Update 2 extensions", "extensions-behind")],
      0,
      { line: "Building run (1 of 4)", far: 0.5 },
    );

    const bar = target.querySelector('[role="progressbar"]');
    expect(bar).not.toBeNull();
    expect(bar?.getAttribute("aria-valuenow")).toBe("50");
    expect(target.textContent).toContain("Building run (1 of 4)");
  });

  /**
   * npm never says how much of itself is left and it is the longest stage.
   * A bar stuck at zero for twenty seconds reads as broken, so it sweeps and
   * reports no value rather than claiming one.
   */
  it("says nothing about how far when nothing knows", () => {
    const target = draw(
      [row("sill:extensions-behind", "Update 1 extension", "extensions-behind")],
      0,
      { line: "Dependencies: added 40 packages", far: null },
    );

    const bar = target.querySelector('[role="progressbar"]');
    expect(bar?.hasAttribute("aria-valuenow")).toBe(false);
    expect(target.querySelector(".far.unknown")).not.toBeNull();
  });

  /**
   * The reason goes on the row, not in the chin.
   *
   * Measured: the chin cut "Hacker News could not be updated. npm is not
   * beside the Node at ..." down to "Hacker News could not be updated. npm
   * ...", which is the half with nothing to act on. Its own comment says
   * prose is the item that gives way there.
   */
  it("shows what the last batch left", () => {
    const why = "Sill's Node has no npm, so dependencies cannot be installed.";
    const target = draw(
      [row("sill:extensions-behind", "Update 2 extensions", "extensions-behind")],
      0,
      null,
      `Hacker News could not be updated. ${why}`,
    );

    expect(target.textContent).toContain(why);
    expect(target.querySelector('[role="progressbar"]')).toBeNull();
  });

  /** While it runs, the progress wins: the outcome is about a batch that ended. */
  it("draws progress rather than an old outcome while a batch is running", () => {
    const target = draw(
      [row("sill:extensions-behind", "Update 2 extensions", "extensions-behind")],
      0,
      { line: "Fetching 3 of 40 files", far: 0.1 },
      "something an earlier batch said",
    );

    expect(target.textContent).toContain("Fetching 3 of 40 files");
    expect(target.textContent).not.toContain("something an earlier batch said");
  });

  /** No batch running, no bar. The row is a button again. */
  it("draws no bar when nothing is being updated", () => {
    const target = draw([
      row("sill:extensions-behind", "Update 2 extensions", "extensions-behind"),
    ]);

    expect(target.querySelector('[role="progressbar"]')).toBeNull();
  });

  /** An ordinary row is untouched by a batch running elsewhere. */
  it("puts no bar on a row that is not the update row", () => {
    const target = draw([row("app:a", "Alpha")], 0, { line: "Fetching", far: 0.2 });

    expect(target.querySelector('[role="progressbar"]')).toBeNull();
  });

  it("draws a row for everything it was given", () => {
    const target = draw([row("app:a", "Alpha"), row("app:b", "Beta")]);

    const rows = target.querySelectorAll('[role="option"]');
    expect(rows.length).toBe(2);
    expect(target.textContent).toContain("Alpha");
    expect(target.textContent).toContain("Beta");
  });

  /**
   * The bug this file exists for, measured rather than assumed.
   *
   * Two rows with one id draws one row. Nothing is thrown, nothing is
   * logged, and the second row is simply not there. The window builds this
   * list by concatenating what Rust ranked with files and browser pages that
   * arrive later from another path, and until `show` compared them, nothing
   * looked at their ids at all.
   *
   * Pinned here as well as guarded in the window, because this is the
   * behaviour the guard exists for and a future Svelte could change it.
   */
  it("silently drops a row when two share an id", () => {
    const target = draw([row("app:same", "First"), row("app:same", "Second")]);

    expect(target.querySelectorAll('[role="option"]').length).toBe(1);
    expect(target.textContent).toContain("First");
    expect(target.textContent).not.toContain("Second");
  });

  /**
   * The field says which row is current, and it has to name one that is there.
   *
   * `P5-05` put row ids and `aria-activedescendant` in for a reason: the list
   * is a listbox the field points into, and an id pointing at nothing means a
   * screen reader announces the field and then silence.
   */
  it("gives every row an id the field can point at", () => {
    const target = draw([row("app:a", "Alpha"), row("app:b", "Beta")], 1);

    const ids = [...target.querySelectorAll('[role="option"]')].map((one) => one.id);

    expect(ids.every((one) => one.length > 0)).toBe(true);
    expect(new Set(ids).size).toBe(ids.length);

    // And the selected one is marked, which is the other half of the same
    // announcement.
    const selected = [...target.querySelectorAll('[role="option"]')].filter(
      (one) => one.getAttribute("aria-selected") === "true",
    );
    expect(selected.length).toBe(1);
    expect(selected[0].textContent).toContain("Beta");
  });

  /**
   * An empty list says why it is empty rather than showing nothing.
   *
   * A blank panel reads as the launcher being broken. `P5-03`'s one recipe
   * decides the words; this only asks that something is said.
   */
  it("says something when there is nothing to show", () => {
    const target = draw([]);

    expect(target.querySelectorAll('[role="option"]').length).toBe(0);
    expect(target.textContent?.trim().length ?? 0).toBeGreaterThan(0);
  });
});

describe("an extension command says which extension", () => {
  /** A command of somebody else's extension, as Rust writes it. */
  function ofExtension(subtitle: string): RankedCommand {
    return {
      ...row("pokedex:nature", "Nature", "view"),
      extension: "pokedex",
      extensionTitle: "Pokédex",
      subtitle,
    } as RankedCommand;
  }

  /*
   * The case that started this. The heading above says "Extensions", which
   * is a category, and the row said "Nature" and "Mechanics", which are the
   * command and its own subtitle. Nothing anywhere said Pokédex.
   */
  it("names it alongside the command's own subtitle", () => {
    const target = draw([ofExtension("Mechanics")]);
    expect(target.textContent).toContain("Pokédex · Mechanics");
  });

  /*
   * Rust already defaults the subtitle to the extension title when the
   * manifest declares none, and saying it twice would read as a mistake.
   */
  it("says it once when the subtitle is already the extension", () => {
    const target = draw([ofExtension("Pokédex")]);
    expect(target.textContent).not.toContain("·");
    expect(target.textContent).toContain("Pokédex");
  });
});

describe("what the mouse does to a row", () => {
  /**
   * The list's own `draw` throws both callbacks away, and these two tests are
   * entirely about which one was called.
   */
  function withSpies(commands: RankedCommand[]) {
    const target = document.createElement("div");
    document.body.append(target);

    const ran: number[] = [];
    const picked: number[] = [];

    mounted = mount(RootList, {
      target,
      props: {
        commands,
        selected: 0,
        onselect: (index: number) => picked.push(index),
        onrun: (index: number) => ran.push(index),
        working: null,
        outcome: "",
      },
    });

    return { target, ran, picked };
  }

  /*
   * A single click used to run the row, which left nothing for a mouse to say
   * "this one, but tell me what I can do to it" with: moving onto a row
   * already selects it, so the click had no meaning left to carry.
   *
   * Running moved to the double click. If this goes red because a single
   * click ran something, the action panel is unreachable without the keyboard
   * again, and that is the thing to notice rather than the assertion.
   */
  it("selects on a click, and runs only on a double click", () => {
    const { target, ran, picked } = withSpies([row("a", "Alpha"), row("b", "Beta")]);

    const second = target.querySelectorAll<HTMLElement>("[data-row]")[1];
    expect(second, "the list drew fewer rows than it was given").toBeTruthy();

    second!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(ran, "a single click ran the row").toEqual([]);
    expect(picked, "a single click did not select the row").toEqual([1]);

    second!.dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    expect(ran, "a double click did not run the row").toEqual([1]);
  });

  /// The row already under the pointer is already selected; saying so again
  /// would raise the pointer flag and swallow the next arrow key.
  it("says nothing when the click lands on the row already selected", () => {
    const { target, ran, picked } = withSpies([row("a", "Alpha"), row("b", "Beta")]);

    const first = target.querySelectorAll<HTMLElement>("[data-row]")[0];
    first!.dispatchEvent(new MouseEvent("click", { bubbles: true }));

    expect(picked).toEqual([]);
    expect(ran).toEqual([]);
  });
  /*
   * The bug this prevents is the one the note on `byPointer` describes: while
   * moving the mouse selected, a row half over the bottom edge was selected
   * the moment the cursor touched it, scrolling it on pulled the next row up
   * under the cursor, and the list crawled downward on its own.
   *
   * If this goes red, pointing and selecting have become the same gesture
   * again and the crawl is back.
   */
  it("points without selecting, and lights the row it points at", async () => {
    const { target, ran, picked } = withSpies([row("a", "Alpha"), row("b", "Beta")]);

    const rows = target.querySelectorAll<HTMLElement>("[data-row]");
    rows[1]!.dispatchEvent(new MouseEvent("mousemove", { bubbles: true }));
    await tick();

    expect(picked, "moving the mouse moved the selection").toEqual([]);
    expect(ran, "moving the mouse ran something").toEqual([]);

    expect(rows[1]!.classList.contains("hovered"), "the pointed row is not lit").toBe(true);
    expect(rows[0]!.classList.contains("selected"), "the selection moved").toBe(true);
    expect(
      rows[0]!.classList.contains("hovered"),
      "the selected row is lit twice over",
    ).toBe(false);
  });
});
