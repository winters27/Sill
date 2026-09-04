import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Browse, Preparation, StoreRow } from "$lib/store";

/**
 * The store's update path, which `P7-03` named as untested.
 *
 * Rust decides whether an installed extension is behind the catalogue, and
 * `store/mod.rs` covers that decision well: `outdated_against` and the
 * `updates_only` filter both have tests. **What none of them reach is whether
 * the answer is ever drawn or ever acted on**, because both live inside a
 * component and nothing here could render one.
 *
 * That gap has a shape this codebase already knows. A row that draws but never
 * changes; two lists that must agree with nothing making them agree; a pure
 * function with good tests whose only caller is somewhere no test goes. An
 * update badge nobody draws is the quietest of them: the extension is behind,
 * Rust says so, the row says "Installed", and the person never learns there is
 * anything to do.
 */

/**
 * Room for the first mount on a cold cache.
 *
 * Observed once and then fixed rather than lived with: on a run with no vite
 * cache this file's first test and the launcher page's compile in parallel
 * workers, and five seconds is not enough for both. A ceiling that a slow
 * machine can trip is a flaky gate, which teaches people to rerun rather than
 * to read. Still a ceiling: a component that genuinely never settles fails.
 */
vi.setConfig({ testTimeout: 30_000 });

const invoke = vi.fn(async (command: string, _args?: unknown) => {
  if (command === "store_browse") return browse;
  if (command === "store_prepare") return prepared;
  return null;
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: unknown) => invoke(command, args as never),
  convertFileSrc: (path: string) => path,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
  emit: async () => {},
}));

/** One listing, with only the fields the shelf reads. */
function listing(
  name: string,
  installed: { revision: string; source: string; outdated: boolean } | null,
): StoreRow {
  return {
    name,
    folder: name,
    title: name,
    description: `about ${name}`,
    author: "someone",
    categories: [],
    platforms: ["windows"],
    downloads: 10,
    revision: "bbbb",
    icon: "",
    commands: [{ name: "one", title: "One", description: "", mode: "view", runnable: true }],
    installed,
    blocked: null,
    sourceUrl: "",
    native: false,
  } as StoreRow;
}

let browse: Browse;

/** What `store_prepare` answers: fetched and read, installed nothing. */
const prepared: Preparation = {
  name: "behind",
  title: "behind",
  revision: "bbbb",
  folder: "behind",
  icon: "",
  sourceUrl: "",
  files: 3,
  bytes: 1024,
  commands: [],
  capabilities: [],
  packages: [],
  secrets: [],
  apiWarning: null,
  refused: [],
  notEnforced: "",
};

let mounted: Record<string, unknown> | null = null;

async function shelf(rows: StoreRow[]) {
  const { mount, tick } = await import("svelte");
  const StoreView = (await import("./StoreView.svelte")).default;

  browse = {
    rows,
    categories: [],
    matched: rows.length,
    total: rows.length,
    hidden: 0,
    updates: rows.filter((row) => row.installed?.outdated).length,
    fetchedAt: 0,
  };

  const target = document.createElement("div");
  document.body.append(target);

  mounted = mount(StoreView, {
    target,
    props: {
      query: "",
      selected: 0,
      onselect: () => {},
      oncount: () => {},
      onstatus: () => {},
      oncurrent: () => {},
      onchanged: () => {},
      prefs: null,
    },
  });

  // The catalogue is fetched in an effect, so the first paint is a shelf with
  // nothing on it.
  for (let i = 0; i < 8; i += 1) {
    await Promise.resolve();
    await tick();
  }

  return { target, tick };
}

beforeEach(() => {
  invoke.mockClear();
});

afterEach(async () => {
  if (mounted) {
    const { unmount } = await import("svelte");
    await unmount(mounted, { outro: false });
  }
  mounted = null;
  document.body.innerHTML = "";
});

describe("the store's update path", () => {
  /**
   * An extension the catalogue has moved past says so on its row.
   *
   * Three rows in one shelf on purpose. A test with only the outdated one
   * would pass for a component that badges everything, and "Update" on an
   * extension that is current is the same lie in the other direction.
   */
  it("badges only the installed row the catalogue has moved past", async () => {
    const { target } = await shelf([
      listing("behind", { revision: "aaaa", source: "store", outdated: true }),
      listing("current", { revision: "bbbb", source: "store", outdated: false }),
      listing("absent", null),
    ]);

    const rows = [...target.querySelectorAll<HTMLElement>('[role="option"]')];
    expect(rows.length).toBe(3);

    expect(rows[0].querySelector(".update")?.textContent).toBe("Update");
    expect(rows[0].querySelector(".have")).toBe(null);

    expect(rows[1].querySelector(".update")).toBe(null);
    expect(rows[1].querySelector(".have")?.textContent).toBe("Installed");

    expect(rows[2].querySelector(".update")).toBe(null);
    expect(rows[2].querySelector(".have")).toBe(null);
  });

  /**
   * And pressing it starts the update rather than only labelling one.
   *
   * An update is `store_prepare` then `store_install`, the same two steps an
   * install takes, which is why nothing about the row's label can be trusted
   * to mean the row does anything. This is the second half: Enter on a row
   * that is behind reaches Rust with that extension's name.
   */
  it("fetches the newer copy when the outdated row is activated", async () => {
    const { target, tick } = await shelf([
      listing("behind", { revision: "aaaa", source: "store", outdated: true }),
    ]);

    invoke.mockClear();

    const row = target.querySelector<HTMLElement>('[role="option"]');
    row?.click();

    for (let i = 0; i < 6; i += 1) {
      await Promise.resolve();
      await tick();
    }

    expect(invoke.mock.calls.map(([command]) => command)).toContain("store_prepare");
  });
});
