import { describe, expect, it } from "vitest";
import { answerFor } from "./quiet";

function press(
  key: string,
  held: { ctrl?: boolean; alt?: boolean; shift?: boolean; meta?: boolean } = {},
) {
  return {
    key,
    ctrlKey: held.ctrl ?? false,
    altKey: held.alt ?? false,
    shiftKey: held.shift ?? false,
    metaKey: held.meta ?? false,
  };
}

const HOTKEYS = new Set(["Alt+Space", "ContextMenu", "Ctrl+Alt+U"]);

describe("what a keydown is to a Sill window", () => {
  it("names one of Sill's hotkeys, whether it is a chord or a lone key", () => {
    expect(answerFor(press(" ", { alt: true }), HOTKEYS)).toEqual({ hotkey: "Alt+Space" });
    expect(answerFor(press("ContextMenu"), HOTKEYS)).toEqual({ hotkey: "ContextMenu" });
    expect(answerFor(press("u", { ctrl: true, alt: true }), HOTKEYS)).toEqual({
      hotkey: "Ctrl+Alt+U",
    });
  });

  it("is exact about the modifiers of a hotkey", () => {
    expect(answerFor(press("u", { alt: true }), HOTKEYS)).toBeNull();
    expect(answerFor(press("u", { ctrl: true, alt: true, shift: true }), HOTKEYS)).toBeNull();
    expect(answerFor(press(" "), HOTKEYS)).toBeNull();
  });

  it("calls the browser's own keys what they are", () => {
    for (const key of ["F12", "F5", "F3", "F7", "F11"]) {
      expect(answerFor(press(key), new Set())).toEqual({ browser: true });
    }
    expect(answerFor(press("F10", { shift: true }), new Set())).toEqual({ browser: true });
    expect(answerFor(press("r", { ctrl: true }), new Set())).toEqual({ browser: true });
    expect(answerFor(press("P", { ctrl: true }), new Set())).toEqual({ browser: true });
    expect(answerFor(press("i", { ctrl: true, shift: true }), new Set())).toEqual({ browser: true });
    expect(answerFor(press("ArrowLeft", { alt: true }), new Set())).toEqual({ browser: true });
  });

  it("the Menu key is the browser's unless Sill has it", () => {
    expect(answerFor(press("ContextMenu"), new Set())).toEqual({ browser: true });
    expect(answerFor(press("ContextMenu"), HOTKEYS)).toEqual({ hotkey: "ContextMenu" });
  });

  it("leaves typing and Sill's own keys alone", () => {
    expect(answerFor(press("a"), HOTKEYS)).toBeNull();
    expect(answerFor(press("a", { ctrl: true }), HOTKEYS)).toBeNull();
    expect(answerFor(press("Enter"), HOTKEYS)).toBeNull();
    expect(answerFor(press("Escape"), HOTKEYS)).toBeNull();
    expect(answerFor(press("ArrowLeft"), HOTKEYS)).toBeNull();
    expect(answerFor(press("k", { ctrl: true }), HOTKEYS)).toBeNull();
    expect(answerFor(press("r", { ctrl: true, alt: true }), HOTKEYS)).toBeNull();
  });

  it("a hotkey wins over a browser key on the same chord", () => {
    expect(answerFor(press("F5"), new Set(["F5"]))).toEqual({ hotkey: "F5" });
  });
});
