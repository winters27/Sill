import { describe, expect, it } from "vitest";
import { watchFocus } from "./chrome";

describe("watchFocus outside Tauri", () => {
  it("does nothing in a plain browser and hands back a stop that is safe to call", () => {
    // The preview routes render in an ordinary tab, where there is no window
    // to ask. The attribute must not appear and stopping must not throw.
    const stop = watchFocus();

    expect(document.documentElement.hasAttribute("data-window-blurred")).toBe(false);
    expect(() => stop()).not.toThrow();
    expect(document.documentElement.hasAttribute("data-window-blurred")).toBe(false);
  });
});
