/**
 * Which reads say so when they fail, and which are right to stay quiet.
 *
 * The two decisions are the whole of this. `orElse` is for a failure that
 * leaves the window saying something untrue; `silently` is for one where the
 * fallback is the honest answer. Getting the second group wrong is not a
 * missing feature, it is a surface nobody reads: the value of a band that
 * lists what is broken is that it is empty almost always.
 *
 * The wrappers are exercised through the module that owns them rather than by
 * calling `orElse` alone, because the mistake this guards against is not
 * "`orElse` stopped working". It is somebody adding a wrapper and reaching for
 * `.catch(() => [])`, or moving one between windows and leaving the surface
 * behind.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { forgetUnreadable, orElse, silently } = await import("$lib/status");
const { clipboardCollections, clipboardCollectionEntries, clipboardLastSkipped } =
  await import("$lib/clipboard");
const { captureTargets } = await import("$lib/capture");
const { defaultBrowser, fileSearchMissing, listDrives, queryHistory, recordUse, searchEmoji } =
  await import("$lib/exthost/commands");

/** What `note_unreadable` was told, or null if it was never called. */
function reported(): Record<string, string> | null {
  const call = invoke.mock.calls.find(([command]) => command === "note_unreadable");
  return call ? (call[1] as { failed: Record<string, string> }).failed : null;
}

/** Makes exactly one command fail, so a report can only have come from it. */
function refuse(command: string) {
  invoke.mockImplementation((asked: string) =>
    asked === command ? Promise.reject(new Error("denied")) : Promise.resolve(),
  );
}

beforeEach(() => {
  invoke.mockReset();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

describe("a failure that leaves the window saying something untrue", () => {
  /**
   * The collections are somebody's own saved work. An empty rail is not a
   * neutral fallback, it is a claim that they never made any.
   */
  test("the clipboard collections are reported, and still draw", async () => {
    refuse("clipboard_collections");

    await expect(clipboardCollections()).resolves.toEqual([]);
    expect(reported()).toMatchObject({
      surface: "launcher",
      what: "the collections in the clipboard history",
      section: "clipboard",
    });
  });

  test("a collection that opens empty is reported", async () => {
    refuse("clipboard_collection_entries");

    await expect(clipboardCollectionEntries(1)).resolves.toEqual([]);
    expect(reported()?.what).toBe("what is in a clipboard collection");
  });

  /**
   * `null` from this one is a positive claim that nothing is standing in the
   * way of file search, which is the single answer that stops somebody who is
   * typing a filename and seeing nothing from looking any further.
   */
  test("nothing standing in the way of file search is reported when it is a failure", async () => {
    refuse("file_search_missing");

    await expect(fileSearchMissing()).resolves.toBeNull();
    expect(reported()).toMatchObject({
      surface: "launcher",
      what: "what is stopping file search from answering",
      section: "files",
    });
  });

  /** No machine has no drives, so an empty list is never the truth. */
  test("the drives are reported", async () => {
    refuse("list_drives");

    await expect(listDrives()).resolves.toEqual([]);
    expect(reported()?.surface).toBe("settings");
  });

  /**
   * The overlay draws an empty target list exactly as it draws a bare desk, so
   * clicking a window does nothing while the setting that offers it reads as
   * on.
   */
  test("the windows the capture overlay can offer are reported", async () => {
    refuse("capture_targets");

    await expect(captureTargets()).resolves.toEqual([]);
    expect(reported()).toMatchObject({
      surface: "capture",
      what: "which windows are on screen to capture",
    });
  });
});

describe("a failure where the fallback is the honest answer", () => {
  /**
   * These are the judgment, and they are the half that keeps the surface worth
   * reading. Each one is silent for its own reason, stated where it is
   * written, and a test that only proved the reporting half would let the next
   * person report all of them.
   */
  test.each([
    ["clipboard_last_skipped", () => clipboardLastSkipped(), null],
    ["query_history", () => queryHistory(), []],
    ["record_use", () => recordUse("sill:clipboard"), undefined],
    ["search_emoji", () => searchEmoji("smile"), []],
    ["default_browser", () => defaultBrowser(), null],
  ])("%s says nothing", async (command, call, fallback) => {
    refuse(command);

    await expect(call()).resolves.toEqual(fallback);
    expect(reported()).toBeNull();
  });
});

describe("which window is asking", () => {
  /**
   * Each window withdraws only its own reports. A flat group meant that
   * opening settings, or taking a screenshot, erased what the launcher had
   * found, and opening settings to read a trouble is exactly what somebody
   * does with one.
   */
  test("clearing names the window, so one does not erase another", async () => {
    invoke.mockResolvedValue(undefined);

    await forgetUnreadable("launcher");

    expect(invoke).toHaveBeenCalledWith("forget_unreadable", { surface: "launcher" });
  });

  test("a report carries the window that made it", async () => {
    invoke.mockResolvedValue(undefined);

    orElse("ask", "whether anything is set up to answer", null, "ai")(new Error("denied"));

    expect(reported()).toMatchObject({ surface: "ask", section: "ai" });
  });
});

describe("saying nothing", () => {
  test("hands back the fallback and reports it nowhere", () => {
    invoke.mockResolvedValue(undefined);

    expect(silently([])).toBeTypeOf("function");
    expect(silently("kept")(new Error("denied"))).toBe("kept");
    expect(reported()).toBeNull();
  });
});
