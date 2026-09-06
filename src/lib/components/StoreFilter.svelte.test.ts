/**
 * The store's filter control: what it offers, what it says, what a pick means.
 *
 * The control draws state the store view reports and hands back a value the
 * view already understands. What is worth holding is the vocabulary between
 * the two, because a value the control emits that the view does not read is
 * a menu item that does nothing, and that is invisible from either side.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StoreFilterState } from "$lib/store";

vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => null }));

const STATE: StoreFilterState = {
  scope: "all",
  category: null,
  categories: [
    { name: "Developer Tools", count: 12, mark: "Code" },
    { name: "Media", count: 3, mark: "Image" },
  ],
  updates: 2,
};

let cleanup: (() => void) | null = null;

async function control(filter: StoreFilterState, onpick = vi.fn()) {
  const { mount, unmount, tick } = await import("svelte");
  const StoreFilter = (await import("./StoreFilter.svelte")).default;

  const target = document.createElement("div");
  document.body.append(target);
  const mounted = mount(StoreFilter, { target, props: { filter, onpick } });
  cleanup = () => {
    unmount(mounted);
    target.remove();
  };
  await tick();
  return { target, tick, onpick };
}

afterEach(() => {
  cleanup?.();
  cleanup = null;
});

describe("the store's filter", () => {
  it("says what is shown, and opens onto scopes and categories with their marks", async () => {
    const { target, tick } = await control(STATE);

    const trigger = target.querySelector<HTMLButtonElement>(".trigger");
    expect(trigger?.textContent).toContain("All");
    expect(target.querySelector('[role="menu"]')).toBeNull();

    trigger?.click();
    await tick();

    const options = [...target.querySelectorAll<HTMLElement>(".option")];
    expect(options.map((one) => one.querySelector(".text")?.textContent)).toEqual([
      "All",
      "Installed",
      "Updates",
      "Any",
      "Developer Tools",
      "Media",
    ]);
    // Every category carries its mark, drawn rather than named.
    expect(options[4].querySelector("svg")).not.toBeNull();
    // The update count is on its scope.
    expect(options[2].querySelector(".count")?.textContent).toBe("2");
    // What is chosen is checked, for a reader that cannot see a fill.
    expect(options[0].getAttribute("aria-checked")).toBe("true");
    expect(options[3].getAttribute("aria-checked")).toBe("true");
  });

  it("a pick is reported in the store view's vocabulary and closes the menu", async () => {
    const { target, tick, onpick } = await control(STATE);
    target.querySelector<HTMLButtonElement>(".trigger")?.click();
    await tick();

    const options = [...target.querySelectorAll<HTMLElement>(".option")];
    options[5].click();
    await tick();
    expect(onpick).toHaveBeenCalledWith("category:Media");
    expect(target.querySelector('[role="menu"]')).toBeNull();

    target.querySelector<HTMLButtonElement>(".trigger")?.click();
    await tick();
    [...target.querySelectorAll<HTMLElement>(".option")][1].click();
    expect(onpick).toHaveBeenCalledWith("scope:installed");
  });

  it("names the category on the trigger when one is chosen", async () => {
    const { target } = await control({ ...STATE, category: "Media", scope: "installed" });
    expect(target.querySelector(".trigger")?.textContent).toContain("Media");
  });

  it("Escape closes the menu and returns to the trigger", async () => {
    const { target, tick } = await control(STATE);
    const trigger = target.querySelector<HTMLButtonElement>(".trigger");
    trigger?.click();
    await tick();

    const menu = target.querySelector<HTMLElement>('[role="menu"]');
    menu?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await tick();
    expect(target.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
});
