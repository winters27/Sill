/**
 * Pictures out of an extension's assets: asked once, then known.
 *
 * The property that matters is the same one the application icon cache has:
 * a picture already fetched is readable without awaiting, so a row redrawn
 * on the next keystroke draws it on the first frame, and a picture not yet
 * fetched is distinguishable from "no picture" so the tile is reserved
 * rather than lettered.
 */
import { beforeEach, describe, expect, test, vi } from "vitest";

const invoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { extensionAsset, forgetAssets, knownAsset } = await import("$lib/exthost/assets");

beforeEach(() => {
  invoke.mockReset();
  forgetAssets();
});

describe("a picture beside the extension's code", () => {
  test("is unknown, then asked for once, then known without asking", async () => {
    invoke.mockResolvedValue("data:image/png;base64,AAAA");

    expect(knownAsset("s1", "files.png")).toBeUndefined();

    const first = extensionAsset("s1", "files.png");
    const second = extensionAsset("s1", "files.png");
    expect(await first).toBe("data:image/png;base64,AAAA");
    expect(await second).toBe("data:image/png;base64,AAAA");

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("extension_asset", { session: "s1", name: "files.png" });
    expect(knownAsset("s1", "files.png")).toBe("data:image/png;base64,AAAA");
  });

  test("a picture Rust has none of is known as none, and not asked again", async () => {
    invoke.mockResolvedValue(null);
    expect(await extensionAsset("s1", "gone.png")).toBeNull();
    expect(knownAsset("s1", "gone.png")).toBeNull();
    await extensionAsset("s1", "gone.png");
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  test("a refusal reads as no picture rather than as an error on the row", async () => {
    invoke.mockRejectedValue(new Error("no such session: s1"));
    expect(await extensionAsset("s1", "files.png")).toBeNull();
    expect(knownAsset("s1", "files.png")).toBeNull();
  });

  test("is keyed by session, since two extensions can both have a logo.png", async () => {
    invoke.mockResolvedValueOnce("data:one").mockResolvedValueOnce("data:two");
    expect(await extensionAsset("s1", "logo.png")).toBe("data:one");
    expect(await extensionAsset("s2", "logo.png")).toBe("data:two");
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  test("with no session there is nothing to ask", async () => {
    expect(knownAsset(null, "files.png")).toBeNull();
    expect(await extensionAsset(null, "files.png")).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });
});
