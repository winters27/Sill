/**
 * What the settings module does when Rust does not answer.
 *
 * Every one of these reads has a fallback, and the fallback is not the
 * problem: a pane that throws is worse than one showing an empty list. The
 * problem was that the fallback was all there was. An empty list drawn where
 * an answer belongs is indistinguishable from the answer being empty, so a
 * refused command read as "there are no search engines", "there are no
 * browsers on this machine", and worst of all "no hotkey is taken", which is
 * the reading that hides the one signal the summon-key work added.
 *
 * Tauri denies a command to a window missing from `capabilities/default.json`
 * silently. Nothing throws in Rust, nothing reaches the log, and the page
 * renders perfectly. That is how the tray menu once shipped completely dead,
 * so this is the most likely reason a settings pane is empty, not a
 * hypothetical one.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { browserProfiles, hotkeyConflicts, indexRows, navigationChords, searchEngines } =
  await import("$lib/settings");
const { statusTroubles } = await import("$lib/status");

/** What `note_unreadable` was told, or null if it was never called. */
function reported(): Record<string, string> | null {
  const call = invoke.mock.calls.find(([command]) => command === "note_unreadable");
  return call ? (call[1] as { failed: Record<string, string> }).failed : null;
}

beforeEach(() => {
  invoke.mockReset();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

describe("a read Rust refuses", () => {
  test("still gives the caller something to draw", async () => {
    invoke.mockImplementation((command: string) =>
      command === "search_engines" ? Promise.reject(new Error("denied")) : Promise.resolve(),
    );

    // The whole reason the fallback exists. A settings pane that throws part
    // way through leaves the rest of the window unbuilt.
    await expect(searchEngines()).resolves.toEqual([]);
  });

  test("says so, instead of passing the empty list off as the answer", async () => {
    invoke.mockImplementation((command: string) =>
      command === "search_engines" ? Promise.reject(new Error("denied")) : Promise.resolve(),
    );

    await searchEngines();

    expect(reported()).toMatchObject({
      surface: "settings",
      what: "the search engines",
      reason: "Error: denied",
      section: "sources",
    });
  });

  test("keeps the reason, which is the part that says what went wrong", async () => {
    invoke.mockImplementation((command: string) =>
      command === "browser_profiles"
        ? Promise.reject("not allowed by the capability")
        : Promise.resolve(),
    );

    await browserProfiles();

    // `.catch(() => [])` threw this away, which is what left an empty pane and
    // nothing anywhere saying why.
    expect(reported()?.reason).toBe("not allowed by the capability");
  });

  test("names the thing the way a sentence would, because Rust puts it in one", async () => {
    invoke.mockImplementation((command: string) =>
      command === "hotkey_conflicts" ? Promise.reject(new Error("denied")) : Promise.resolve(),
    );

    await hotkeyConflicts();

    expect(reported()?.what).toBe("which hotkeys another application already has");
  });

  test("a page of nothing is reported too, not just an empty list", async () => {
    invoke.mockImplementation((command: string) =>
      command === "index_rows" ? Promise.reject(new Error("denied")) : Promise.resolve(),
    );

    // The fallback here reads as "the index is empty", which is a sentence
    // about the machine rather than about the call that failed.
    await expect(indexRows("anything")).resolves.toEqual({ rows: [], total: 0 });
    expect(reported()?.what).toBe("what is in the index");
  });
});

describe("a read the launcher makes, not this window", () => {
  /**
   * The correction this file exists to hold onto. `navigation_chords` is asked
   * for by the launcher, so reporting it as a settings read put it in the
   * group the settings window clears when it opens: opening settings to read
   * the trouble would have been the act that erased it.
   *
   * It is silent rather than moved to the launcher's group, because an empty
   * chord map is not a lie anybody believes. The arrows and Enter are not
   * chords and keep working, so what is left is Ctrl+N doing nothing, which
   * the person pressing it finds out in the instant they press it.
   */
  test("is not reported as one of this window's", async () => {
    invoke.mockImplementation((command: string) =>
      command === "navigation_chords" ? Promise.reject(new Error("denied")) : Promise.resolve(),
    );

    await expect(navigationChords()).resolves.toEqual({});
    expect(reported()).toBeNull();
  });
});

describe("a read that works", () => {
  test("reports nothing at all", async () => {
    invoke.mockResolvedValue([{ id: "duck", name: "DuckDuckGo", url: "https://x/{query}" }]);

    await searchEngines();

    expect(reported()).toBeNull();
  });
});

describe("the surface itself", () => {
  /**
   * The one read that cannot report its own failure, because the surface is
   * what it failed to reach. It still must not throw: the settings window
   * calls it during mount, and a rejection there would leave the window with
   * no panels rather than with one missing message.
   */
  test("failing to read what is wrong is not itself fatal", async () => {
    invoke.mockRejectedValue(new Error("denied"));

    await expect(statusTroubles()).resolves.toEqual([]);
    expect(reported()).toBeNull();
  });
});
